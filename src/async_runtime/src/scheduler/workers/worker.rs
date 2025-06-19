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

use core::task::Context;
use std::{rc::Rc, sync::Arc};

use crate::scheduler::{context::ctx_get_drivers, driver::Drivers, scheduler_mt::DedicatedScheduler, waker::create_waker, workers::Thread};
use foundation::base::fast_rand::FastRand;
use foundation::containers::spmc_queue::BoundProducerConsumer;
use foundation::prelude::*;
use foundation::threading::thread_wait_barrier::ThreadReadyNotifier;

use crate::scheduler::{
    context::{ctx_get_worker_id, ctx_initialize, ContextBuilder},
    scheduler_mt::AsyncScheduler,
    task::async_task::*,
    workers::{spawn_thread, ThreadParameters},
};

use super::worker_types::*;

pub const FIRST_WORKER_ID: u8 = 0;

// The facade to represent this in runtime
pub(crate) struct Worker {
    thread_handle: Option<Thread>,
    id: WorkerId,
    engine_has_safety_worker: bool,
    scheduler: Option<Arc<AsyncScheduler>>,
}

#[derive(PartialEq)]
enum LocalState {
    Searching,
    Executing,
}

// the actual impl
struct WorkerInner {
    own_interactor: WorkerInteractor,
    producer_consumer: Rc<BoundProducerConsumer<TaskRef>>,
    scheduler: Arc<AsyncScheduler>,

    local_state: LocalState, // small optimization to not touch global atomic state if we don't  really need
    id: WorkerId,
    randomness_source: FastRand,

    next_task_tick: u64,
}

///
/// Async Worker implementation
///
/// TODO:
///     - shutdown
///     - join logic
///     - prio & affinity
///     - migrate to iceoryxbb2 once we know details
///     - ....
///
///
impl Worker {
    pub(crate) fn new(_prio: Option<u8>, id: WorkerId, engine_has_safety_worker: bool) -> Self {
        Self {
            thread_handle: None,
            id,
            engine_has_safety_worker,
            scheduler: None,
        }
    }

    pub(crate) fn start(
        &mut self,
        scheduler: Arc<AsyncScheduler>,
        drivers: Drivers,
        dedicated_scheduler: Arc<DedicatedScheduler>,
        ready_notifier: ThreadReadyNotifier,
        thread_params: &ThreadParameters,
    ) {
        self.scheduler = Some(scheduler.clone());
        self.thread_handle = {
            let interactor = scheduler.worker_access[self.id.worker_id() as usize].clone();
            let id = self.id;
            let with_safety = self.engine_has_safety_worker;

            // Entering a thread
            let thread = spawn_thread(
                "aworker_",
                &self.id,
                move || {
                    let prod_consumer = interactor.steal_handle.get_boundedl().unwrap();

                    let internal = WorkerInner {
                        own_interactor: interactor,
                        local_state: LocalState::Executing,
                        scheduler: scheduler.clone(),
                        id,
                        producer_consumer: Rc::new(prod_consumer),
                        randomness_source: FastRand::new(82382389432984 / (id.worker_id() as u64 + 1)), // Random seed for now as const
                        next_task_tick: 0,
                    };

                    Self::run_internal(internal, drivers, dedicated_scheduler, ready_notifier, with_safety);
                },
                thread_params,
            )
            .unwrap();
            Some(thread)
        };
    }

    fn run_internal(
        mut worker: WorkerInner,
        drivers: Drivers,
        dedicated_scheduler: Arc<DedicatedScheduler>,
        ready_notifier: ThreadReadyNotifier,
        with_safety: bool,
    ) {
        worker.pre_run(drivers, dedicated_scheduler, with_safety);

        // Let the engine know what we are ready to handle tasks
        ready_notifier.ready();

        worker.run();
    }

    pub(crate) fn stop(&mut self) {
        if let Some(scheduler) = &self.scheduler {
            let id = self.id.worker_id() as usize;
            let _guard = scheduler.worker_access[id].mtx.lock().unwrap();
            // Set the state to shutting down
            scheduler.worker_access[id]
                .state
                .0
                .store(WORKER_STATE_SHUTTINGDOWN, std::sync::atomic::Ordering::SeqCst);

            // wake up the worker in case it is parked, it then shuts down
            scheduler.notify_worker(self.id.worker_id());
        }
    }
}

impl WorkerInner {
    fn pre_run(&mut self, drivers: Drivers, dedicated_scheduler: Arc<DedicatedScheduler>, with_safety: bool) {
        let mut builder = ContextBuilder::new(drivers)
            .thread_id(0)
            .with_async_handle(self.producer_consumer.clone(), self.scheduler.clone(), dedicated_scheduler)
            .with_worker_id(self.id);

        if with_safety {
            builder = builder.with_safety();
        }

        // Setup context
        ctx_initialize(builder);

        self.local_state = LocalState::Executing;
        self.own_interactor
            .state
            .0
            .store(WORKER_STATE_EXECUTING, std::sync::atomic::Ordering::SeqCst);
    }

