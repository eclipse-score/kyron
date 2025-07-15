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
use ::core::sync::atomic::Ordering;
use foundation::{
    containers::trigger_queue::TriggerQueueConsumer, prelude::FoundationAtomicBool, threading::thread_wait_barrier::ThreadReadyNotifier,
};
use std::sync::Arc;

use ::core::task::Context;

use crate::{
    scheduler::{
        context::{ctx_initialize, ContextBuilder},
        driver::Drivers,
        scheduler_mt::{AsyncScheduler, DedicatedScheduler},
        task::async_task::TaskPollResult,
        waker::create_waker,
        workers::{spawn_thread, Thread, ThreadParameters},
    },
    TaskRef,
};
use foundation::prelude::*;

use super::worker_types::WorkerId;

///
/// This is a factor which we will divide worker queue size to obtain size of local storage that is used to pop multiple items under single lock.
///
const LOCAL_STORAGE_SIZE_REDUCTION: usize = 8;

pub(crate) struct DedicatedWorker {
    thread_handle: Option<Thread>,
    id: WorkerId,
    engine_has_safety_worker: bool,
    stop_signal: Arc<FoundationAtomicBool>,
}

impl DedicatedWorker {
    pub(crate) fn new(id: WorkerId, engine_has_safety_worker: bool) -> Self {
        DedicatedWorker {
            id,
            thread_handle: None,
            engine_has_safety_worker,
            stop_signal: Arc::new(FoundationAtomicBool::new(false)),
        }
    }

    pub(crate) fn stop(&self) {
        self.stop_signal.store(true, Ordering::Release);
    }

    pub(crate) fn start(
        &mut self,
        scheduler: Arc<AsyncScheduler>,
        drivers: Drivers,
        dedicated_scheduler: Arc<DedicatedScheduler>,
        ready_notifier: ThreadReadyNotifier,
        thread_params: &ThreadParameters,
    ) {
        self.thread_handle = {
            let queue = self.get_queue(&dedicated_scheduler);
            let id = self.id;
            let local_size = queue.capacity() / LOCAL_STORAGE_SIZE_REDUCTION;
            let with_safety = self.engine_has_safety_worker;
            let stop_signal = self.stop_signal.clone();

            Some(
                spawn_thread(
                    "dworker_",
                    &self.id,
                    move || {
                        let internal = WorkerInner {
                            dedicated_scheduler,
                            consumer: queue,
                            local_storage: Vec::new(local_size),
                            id,
                            stop_signal,
                        };

                        Self::run_internal(internal, drivers, scheduler, ready_notifier, with_safety);
                    },
                    thread_params,
                )
                .unwrap(),
            )
        };
    }

    fn get_queue(&self, dedicated_scheduler: &Arc<DedicatedScheduler>) -> TriggerQueueConsumer<TaskRef> {
        dedicated_scheduler
            .dedicated_queues
            .iter()
            .find(|(id, _)| *id == self.id)
            .expect("The queue for the worker has to be provided")
            .1
            .get_consumer()
            .expect("There shall be consumer available as only we shall pick it")
    }

    fn run_internal(
        mut worker: WorkerInner,
        drivers: Drivers,
        scheduler: Arc<AsyncScheduler>,
        ready_notifier: ThreadReadyNotifier,
        with_safety: bool,
    ) {
        worker.pre_run(drivers, scheduler, with_safety);

        // Let the engine know what we are ready to handle tasks
        ready_notifier.ready();

        debug!("Dedicated worker {:?} started", worker.id.unique_id());
        worker.run();
    }
}

struct WorkerInner {
    dedicated_scheduler: Arc<DedicatedScheduler>,
    consumer: TriggerQueueConsumer<TaskRef>,
    local_storage: Vec<TaskRef>,
    id: WorkerId,
    stop_signal: Arc<FoundationAtomicBool>,
}

impl WorkerInner {
    fn pre_run(&mut self, drivers: Drivers, scheduler: Arc<AsyncScheduler>, with_safety: bool) {
        let mut builder = ContextBuilder::new(drivers)
            .thread_id(0)
            .with_dedicated_handle(scheduler, self.dedicated_scheduler.clone())
            .with_worker_id(self.id);

        if with_safety {
            builder = builder.with_safety();
        }

        // Setup context
        ctx_initialize(builder);
    }

    fn run(&mut self) {
        while !self.stop_signal.load(Ordering::Acquire) {
            while !self.local_storage.is_empty() {
                let task = self.local_storage.pop().unwrap(); // Since it was not empty, value must be there.
                self.run_task(task);
            }

            self.consumer.pop_into_vec(&mut self.local_storage);

            if !self.local_storage.is_empty() {
                // If we have new data available, continue processing
                continue;
            }

            match self.consumer.pop_blocking_with_timeout(::core::time::Duration::from_millis(100)) {
                Ok(task_ref) => {
                    self.local_storage.push(task_ref);
                }
                Err(CommonErrors::Timeout) => {
                    continue;
                }
                Err(_) => todo!(),
            }
        }
        debug!("Dedicated worker {:?} stopping", self.id.unique_id());
    }

    fn run_task(&mut self, task: TaskRef) {
        let waker = create_waker(task.clone());
        let mut ctx = Context::from_waker(&waker);
        match task.poll(&mut ctx) {
            TaskPollResult::Done => {
                // Literally nothing to do ;)
            }
            TaskPollResult::Notified => {
                // For now stupid respawn
                self.dedicated_scheduler.spawn(task, self.id.unique_id());
            }
        }
    }
}
