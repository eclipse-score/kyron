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
use ::core::task::Context;
use std::sync::Arc;

use foundation::{containers::trigger_queue::TriggerQueue, threading::thread_wait_barrier::ThreadReadyNotifier};
use iceoryx2_bb_posix::thread::Thread;

use crate::{
    scheduler::{
        context::{ctx_initialize, ContextBuilder},
        driver::Drivers,
        scheduler_mt::{AsyncScheduler, DedicatedScheduler},
        task::async_task::TaskPollResult,
        waker::create_waker,
    },
    TaskRef,
};
use foundation::prelude::*;

use super::{spawn_thread, worker_types::WorkerId, ThreadParameters};

///
/// This is a factor which we will divide worker queue size to obtain size of local storage that is used to pop multiple items under single lock.
///
const LOCAL_STORAGE_SIZE_REDUCTION: usize = 8;

const SAFETY_QUEUE_SIZE: usize = 64; // For now hardcoded

pub(crate) struct SafetyWorker {
    thread_handle: Option<Thread>,
    id: WorkerId,
    queue: Arc<TriggerQueue<TaskRef>>,
    stop_signal: Arc<FoundationAtomicBool>,
}

impl SafetyWorker {
    pub(crate) fn new(id: WorkerId) -> Self {
        SafetyWorker {
            id,
            thread_handle: None,
            queue: Arc::new(TriggerQueue::new(SAFETY_QUEUE_SIZE)),
            stop_signal: Arc::new(FoundationAtomicBool::new(false)),
        }
    }

    pub(crate) fn get_queue(&self) -> Arc<TriggerQueue<TaskRef>> {
        self.queue.clone()
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
            let queue = self.queue.clone();
            let id = self.id;
            let local_size = queue.capacity() / LOCAL_STORAGE_SIZE_REDUCTION;
            let stop_signal = self.stop_signal.clone();

            Some(
                spawn_thread(
                    "sworker_",
                    &self.id,
                    move || {
                        let internal = WorkerInner {
                            queue,
                            local_storage: Vec::new(local_size),
                            id,
                            stop_signal,
                        };

                        Self::run_internal(internal, drivers, dedicated_scheduler, scheduler, ready_notifier);
                    },
                    thread_params,
                )
                .unwrap(),
            )
        };
    }

    fn run_internal(
        mut worker: WorkerInner,
        drivers: Drivers,
        dedicated_scheduler: Arc<DedicatedScheduler>,
        scheduler: Arc<AsyncScheduler>,
        ready_notifier: ThreadReadyNotifier,
    ) {
        worker.pre_run(drivers, dedicated_scheduler, scheduler);

        // Let the engine know what we are ready to handle tasks
        ready_notifier.ready();

        debug!("Safety worker {:?} started", worker.id.unique_id());
        worker.run();
    }
}

struct WorkerInner {
    queue: Arc<TriggerQueue<TaskRef>>,
    local_storage: Vec<TaskRef>,
    id: WorkerId,
    stop_signal: Arc<FoundationAtomicBool>,
}

impl WorkerInner {
    fn pre_run(&mut self, drivers: Drivers, dedicated_scheduler: Arc<DedicatedScheduler>, scheduler: Arc<AsyncScheduler>) {
        let builder = ContextBuilder::new(drivers)
            .thread_id(0)
            .with_dedicated_handle(scheduler, dedicated_scheduler)
            .with_worker_id(self.id)
            .with_safety();

        // Setup context
        ctx_initialize(builder);
    }

    fn run(&mut self) {
        let consumer = self
            .queue
            .get_consumer()
            .expect("There shall be consumer available as only we shall pick it");

        while !self.stop_signal.load(Ordering::Acquire) {
            while !self.local_storage.is_empty() {
                let task = self.local_storage.pop().unwrap(); // Since it was not empty, value must be there.
                self.run_task(task);
            }

            consumer.pop_into_vec(&mut self.local_storage);

            if !self.local_storage.is_empty() {
                // If we have new data available, continue processing
                continue;
            }

            match consumer.pop_blocking_with_timeout(::core::time::Duration::from_millis(100)) {
                Ok(task_ref) => {
                    self.local_storage.push(task_ref);
                }
                Err(CommonErrors::Timeout) => {
                    continue;
                }
                Err(_) => todo!(),
            }
        }
    }

    fn run_task(&mut self, task: TaskRef) {
        let waker = create_waker(task.clone());
        let mut ctx = Context::from_waker(&waker);
        match task.poll(&mut ctx) {
            TaskPollResult::Done => {
                // Literally nothing to do ;)
            }
            TaskPollResult::Notified => {
                // TODO: Think over if we rather shall use task.schedule() (which would not work right now)
                self.queue.push(task);
            }
        }
    }
}
