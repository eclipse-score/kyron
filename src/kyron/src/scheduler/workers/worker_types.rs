// *******************************************************************************
// Copyright (c) 2026 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// <https://www.apache.org/licenses/LICENSE-2.0>
//
// SPDX-License-Identifier: Apache-2.0
// *******************************************************************************

use crate::common::types::UniqueWorkerId;
use crate::io::driver::{IoDriver, IoDriverUnparker};
use crate::macros::log::*;
use crate::scheduler::scheduler_mt::AsyncScheduler;
use crate::scheduler::task::async_task::TaskRef;
use ::core::ops::Deref;
use kyron_foundation::containers::spmc_queue::*;
use kyron_foundation::{
    not_recoverable_error,
    prelude::{CommonErrors, FoundationAtomicU8},
};
use std::sync::Arc;

use ::core::sync::atomic::Ordering;

pub type TaskStealQueue = Arc<SpmcStealQueue<TaskRef>>;
pub type StealHandle = TaskStealQueue;

pub fn create_steal_queue(size: u32) -> TaskStealQueue {
    Arc::new(SpmcStealQueue::new(size))
}

pub(in super::super) const WORKER_STATE_SLEEPING_CV: u8 = 0b00000000;
pub(in super::super) const WORKER_STATE_SLEEPING_IO: u8 = 0b00000010;
pub(in super::super) const WORKER_STATE_NOTIFIED: u8 = 0b00000001; // Was asked to wake-up
pub(in super::super) const WORKER_STATE_EXECUTING: u8 = 0b00000011;
pub(in super::super) const WORKER_STATE_SHUTTINGDOWN: u8 = 0b00000100;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct WorkerId {
    unique_id: UniqueWorkerId,
    engine_id: u8,   // in which engine worker is working
    worker_id: u8,   // whats the worker id in this engine
    typ: WorkerType, // whats the type
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum WorkerType {
    Async,
    Dedicated,
}

impl WorkerId {
    pub(crate) fn new(unique_id: UniqueWorkerId, engine_id: u8, worker_id: u8, typ: WorkerType) -> Self {
        Self {
            unique_id,
            engine_id,
            worker_id,
            typ,
        }
    }

    // Getters for all fields
    pub fn unique_id(&self) -> UniqueWorkerId {
        self.unique_id
    }

    pub fn engine_id(&self) -> u8 {
        self.engine_id
    }

    pub fn worker_id(&self) -> u8 {
        self.worker_id
    }

    pub fn typ(&self) -> WorkerType {
        self.typ
    }
}

#[derive(Clone)]
pub(crate) struct WorkerInteractor {
    inner: Arc<WorkerInteractorInnner>,
}

impl Deref for WorkerInteractor {
    type Target = WorkerInteractorInnner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub(crate) struct WorkerState(pub FoundationAtomicU8);

impl WorkerState {
    pub(crate) fn new(val: u8) -> Self {
        Self(FoundationAtomicU8::new(val))
    }
}

pub(crate) struct WorkerInteractorInnner {
    pub(crate) steal_handle: StealHandle,

    worker_id: WorkerId,
    state: WorkerState,
    mtx: std::sync::Mutex<()>,
    cv: std::sync::Condvar,
}

pub trait ParkerTrait {
    /// Park the current worker.
    ///
    /// The `after_park_decision` closure is called after the worker decided that it will for sure TRY to park (so try to hang on some parking primitive)
    ///
    /// Return `Ok` when parking was successful and the worker can continue its work, error otherwise
    /// - `Timeout` - when the park timed out
    ///
    fn park<AfterParkDecision>(
        &self,
        scheduler: &AsyncScheduler,
        driver: &IoDriver,
        after_park_decision: AfterParkDecision,
        timeout: Option<core::time::Duration>,
    ) -> Result<(), CommonErrors>
    where
        AfterParkDecision: FnOnce();

    /// Unpark the worker, making it runnable again.
    fn unpark(&self, driver: &IoDriverUnparker);
}

impl ParkerTrait for WorkerInteractor {
    fn park<AfterParkDecision>(
        &self,
        scheduler: &AsyncScheduler,
        driver: &IoDriver,
        after_park_decision: AfterParkDecision,
        timeout: Option<core::time::Duration>,
    ) -> Result<(), CommonErrors>
    where
        AfterParkDecision: FnOnce(),
    {
        // Do not sleep if duration is 0 but still do handle state transitions correctly
        if timeout.as_ref().is_some_and(|dur| dur.is_zero()) {
            self.move_to_executing(scheduler);
            return Ok(());
        }

        if let Some(mut io) = driver.try_get_access() {
            let mut _guard = self.mtx.lock().unwrap();

            // Needs to be called under lock
            unsafe {
                if !self.park_pre_stage(scheduler, WORKER_STATE_SLEEPING_IO) {
                    // We were notified before we could park, so we just return
                    return Ok(());
                }
            }

            // Call user code if he needs it under specific conditions
            after_park_decision();
            drop(_guard); // We need to drop the lock before going into IO wait

            debug!("Parking on IO Driver with timeout: {:?}", timeout);
            let ret = driver.process_io(&mut io, timeout);

            // TODO : Consider that in if this case we need the lock
            _guard = self.mtx.lock().unwrap();

            // Make sure our state is consistent after we come back from IO wait
            self.move_to_executing(scheduler);
            debug!("Unparked on IO Driver with result: {}", &format!("{:?}", ret));

            ret
        } else {
            // We have not get IoHandle to sleep, so we sleep on internal CV
            self.park_internal(scheduler, after_park_decision, timeout)
        }
    }

    fn unpark(&self, driver: &IoDriverUnparker) {
        self.unpark_internal(driver);
    }
}

impl WorkerInteractor {
    pub fn new(handle: StealHandle, worker_id: WorkerId) -> Self {
        Self {
            inner: Arc::new(WorkerInteractorInnner {
                mtx: std::sync::Mutex::new(()),
                cv: std::sync::Condvar::new(),
                steal_handle: handle,
                state: WorkerState::new(WORKER_STATE_EXECUTING),
                worker_id,
            }),
        }
    }

    pub fn request_stop(&self, io: &IoDriverUnparker) {
        let _guard = self.mtx.lock().unwrap();
        // Set the state to shutting down
        let prev = self
            .state
            .0
            .swap(WORKER_STATE_SHUTTINGDOWN, ::core::sync::atomic::Ordering::SeqCst);
        if prev == WORKER_STATE_SLEEPING_CV {
            self.cv.notify_one();
        } else if prev == WORKER_STATE_SLEEPING_IO {
            io.unpark();
        }
    }

    pub fn is_shutdown_requested(&self) -> bool {
        self.state.0.load(::core::sync::atomic::Ordering::Acquire) == WORKER_STATE_SHUTTINGDOWN
    }

    fn park_internal<AfterParkDecision>(
        &self,
        scheduler: &AsyncScheduler,
        after_park_decision: AfterParkDecision,
        timeout: Option<core::time::Duration>,
    ) -> Result<(), CommonErrors>
    where
        AfterParkDecision: FnOnce(),
    {
        if let Some(timeout) = timeout {
            self.park_on_cv_timeout(scheduler, timeout, after_park_decision)
        } else {
            self.park_on_cv(scheduler, after_park_decision)
        }
    }

    fn unpark_internal(&self, driver: &IoDriverUnparker) {
        match self.state.0.swap(WORKER_STATE_NOTIFIED, Ordering::SeqCst) {
            WORKER_STATE_NOTIFIED => {
                //Nothing to do, already someone did if for us
            },
            WORKER_STATE_SLEEPING_CV => {
                drop(self.mtx.lock().unwrap()); // Synchronize so worker does not lose the notification in before it goes into a wait
                self.cv.notify_one(); // notify without lock in case we get preempted by woken thread
                trace!("Unparked worker sleeping on CV");
            },
            WORKER_STATE_SLEEPING_IO => {
                driver.unpark();
                trace!("Unparked worker sleeping on IO");
            },
            WORKER_STATE_EXECUTING => {
                //Nothing to do, looks like we already running
            },
            WORKER_STATE_SHUTTINGDOWN => {
                // Put back SHUTTINGDOWN state, so we can shutdown properly
                self.state.0.store(WORKER_STATE_SHUTTINGDOWN, Ordering::SeqCst);
            },
            _ => {
                panic!("Inconsistent/not handled state when unparking worker!")
            },
        };
    }

    fn park_on_cv(&self, scheduler: &AsyncScheduler, after_park_decision: impl FnOnce()) -> Result<(), CommonErrors> {
        let mut _guard = self.mtx.lock().unwrap();

        // Needs to be called under lock
        unsafe {
            if !self.park_pre_stage(scheduler, WORKER_STATE_SLEEPING_CV) {
                // We were notified before we could park, so we just return
                return Ok(());
            }
        }

        // Call user code if he needs it under specific conditions
        after_park_decision();

        debug!("Parking on CV without timeout");

        _guard = self
            .cv
            .wait_while(
                _guard,
                // Needs to be called under lock
                |_| unsafe { self.park_condition_check_and_transition(scheduler, "from park_on_cv") },
            )
            .unwrap();
        Ok(())
    }

    #[allow(clippy::to_string_in_format_args)] // to_string() is needed to avoid implementing ScoreDebug for simple cases.
    fn park_on_cv_timeout(
        &self,
        scheduler: &AsyncScheduler,
        dur: core::time::Duration,
        after_park_decision: impl FnOnce(),
    ) -> Result<(), CommonErrors> {
        let mut _guard = self.mtx.lock().unwrap();

        // Needs to be called under lock
        unsafe {
            if !self.park_pre_stage(scheduler, WORKER_STATE_SLEEPING_CV) {
                // We were notified before we could park, so we just return
                return Ok(());
            }
        }

        // Call user code if he needs it under specific conditions
        after_park_decision();

        debug!("Parking on CV with timeout {} ms", dur.as_millis().to_string());

        let wait_result;

        (_guard, wait_result) = self
            .cv
            .wait_timeout_while(
                _guard,
                dur,
                // Needs to be called under lock
                |_| unsafe { self.park_condition_check_and_transition(scheduler, "from park_on_cv_timeout") },
            )
            .unwrap();

        if wait_result.timed_out() {
            // Since timeout is here, we need to make sure we went off the sleep
            self.move_to_executing(scheduler);
            debug!("Unparked from CV after timeout");
            Err(CommonErrors::Timeout)
        } else {
            Ok(())
        }
    }

    /// # Safety
    /// This function must be called under mutex lock of the worker.
    unsafe fn park_pre_stage(&self, scheduler: &AsyncScheduler, sleep_reason: u8) -> bool {
        match self.state.0.compare_exchange(
            WORKER_STATE_EXECUTING,
            sleep_reason,
            ::core::sync::atomic::Ordering::SeqCst,
            ::core::sync::atomic::Ordering::SeqCst,
        ) {
            Ok(_) => true,
            Err(WORKER_STATE_NOTIFIED) => {
                // We were notified before, so we shall continue
                scheduler.transition_from_parked(self.worker_id);

                self.state
                    .0
                    .store(WORKER_STATE_EXECUTING, ::core::sync::atomic::Ordering::SeqCst);
                debug!("Notified while try to sleep, searching again");
                false
            },
            Err(WORKER_STATE_SHUTTINGDOWN) => {
                // If we should shutdown, we simply need to return. And the run loop exits itself.
                false
            },
            Err(s) => {
                not_recoverable_error!(with s, "Inconsistent state when parking");
            },
        }
    }

    /// # Safety
    /// This function must be called under mutex lock of the worker.
    ///
    /// This function takes care to transition the worker state from NOTIFIED to EXECUTING, sync scheduler
    /// and return `true` if state was not NOTIFIED or SHUTTINGDOWN
    unsafe fn park_condition_check_and_transition(&self, scheduler: &AsyncScheduler, info: &str) -> bool {
        match self.state.0.compare_exchange(
            WORKER_STATE_NOTIFIED,
            WORKER_STATE_EXECUTING,
            ::core::sync::atomic::Ordering::SeqCst,
            ::core::sync::atomic::Ordering::SeqCst,
        ) {
            Ok(_) => {
                scheduler.transition_from_parked(self.worker_id);
                debug!("Unparked from CV({}) by notification", info);
                false
            },
            Err(WORKER_STATE_SHUTTINGDOWN) => {
                // break here and run loop will exit
                false
            },
            Err(_) => {
                true // spurious wake-up, or any other wakeup
            },
        }
    }

    fn move_to_executing(&self, scheduler: &AsyncScheduler) {
        let prev = self
            .state
            .0
            .swap(WORKER_STATE_EXECUTING, ::core::sync::atomic::Ordering::SeqCst);

        if prev == WORKER_STATE_SHUTTINGDOWN {
            // Put back SHUTTINGDOWN state, so we can shutdown properly
            self.state.0.store(WORKER_STATE_SHUTTINGDOWN, Ordering::SeqCst);
        } else {
            scheduler.transition_from_parked(self.worker_id);
        }
    }
}

#[cfg(test)]
#[cfg(not(loom))]
#[cfg(not(miri))]
mod tests {
    use crate::io::AsyncSelector;

    use super::*;

    #[test]
    fn test_worker_shutdown_state() {
        let queue = create_steal_queue(4);
        let worker = WorkerInteractor::new(queue, WorkerId::new("1".into(), 0, 0, WorkerType::Async));
        let io = IoDriver::new(AsyncSelector::new(128));

        // Set state to SHUTTINGDOWN
        worker.state.0.store(WORKER_STATE_SHUTTINGDOWN, Ordering::SeqCst);

        // Call unpark and ensure we do not panic and state remains SHUTTINGDOWN
        worker.unpark(&io.get_unparker());
        let state = worker.state.0.load(Ordering::SeqCst);
        assert_eq!(state, WORKER_STATE_SHUTTINGDOWN);
    }

    #[test]
    fn test_worker_unpark_from_sleeping_cv() {
        let queue = create_steal_queue(4);
        let worker = WorkerInteractor::new(queue, WorkerId::new("1".into(), 0, 0, WorkerType::Async));
        let io = IoDriver::new(AsyncSelector::new(128));
        // Set state to SLEEPING_CV
        worker.state.0.store(WORKER_STATE_SLEEPING_CV, Ordering::SeqCst);

        // Call unpark and ensure state is set to NOTIFIED
        worker.unpark(&io.get_unparker());
        let state = worker.state.0.load(Ordering::SeqCst);
        assert_eq!(state, WORKER_STATE_NOTIFIED);
    }

    #[test]
    fn test_worker_unpark_from_executing() {
        let queue = create_steal_queue(4);
        let worker = WorkerInteractor::new(queue, WorkerId::new("1".into(), 0, 0, WorkerType::Async));
        let io = IoDriver::new(AsyncSelector::new(128));
        // State is EXECUTING by default
        worker.unpark(&io.get_unparker());
        let state = worker.state.0.load(Ordering::SeqCst);
        // Should be NOTIFIED after unpark
        assert_eq!(state, WORKER_STATE_NOTIFIED);
    }

    #[test]
    #[should_panic(expected = "Inconsistent/not handled state when unparking worker!")]
    fn test_worker_unpark_invalid_state() {
        let queue = create_steal_queue(4);
        let worker = WorkerInteractor::new(queue, WorkerId::new("1".into(), 0, 0, WorkerType::Async));
        let io = IoDriver::new(AsyncSelector::new(128));
        // Set to an invalid state
        worker.state.0.store(0xFF, Ordering::SeqCst);
        worker.unpark(&io.get_unparker());
    }
}