    fn run(&mut self) {
        while self.own_interactor.state.0.load(std::sync::atomic::Ordering::Acquire) != WORKER_STATE_SHUTTINGDOWN {
            let (task_opt, should_notify) = self.try_pick_work();

            if let Some(task) = task_opt {
                self.run_task(task, should_notify);
                continue;
            }

            self.park_worker();
            self.local_state = LocalState::Executing;
        }
        debug!("Worker{} received stop request, shutting down", self.id.worker_id());
    }

    fn park_worker(&mut self) {
        if self
            .scheduler
            .transition_to_parked(self.local_state == LocalState::Searching, self.get_worker_id())
        {
            trace!("Last searcher is trying to sleep, inspect all work sources");

            // we transition ourself but we are last one who is going to sleep, let's recheck all queues, otherwise something may stuck there
            let gc_empty = self.scheduler.global_queue.is_empty();

            if !gc_empty {
                debug!("Unparking during parking due to global queue having work");
                self.scheduler.transition_from_parked(self.get_worker_id());
                return;
            }

            for access in &self.scheduler.worker_access {
                if access.steal_handle.count() > 0 {
                    debug!("Unparking during parking due to some steal queue having work");
                    self.scheduler.transition_from_parked(self.get_worker_id());
                    return;
                }
            }
        }

        ctx_get_drivers().park(&self.scheduler, &self.own_interactor);
    }

    fn run_task(&mut self, task: TaskRef, should_notify: bool) {
        self.transition_to_executing();

        if should_notify {
            self.scheduler.try_notify_siblings_workers(Some(self.get_worker_id()));
        }

        let waker = create_waker(task.clone());
        let mut ctx = Context::from_waker(&waker);
        match task.poll(&mut ctx) {
            TaskPollResult::Done => {
                // Literally nothing to do ;)
            }
            TaskPollResult::Notified => {
                // For now stupid respawn
                self.scheduler.spawn_from_runtime(task, &self.producer_consumer);
            }
        }
    }

    fn try_pick_work(&mut self) -> (Option<TaskRef>, bool) {
        self.next_task_tick = self.next_task_tick.wrapping_add(1);

        self.maybe_run_driver();

        // First check our queue for work
        let mut task = self.producer_consumer.pop();
        if task.is_some() {
            return (task, false);
        }

        // Now we enter searching if there is no enough contention already.
        let res = self.try_transition_to_searching();

        if !res {
            trace!("Decided to not steal and sleep!");
            return (None, false); // Seems there is enough workers doing contended access, we shall sleep
        }

        // Next, try steal from other workers. Do this only, if no more than half the workers are
        // already searching for work.
        let mut should_notify;

        (task, should_notify) = self.try_steal_work();
        if task.is_some() {
            return (task, should_notify);
        }

        // Next, check global queue
        (task, should_notify) = self.try_take_global_work();
        if task.is_some() {
            return (task, should_notify);
        }

        (None, false)
    }

    fn try_steal_work(&mut self) -> (Option<TaskRef>, bool) {
        let current_worker = ctx_get_worker_id().worker_id() as usize;

        let start_idx = self.randomness_source.next() as usize;
        let cnt = self.scheduler.worker_access.len();

        let mut stolen = 0;

        // Start from random worker
        for idx in 0..cnt {
            let real_idx = (start_idx + idx) % cnt;

            if real_idx == current_worker {
                continue;
            }

            let res = self.scheduler.worker_access[real_idx]
                .steal_handle
                .steal_into(&self.own_interactor.steal_handle, None);

            stolen += res.unwrap_or_default();
        }

        trace!("Stolen {:?}", stolen);
        (self.producer_consumer.pop(), stolen > 0)
    }

    //
    // Tries to take  TAKE_GLOBAL_WORK_SIZE `TaskRef` items from the global_queue into the local task queue. Returns
    // the first `TaskRef` if that did work, or None if that did not work or the global_queue lock
    // could not be acquired.
    //
    // NOTE: This is currently double copying: 1. From global_queue into `mem` here and 2. From
    // `mem` to local_queue. Maybe we can optimize this in the future.
    //
    fn try_take_global_work(&self) -> (Option<TaskRef>, bool) {
        let taken = self.try_take_global_work_internal();

        if taken > 0 {
            (self.producer_consumer.pop(), taken > 0)
        } else {
            (None, false)
        }
    }

    fn try_take_global_work_internal(&self) -> usize {
        let cnt = self.producer_consumer.fetch_from(&self.scheduler.global_queue);
        trace!("Taken from global queue {}", cnt);
        cnt
    }

