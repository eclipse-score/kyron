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

// TODO: To be removed once used in IO APIs
#[allow(dead_code)]
use ::core::{ops::Deref, task::Waker};
use std::sync::Arc;

use crate::macros::log::*;
use kyron_foundation::prelude::{CommonErrors, FoundationAtomicU64, ScopeGuardBuilder};

use crate::io::driver::IoDriver;
use crate::io::AsyncSelector;
use crate::{
    scheduler::{
        context::{
            ctx_get_handler, ctx_get_wakeup_time, ctx_get_worker_id, ctx_set_wakeup_time, ctx_unset_wakeup_time,
        },
        scheduler_mt::AsyncScheduler,
        workers::worker_types::{WorkerInteractor, *},
    },
    time::{clock::Instant, TimeDriver},
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
    io: IoDriver,

    next_promised_wakeup: FoundationAtomicU64, // next time we promise to wakeup "some" worker
}

impl Inner {
    pub fn new() -> Self {
        // TODO: Once tackling all configuration points in runtime, expose this
        let selector = AsyncSelector::new(2048);
        Self {
            time: TimeDriver::new(4096),
            next_promised_wakeup: FoundationAtomicU64::new(0),
            io: IoDriver::new(selector),
        }
    }

    pub fn get_io_driver(&self) -> &IoDriver {
        &self.io
    }

    pub fn register_timeout(&self, expire_at: Instant, waker: Waker) -> Result<(), CommonErrors> {
        self.time.register_timeout(expire_at, waker)?;

        if ctx_get_worker_id().typ() == WorkerType::Dedicated {
            // Unparking needs to happen as all async workers may be parked so no one ever will look at this timeout
            ctx_get_handler().unwrap().unpark_some_async_worker();
        }

        Ok(())
    }

    pub fn process_work(&self) {
        self.time.process_timeouts();
    }

    /// Parks the current worker until the next timeout needs to be processed. Returns true if the worker was actually parked, false otherwise.
    pub fn park(&self, scheduler: &AsyncScheduler, worker: &WorkerInteractor) {
        let _exit_guard = ScopeGuardBuilder::new(true)
            .on_init(|_| Ok(()) as Result<(), ()>)
            .on_drop(|_| {
                self.time.process_timeouts();
            })
            .create()
            .unwrap();

        // TODO: Think over if we want to mark globally that someone is right now sleeping on timer
        // Then logic with this time calculations shall not be needed

        // This is th last time we promised to wakeup this worker
        let previous_wakeup_time = self.get_last_wakeup_time_for_worker();
        let global_promis_next_time_wakeup = self.next_promised_wakeup.load(::core::sync::atomic::Ordering::Relaxed);

        // If global promised time is the same as our last promised time it means it was us who promised sleep, so we need to
        // make sure that we take a lock and don't race it with other worker, otherwise no one will sleep on time
        let is_doing_resleep = previous_wakeup_time == global_promis_next_time_wakeup;

        let expire_time = self.time.next_process_time(is_doing_resleep);

        if expire_time.is_none() {
            // No expiration is there, I sleep indefinitely
            let _ = worker.park(scheduler, &self.io, || {}, None);
            return;
        }

        let expire_time_instant = expire_time.unwrap();
        let expire_time_u64 = self.time.instant_into_u64(&expire_time_instant);

        if (expire_time_u64 == global_promis_next_time_wakeup) && (previous_wakeup_time != expire_time_u64) {
            debug!("Someone else waiting on timewheel, we will park without timeout");
            let _ = worker.park(scheduler, &self.io, || {}, None);
            return;
        }

        self.park_with_timeout(scheduler, worker, &expire_time_instant, expire_time_u64);
    }

    fn park_with_timeout(
        &self,
        scheduler: &AsyncScheduler,
        worker: &WorkerInteractor,
        expire_time_instant: &Instant,
        expire_time_u64: u64,
    ) {
        let res = worker.park(
            scheduler,
            &self.io,
            || {
                // We sleep to new timeout
                self.next_promised_wakeup
                    .store(expire_time_u64, ::core::sync::atomic::Ordering::Relaxed);
                self.stash_promised_wakeup_time_for_worker(expire_time_u64);
            },
            Some(self.time.duration_since_now(expire_time_instant)),
        );

        if let Err(CommonErrors::Timeout) = res {
            // We did timeout due to sleep request, so we fulfilled driver promise
            self.clear_last_wakeup_time_for_worker();
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
