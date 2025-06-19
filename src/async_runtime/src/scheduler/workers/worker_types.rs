//
// Copyright (c) 2025 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// <https://www.apache.org/licenses/LICENSE-2.0>
//
// SPDX-License-Identifier: Apache-2.0
//

use crate::core::types::UniqueWorkerId;
use crate::scheduler::task::async_task::TaskRef;
use foundation::containers::spmc_queue::*;
use foundation::prelude::*;
use std::ops::Deref;
use std::sync::{atomic::Ordering, Arc};

pub type TaskStealQueue = Arc<SpmcStealQueue<TaskRef>>;
pub type StealHandle = TaskStealQueue;

pub fn create_steal_queue(size: usize) -> TaskStealQueue {
    Arc::new(SpmcStealQueue::new(size as u32))
}

pub(in super::super) const WORKER_STATE_SLEEPING_CV: u8 = 0b00000000;
pub(in super::super) const WORKER_STATE_NOTIFIED: u8 = 0b00000001; // Was asked to wake-up
pub(in super::super) const WORKER_STATE_EXECUTING: u8 = 0b00000011;
pub(in super::super) const WORKER_STATE_SHUTTINGDOWN: u8 = 0b00000100;

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) struct WorkerId {
    unique_id: UniqueWorkerId,
    engine_id: u8,   // in which engine worker is working
    worker_id: u8,   // whats the worker id in this engine
    typ: WorkerType, // whats the type
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum WorkerType {
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
    pub(crate) fn unique_id(&self) -> UniqueWorkerId {
        self.unique_id
    }

    #[allow(dead_code)]
    pub(crate) fn engine_id(&self) -> u8 {
        self.engine_id
    }

    pub(crate) fn worker_id(&self) -> u8 {
        self.worker_id
    }

    #[allow(dead_code)]
    pub(crate) fn typ(&self) -> WorkerType {
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

unsafe impl Send for WorkerInteractor {}

pub(crate) struct WorkerState(pub FoundationAtomicU8);

impl WorkerState {
    pub(crate) fn new(val: u8) -> Self {
        Self(FoundationAtomicU8::new(val))
    }
}

pub(crate) struct WorkerInteractorInnner {
    pub(crate) steal_handle: StealHandle,

    pub(crate) state: WorkerState,
    pub(crate) mtx: std::sync::Mutex<()>,
    pub(crate) cv: std::sync::Condvar,
}

impl WorkerInteractor {
    pub fn new(handle: StealHandle) -> Self {
        Self {
            inner: Arc::new(WorkerInteractorInnner {
                mtx: std::sync::Mutex::new(()),
                cv: std::sync::Condvar::new(),
                steal_handle: handle,
                state: WorkerState::new(WORKER_STATE_EXECUTING),
            }),
        }
    }

    pub(crate) fn unpark(&self) {
        match self.state.0.swap(WORKER_STATE_NOTIFIED, Ordering::SeqCst) {
            WORKER_STATE_NOTIFIED => {
                //Nothing to do, already someone did if for us
            }
            WORKER_STATE_SLEEPING_CV => {
                drop(self.mtx.lock().unwrap()); // Synchronize so worker does not lose the notification in before it goes into a wait
                self.cv.notify_one(); // notify without lock in case we get preempted by woken thread
            }
            WORKER_STATE_EXECUTING => {
                //Nothing to do, looks like we already running
            }
            WORKER_STATE_SHUTTINGDOWN => {
                // Put back SHUTTINGDOWN state, so we can shutdown properly
                self.state.0.store(WORKER_STATE_SHUTTINGDOWN, Ordering::SeqCst);
            }
            _ => {
                panic!("Inconsistent/not handled state when unparking worker!")
            }
        };
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use super::*;

    #[test]
    fn test_worker_shutdown_state() {
        let queue = create_steal_queue(4);
        let worker = WorkerInteractor::new(queue);

        // Set state to SHUTTINGDOWN
        worker.state.0.store(WORKER_STATE_SHUTTINGDOWN, Ordering::SeqCst);

        // Call unpark and ensure we do not panic and state remains SHUTTINGDOWN
        worker.unpark();
        let state = worker.state.0.load(Ordering::SeqCst);
        assert_eq!(state, WORKER_STATE_SHUTTINGDOWN);
    }

    #[test]
    fn test_worker_unpark_from_sleeping_cv() {
        let queue = create_steal_queue(4);
        let worker = WorkerInteractor::new(queue);

        // Set state to SLEEPING_CV
        worker.state.0.store(WORKER_STATE_SLEEPING_CV, Ordering::SeqCst);

        // Call unpark and ensure state is set to NOTIFIED
        worker.unpark();
        let state = worker.state.0.load(Ordering::SeqCst);
        assert_eq!(state, WORKER_STATE_NOTIFIED);
    }

    #[test]
    fn test_worker_unpark_from_executing() {
        let queue = create_steal_queue(4);
        let worker = WorkerInteractor::new(queue);

        // State is EXECUTING by default
        worker.unpark();
        let state = worker.state.0.load(Ordering::SeqCst);
        // Should be NOTIFIED after unpark
        assert_eq!(state, WORKER_STATE_NOTIFIED);
    }

    #[test]
    #[should_panic(expected = "Inconsistent/not handled state when unparking worker!")]
    fn test_worker_unpark_invalid_state() {
        let queue = create_steal_queue(4);
        let worker = WorkerInteractor::new(queue);

        // Set to an invalid state
        worker.state.0.store(0xFF, Ordering::SeqCst);
        worker.unpark();
    }
}