    fn try_transition_to_searching(&mut self) -> bool {
        let mut res = true;

        if self.local_state != LocalState::Searching {
            res = self.scheduler.try_transition_worker_to_searching();

            if res {
                self.local_state = LocalState::Searching;
            }
        }

        res
    }

    fn transition_to_executing(&mut self) {
        if self.local_state != LocalState::Executing {
            self.scheduler.transition_worker_to_executing();
            self.local_state = LocalState::Executing;
        }
    }

    fn get_worker_id(&self) -> usize {
        self.id.worker_id() as usize
    }

    // Function responsible to run work under the driver (ie process timeouts, etc.)
    fn maybe_run_driver(&mut self) {
        const EVENT_POLLING_TICK: u64 = 31;

        if self.next_task_tick % EVENT_POLLING_TICK == 0 {
            ctx_get_drivers().process_work();
        }
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use crate::scheduler::scheduler_mt::scheduler_new;
    use crate::scheduler::workers::{worker::Worker, worker_types::*};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    #[test]
    fn test_worker_stop_sets_shutdown_state() {
        let scheduler = Arc::new(scheduler_new(1, 4));
        let mut worker = Worker {
            thread_handle: None,
            id: WorkerId::new(format!("arunner{}", 0).as_str().into(), 0, 0, WorkerType::Async),
            engine_has_safety_worker: false,
            scheduler: Some(scheduler.clone()),
        };

        worker.stop();

        let state = scheduler.worker_access[0].state.0.load(Ordering::SeqCst);
        assert_eq!(state, WORKER_STATE_SHUTTINGDOWN);

        // Check that unpark does not panic and state remains SHUTTINGDOWN
        scheduler.worker_access[0].unpark();
        let state = scheduler.worker_access[0].state.0.load(Ordering::SeqCst);
        assert_eq!(state, WORKER_STATE_SHUTTINGDOWN);
    }

    #[test]
    // miri does not like this test for some reason. Disable it for now. The message is
    // ```
    // error: unsupported operation: can't call foreign function `pthread_attr_init` on OS `linux`
    // ```
    // See https://github.com/qorix-group/inc_orchestrator_internal/actions/runs/15205492004/job/42767523599#step:9:237
    // for an example CI run.
    #[cfg(not(miri))]
    fn test_worker_stop() {
        use crate::scheduler::driver::Drivers;
        use crate::{box_future, AsyncTask, FoundationAtomicBool, TaskRef};
        use foundation::prelude::debug;
        use foundation::threading::thread_wait_barrier::ThreadWaitBarrier;
        use std::sync::Arc;
        use std::time::Duration;

        async fn test_fn(b: Arc<FoundationAtomicBool>) {
            b.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        let scheduler = Arc::new(crate::scheduler::scheduler_mt::scheduler_new(1, 4));
        let dedicated_scheduler = Arc::new(crate::scheduler::scheduler_mt::DedicatedScheduler {
            dedicated_queues: Box::new([]),
        });

        let barrier = Arc::new(ThreadWaitBarrier::new(1));
        let ready_notifier = barrier.get_notifier().unwrap();

        let thread_params = crate::scheduler::workers::ThreadParameters::default();

        let mut worker = Worker {
            thread_handle: None,
            id: WorkerId::new(format!("arunner{}", 0).as_str().into(), 0, 0, WorkerType::Async),
            engine_has_safety_worker: false,
            scheduler: Some(scheduler.clone()),
        };
        worker.start(scheduler.clone(), Drivers::new(), dedicated_scheduler, ready_notifier, &thread_params);

        match barrier.wait_for_all(Duration::from_secs(5)) {
            Ok(_) => {
                debug!("Worker ready, continuing with test...");
            }
            Err(_) => {
                panic!("Timeout waiting for worker to become ready");
            }
        }

        // First, test that tasks are executed normally
        let first_task_executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let first_task_executed_clone = first_task_executed.clone();

        let task = Arc::new(AsyncTask::new(box_future(test_fn(first_task_executed_clone)), 0, scheduler.clone()));

        scheduler.spawn_outside_runtime(TaskRef::new(task));
        std::thread::sleep(Duration::from_millis(100));

        assert!(
            first_task_executed.load(std::sync::atomic::Ordering::SeqCst),
            "First task was not executed while worker was still active"
        );

        // Now stop the worker
        worker.stop();

        // Try to execute a second task after stopping
        let second_task_executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let second_task_executed_clone = second_task_executed.clone();

        let task = Arc::new(AsyncTask::new(box_future(test_fn(second_task_executed_clone)), 0, scheduler.clone()));

        scheduler.spawn_outside_runtime(TaskRef::new(task));

        std::thread::sleep(Duration::from_millis(100));

        // The second task should NOT have been executed
        assert!(
            !second_task_executed.load(std::sync::atomic::Ordering::SeqCst),
            "Second task was executed even though worker was stopped"
        );
    }
}
