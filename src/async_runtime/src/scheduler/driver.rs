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
#![allow(dead_code)]
use std::{ops::Deref, sync::Arc, task::Waker, time::Duration};

use foundation::prelude::FoundationAtomicU64;
use foundation::prelude::*;

use crate::{
    scheduler::{
        context::{ctx_get_handler, ctx_get_wakeup_time, ctx_get_worker_id, ctx_set_wakeup_time, ctx_unset_wakeup_time},
        scheduler_mt::AsyncScheduler,
        workers::worker_types::{WorkerInteractor, *},
    },
    time::{
        clock::{Clock, Instant},
        TimeDriver,
    },
};

#[derive(Clone)]
pub(crate) struct Drivers {
    inner: Arc<Inner>,
}

impl Drivers {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner::new()),
        }
    }
}

impl Deref for Drivers {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub(crate) struct Inner {
    time: TimeDriver,

    next_promised_wakeup: FoundationAtomicU64, // next time we promise to wakeup "some" worker
}

//TODO: This has to be reworked once we have IoDriver since those two are tightly coupled between each other
impl Inner {
    pub fn new() -> Self {
        Self {
            time: TimeDriver::new(4096),
            next_promised_wakeup: FoundationAtomicU64::new(0),
        }
    }

    // pub fn time_driver(&self) -> &TimeDriver {
    //     &self.time
    // }

    pub fn register_timeout(&self, expire_at: Instant, waker: Waker) -> Result<(), CommonErrors> {
        self.time.register_timeout(expire_at, waker)?;

        if ctx_get_worker_id().typ() == WorkerType::Dedicated {
            // Unparking needs to happen as all async workers may be parked so no one ever will look at this timeout
            ctx_get_handler().unwrap().unpark_some_async_worker();
        }

        Ok(())
    }

    pub fn process_work(&self) {
        self.time.process_timeouts(Clock::now());
    }

    pub fn park(&self, scheduler: &AsyncScheduler, worker: &WorkerInteractor) {
        let expire_time = self.time.next_process_time();

        if expire_time.is_none() {
            // No expiration is there, I sleep indefinitely
            debug!("No next expiration time, parking indefinitely");
            self.park_on_cv(scheduler, worker);
        } else {
            let expire_time_instant = expire_time.unwrap();
            let expire_time_u64 = self.time.instant_into_u64(expire_time_instant);

            // This is th last time we promised to wakeup this worker
            let previous_wakeup_time = self.get_last_wakeup_time_for_worker();
            let global_promis_next_time_wakeup = self.next_promised_wakeup.load(std::sync::atomic::Ordering::Relaxed);

            if (expire_time_u64 == global_promis_next_time_wakeup) && (previous_wakeup_time != expire_time_u64) {
                debug!("Someone else waiting on timewheel, we will park on cv without timeout");
                self.park_on_cv(scheduler, worker);
            } else {
                // We sleep to new timeout
                self.next_promised_wakeup.store(expire_time_u64, std::sync::atomic::Ordering::Relaxed);
                self.stash_promised_wakeup_time_for_worker(expire_time_u64);

                let dur = self.time.duration_since_now(expire_time_instant);

                self.park_on_cv_timeout(scheduler, worker, dur);
            }
        }

        self.time.process_timeouts(Clock::now());
    }

    fn park_on_cv(&self, scheduler: &AsyncScheduler, worker: &WorkerInteractor) {
        let worker_id = ctx_get_worker_id().worker_id() as usize;

        let mut guard = worker.mtx.lock().unwrap();

        match worker.state.0.compare_exchange(
            WORKER_STATE_EXECUTING,
            WORKER_STATE_SLEEPING_CV,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        ) {
            Ok(_) => {
                debug!("Definite sleep decision without timeout");
            }
            Err(WORKER_STATE_NOTIFIED) => {
                // We were notified before, so we shall continue
                scheduler.transition_from_parked(worker_id);

                worker.state.0.store(WORKER_STATE_EXECUTING, std::sync::atomic::Ordering::SeqCst);
                debug!("Notified while try to sleep, searching again");
                return;
            }
            Err(WORKER_STATE_SHUTTINGDOWN) => {
                // If we should shutdown, we simply need to return. And the run loop exits itself.
                return;
            }
            Err(s) => {
                panic!("Inconsistent state when parking: {}", s);
            }
        }

        loop {
            guard = worker.cv.wait(guard).unwrap();

            match worker.state.0.compare_exchange(
                WORKER_STATE_NOTIFIED,
                WORKER_STATE_EXECUTING,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            ) {
                Ok(_) => {
                    scheduler.transition_from_parked(worker_id);
                    debug!("Woken up from sleep");
                    break;
                }
                Err(WORKER_STATE_SHUTTINGDOWN) => {
                    // break here and run loop will exit
                    break;
                }
                Err(_) => {
                    continue; // spurious wake-up
                }
            }
        }
    }

    fn park_on_cv_timeout(&self, scheduler: &AsyncScheduler, worker: &WorkerInteractor, dur: Duration) {
        let worker_id = ctx_get_worker_id().worker_id() as usize;

        let mut guard = worker.mtx.lock().unwrap();

        match worker.state.0.compare_exchange(
            WORKER_STATE_EXECUTING,
            WORKER_STATE_SLEEPING_CV,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        ) {
            Ok(_) => {
                debug!("Definite sleep decision, sleeps next {} ms", dur.as_millis());
            }
            Err(WORKER_STATE_NOTIFIED) => {
                // We were notified before, so we shall continue
                scheduler.transition_from_parked(worker_id);

                worker.state.0.store(WORKER_STATE_EXECUTING, std::sync::atomic::Ordering::SeqCst);
                debug!("Notified while try to sleep, searching again");
                return;
            }
            Err(WORKER_STATE_SHUTTINGDOWN) => {
                // If we should shutdown, we simply need to return. And the run loop exits itself.
                return;
            }
            Err(s) => {
                panic!("Inconsistent state when parking: {}", s);
            }
        }

        loop {
            let wait_result;

            (guard, wait_result) = worker.cv.wait_timeout(guard, dur).unwrap();

            if wait_result.timed_out() {
                // We did timeout due to sleep request, so we fullfilled driver promise
                self.clear_last_wakeup_time_for_worker();
                worker.state.0.store(WORKER_STATE_EXECUTING, std::sync::atomic::Ordering::SeqCst);
                scheduler.transition_from_parked(worker_id);
                debug!("Woken up from sleep after timeout");
                break;
            } else {
                match worker.state.0.compare_exchange(
                    WORKER_STATE_NOTIFIED,
                    WORKER_STATE_EXECUTING,
                    std::sync::atomic::Ordering::SeqCst,
                    std::sync::atomic::Ordering::SeqCst,
                ) {
                    Ok(_) => {
                        scheduler.transition_from_parked(worker_id);
                        debug!("Woken up from sleep after timeout");
                        break;
                    }
                    Err(WORKER_STATE_SHUTTINGDOWN) => {
                        // break here and run loop will exit
                        break;
                    }
                    Err(_) => {
                        continue; // spurious wake-up
                    }
                }
            }
        }
    }

    fn stash_promised_wakeup_time_for_worker(&self, at: u64) {
        ctx_set_wakeup_time(at);
    }

    fn get_last_wakeup_time_for_worker(&self) -> u64 {
        ctx_get_wakeup_time()
    }

    fn clear_last_wakeup_time_for_worker(&self) {
        ctx_unset_wakeup_time();
    }
}
