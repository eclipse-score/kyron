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
use ::core::time::Duration;
use std::sync::Arc;

use super::scheduler_mt::*;
use super::task::async_task::TaskRef;
use super::workers::dedicated_worker::DedicatedWorker;
use super::workers::safety_worker::SafetyWorker;
use super::workers::worker::Worker;
use super::workers::worker_types::*;
use crate::macros::log::*;
use crate::scheduler::driver::Drivers;
use crate::{
    box_future,
    common::types::UniqueWorkerId,
    scheduler::{workers::ThreadParameters, SchedulerType},
    AsyncTask, Future,
};
use kyron_foundation::containers::growable_vec::GrowableVec;
use kyron_foundation::containers::mpmc_queue::MpmcQueue;
use kyron_foundation::containers::trigger_queue::{TriggerQueue, TriggerQueueConsumer};
use kyron_foundation::containers::*;
use kyron_foundation::prelude::vector_extension::VectorExtension;
use kyron_foundation::threading::thread_wait_barrier::ThreadWaitBarrier;

pub struct JoinHandle<T> {
    recv: TriggerQueueConsumer<T>,
}

impl<T> JoinHandle<T> {
    /// Blocking wait for the result of future that was scheduled for run into runtime
    pub fn join(self) -> T {
        self.recv.pop_blocking()
    }
}

// Max number of workers per engine (255) = async workers (128) + dedicated workers (126) + safety worker (1)
// The worker IDs range: 0 to 254 (max).
const MAX_NUM_OF_WORKERS: usize = 128;
const MAX_NUM_OF_DEDICATED_WORKERS: usize = 126;
// The worker ID 255 is for main thread which create parent task since `TaskId` encodes worker ID that create the task.
const MAIN_THREAD_ID: u8 = u8::MAX;

enum EngineState {
    Starting,
    Running,
    Stopped, // Engine is stopped, no workers are running anymore
}

impl EngineState {
    #[allow(dead_code)]
    fn is_running(&self) -> bool {
        matches!(self, EngineState::Running)
    }

    fn is_stopped(&self) -> bool {
        matches!(self, EngineState::Stopped)
    }
}
/// The central engine for managing and executing asynchronous and dedicated tasks.
///
/// `ExecutionEngine` encapsulates worker threads, schedulers, and their associated queues.
/// It provides methods to start, stop, and monitor tasks within its own runtime context.
/// Instances are typically created using [`ExecutionEngineBuilder`].
pub struct ExecutionEngine {
    async_workers: Vec<Worker>,

    #[allow(dead_code)]
    async_queues: Vec<TaskStealQueue>,
    async_scheduler: Arc<AsyncScheduler>,

    dedicated_workers: Vec<DedicatedWorker>,
    dedicated_scheduler: Arc<DedicatedScheduler>,

    safety_worker: Option<SafetyWorker>,
    thread_params: ThreadParameters,
    state: EngineState,

    // TODO: When we add enter into runtime from main, this does not need to be here
    drivers: Drivers,
}

impl ExecutionEngine {
    /// Runs the given future to completion, blocking the calling thread until it finishes.
    ///
    /// Starts the engine, executes the future, and returns its result. The calling thread will
    /// block until the given future is complete.
    /// Returns an error if a task is already running or if no result is available.
    pub(crate) fn block_on<T: Future + 'static + Send>(&mut self, future: T) -> T::Output
    where
        T::Output: Send,
    {
        self.run_in_engine(future).join()
    }

    /// Starts the engine and executes the given future within the runtime context.
    ///
    /// Returns an error if a task is already running.
    /// Execution is asynchronous; the result can be retrieved later using [`wait_for`].
    pub(crate) fn run_in_engine<T: Future + 'static + Send>(&mut self, future: T) -> JoinHandle<T::Output>
    where
        T::Output: Send,
    {
        let tq = Arc::new(TriggerQueue::new(1));
        let recv = tq.clone().get_consumer().unwrap();

        let boxed = box_future(async move {
            let res = future.await;
            tq.push(res);
        });

        let scheduler = self.get_async_scheduler();
        let worker_id = WorkerId::new(
            "MainThread".into(),
            MAIN_THREAD_ID,
            MAIN_THREAD_ID,
            WorkerType::Dedicated,
        );
        let task = Arc::new(AsyncTask::new(boxed, &worker_id, scheduler));
        let entry_task = TaskRef::new(task.clone());

        self.async_scheduler.respawn(entry_task);

        JoinHandle { recv }
    }

    pub(crate) fn get_async_scheduler(&self) -> Arc<AsyncScheduler> {
        self.async_scheduler.clone()
    }

    /// Stops all worker threads managed by the engine.
    ///
    /// The running tasks are not finished, they are only finishing their currently running
    /// iteration and are then aborted. That means that running tasks are driven until their
    /// current poll iteration finishes, regardless of the return value. Even when a task returns
    /// Poll::Pending, it will be stopped after the current iteration.
    pub(crate) fn stop(&mut self) {
        if self.state.is_stopped() {
            return; // Already stopped, nothing to do
        }

        self.state = EngineState::Stopped;

        for worker in self.async_workers.iter_mut() {
            worker.stop();
        }

        for dworker in self.dedicated_workers.iter_mut() {
            dworker.stop();
        }

        if let Some(ref sworker) = self.safety_worker {
            sworker.stop();
        }
    }

    fn start(&mut self) {
        let safety_worker_count = self.safety_worker.is_some() as u32;

        let start_barrier = Arc::new(ThreadWaitBarrier::new(
            self.async_workers.len() as u32 + self.dedicated_workers.len() as u32 + safety_worker_count,
        ));

        if safety_worker_count > 0 {
            self.safety_worker
                .as_mut()
                .expect("Safety worker has to present as check was done above")
                .start(
                    self.async_scheduler.clone(),
                    self.drivers.clone(),
                    self.dedicated_scheduler.clone(),
                    start_barrier.get_notifier().unwrap(),
                );
        }

        self.async_workers.iter_mut().for_each(|w| {
            w.start(
                self.async_scheduler.clone(),
                self.drivers.clone(),
                self.dedicated_scheduler.clone(),
                start_barrier.get_notifier().unwrap(),
                &self.thread_params,
            );
        });

        self.dedicated_workers.iter_mut().for_each(|w| {
            w.start(
                self.async_scheduler.clone(),
                self.drivers.clone(),
                self.dedicated_scheduler.clone(),
                start_barrier.get_notifier().unwrap(),
            );
        });

        // Workers are spawned successfully
        self.state = EngineState::Running;
        debug!("Engine starts waiting for workers to be ready");

        let res = start_barrier.wait_for_all(Duration::new(5, 0));
        match res {
            Ok(_) => {
                debug!("Workers ready, continue...");
            },
            Err(_) => {
                panic!("Timeout on starting engine, not all workers reported ready, stopping...");
            },
        }
    }
}

/// Dropping the `ExecutionEngine` will stop all workers and wait for them to finish.
///
/// This means that the current task procseed by workers will be continued until it returns
/// `Pending` or `Ready`.
impl Drop for ExecutionEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct ExecutionEngineBuilder {
    async_workers_cnt: usize,
    queue_size: u32,
    thread_params: ThreadParameters,
    id: u8,
    dedicated_workers_ids: GrowableVec<(UniqueWorkerId, ThreadParameters)>,
    with_safe_worker: (bool, ThreadParameters), //enabled, params
    safety_worker_queue_size: u32,
}

impl Default for ExecutionEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEngineBuilder {
    pub fn new() -> Self {
        Self {
            async_workers_cnt: 1,
            queue_size: 256,
            dedicated_workers_ids: GrowableVec::new(2),
            with_safe_worker: (false, ThreadParameters::default()),
            safety_worker_queue_size: 64,
            thread_params: ThreadParameters::default(),
            id: 0,
        }
    }

    pub(crate) fn set_engine_id(mut self, id: u8) -> Self {
        self.id = id;
        self
    }
    ///
    /// Will create engine with `cnt` async workers. It has to be in range [1, 128]
    ///
    pub fn workers(mut self, cnt: usize) -> Self {
        assert!(
            cnt > 0 && cnt <= MAX_NUM_OF_WORKERS,
            "Cannot create engine with {} workers. Min is 1 and max is {}",
            cnt,
            MAX_NUM_OF_WORKERS
        );

        self.async_workers_cnt = cnt;
        self
    }

    ///
    /// Configure queue size with `size` for each async worker.
    /// >ATTENTION: `size` has to be power of two
    ///
    pub fn task_queue_size(mut self, size: u32) -> Self {
        assert!(
            size.is_power_of_two(),
            "Task queue size ({}) must be power of two",
            size
        );
        self.queue_size = size;
        self
    }

    /// Configures thread priority for the async workers.
    /// Ensure to configure scheduler type also.
    /// If priority or scheduler type is `None`, then both attributes will be inherited from parent thread.
    pub fn thread_priority(mut self, thread_prio: u8) -> Self {
        self.thread_params = self.thread_params.priority(thread_prio);
        self
    }

    /// Configures thread affinity for the async workers.
    /// Input is an array of CPU core ids that the thread can run on.
    pub fn thread_affinity(mut self, affinity: &[usize]) -> Self {
        self.thread_params = self.thread_params.affinity(affinity);
        self
    }

    /// Configures the scheduler type for the async workers.
    /// Ensure to configure priority also.
    /// If priority or scheduler type is `None`, then both attributes will be inherited from parent thread.
    pub fn thread_scheduler(mut self, thread_scheduler_type: SchedulerType) -> Self {
        self.thread_params = self.thread_params.scheduler_type(thread_scheduler_type);
        self
    }

    /// Configures the stack size for the async workers. Min is `Limit::MinStackSizeOfThread.value()`
    /// which is OS and platform dependent.
    pub fn thread_stack_size(mut self, thread_stack_size: u64) -> Self {
        self.thread_params = self.thread_params.stack_size(thread_stack_size);
        self
    }

    /// Enables the safety worker with the given thread parameters `params`.
    /// If priority or scheduler type is `None`, then both attributes will be inherited from parent thread.
    pub fn enable_safety_worker(mut self, params: ThreadParameters) -> Self {
        if params.priority.is_none() ^ params.scheduler_type.is_none() {
            warn!("Either priority or scheduler type is 'None' for safety worker, both attributes will be inherited from parent thread.");
        }
        self.with_safe_worker = (true, params);
        self
    }

    ///
    /// Configure safety worker task queue size with `size`.
    /// >ATTENTION: `size` has to be power of two and safety worker shall be enabled prior to queue size configuration.
    ///
    pub fn safety_worker_task_queue_size(mut self, size: u32) -> Self {
        assert!(
            size.is_power_of_two(),
            "Safety worker task queue size ({}) must be power of two",
            size
        );
        assert!(
            self.with_safe_worker.0,
            "Enable safety worker prior to configuring its queue size."
        );
        self.safety_worker_queue_size = size;
        self
    }

    ///
    /// Adds new dedicated worker identified by `id` to the engine with given thread parameters `params`.
    /// If priority or scheduler type is `None`, then both attributes will be inherited from parent thread.
    #[allow(dead_code)]
    pub fn with_dedicated_worker(mut self, id: UniqueWorkerId, params: ThreadParameters) -> Self {
        assert!(
            self.dedicated_workers_ids.len() < MAX_NUM_OF_DEDICATED_WORKERS,
            "Cannot create engine with more than {} dedicated workers.",
            MAX_NUM_OF_DEDICATED_WORKERS
        );
        assert!(
            !self.dedicated_workers_ids.iter().any(|(worker_id, _)| *worker_id == id),
            "Cannot register same unique worker multiple times!"
        );

        if params.priority.is_none() ^ params.scheduler_type.is_none() {
            warn!(
                "Either priority or scheduler type is 'None' for dedicated worker {:?}, both attributes will be inherited from parent thread.",
                id
            );
        }

        self.dedicated_workers_ids.push((id, params));
        debug!("Registered worker {:?}", id);
        self
    }

    pub(crate) fn build(self) -> ExecutionEngine {
        // Create async workers part
        let mut worker_interactors = Box::<[WorkerInteractor]>::new_uninit_slice(self.async_workers_cnt);
        let mut async_queues: Vec<TaskStealQueue> = Vec::new_in_global(self.async_workers_cnt);
        let safety_worker_queue;
        let safety_worker = {
            if self.with_safe_worker.0 {
                // Async worker ids has to start with 0..N, because worker id is used as index for worker interactors
                // so lets assign safety worker id after all the workers
                let safety_worker_id = self.async_workers_cnt + self.dedicated_workers_ids.len();
                let w = SafetyWorker::new(
                    WorkerId::new(
                        "SafetyWorker".into(),
                        self.id,
                        safety_worker_id as u8,
                        WorkerType::Dedicated,
                    ),
                    self.with_safe_worker.1,
                    self.safety_worker_queue_size,
                );
                safety_worker_queue = Some(w.get_queue());
                Some(w)
            } else {
                safety_worker_queue = None;
                None
            }
        };

        let mut async_workers = Vec::new_in_global(self.async_workers_cnt);

        if self.thread_params.priority.is_none() ^ self.thread_params.scheduler_type.is_none() {
            warn!("Either priority or scheduler type is 'None' for async worker, both attributes will be inherited from parent thread.");
        }

        for i in 0..self.async_workers_cnt {
            async_queues
                .push(create_steal_queue(self.queue_size))
                .expect("Failed to add new steal queue for async worker");

            let id = WorkerId::new(
                format!("arunner{}", i).as_str().into(),
                self.id,
                i as u8,
                WorkerType::Async,
            );

            worker_interactors[i].write(WorkerInteractor::new(async_queues[i].clone(), id));

            async_workers
                .push(Worker::new(id, self.with_safe_worker.0))
                .expect("Failed to add new  async worker");
        }

        let drivers = Drivers::new();

        let global_queue = MpmcQueue::new(32);
        let async_scheduler = Arc::new(AsyncScheduler::new(
            unsafe { worker_interactors.assume_init() },
            global_queue,
            safety_worker_queue,
            drivers.get_io_driver().get_unparker(),
        ));

        // Create dedicated workers part
        let mut dedicated_workers = Vec::new_in_global(core::cmp::max(self.dedicated_workers_ids.len(), 1));
        let mut dedicated_queues =
            Box::<[(WorkerId, Arc<TriggerQueue<TaskRef>>)]>::new_uninit_slice(self.dedicated_workers_ids.len());
        let dedicated_worker_id_start = self.async_workers_cnt;
        for i in 0..self.dedicated_workers_ids.len() {
            let id = self.dedicated_workers_ids[i].0;
            let real_id = WorkerId::new(
                id,
                self.id,
                (dedicated_worker_id_start + i) as u8,
                WorkerType::Dedicated,
            );
            let thread_params = self.dedicated_workers_ids[i].1.clone();
            dedicated_workers
                .push(DedicatedWorker::new(real_id, self.with_safe_worker.0, thread_params))
                .expect("Failed to add new dedicated worker");
            unsafe {
                dedicated_queues[i]
                    .as_mut_ptr()
                    .write((real_id, Arc::new(TriggerQueue::new(self.queue_size as usize))));
            }
        }

        let dedicated_scheduler = Arc::new(DedicatedScheduler {
            dedicated_queues: unsafe { dedicated_queues.assume_init() },
        });

        let mut engine = ExecutionEngine {
            async_workers,
            async_queues,
            async_scheduler,
            dedicated_workers,
            dedicated_scheduler,
            safety_worker,
            thread_params: self.thread_params,
            state: EngineState::Starting,
            drivers,
        };

        engine.start();
        engine
    }
}

#[cfg(test)]
#[cfg(not(loom))]
// This is because of the disabled miri tests below
#[allow(unused_imports)]
mod tests {
    use super::*;
    use core::future;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use core::time::Duration;
    use std::panic;
    use std::sync::Arc;
    use std::thread;

    // used from kyron.rs unit test
    impl ExecutionEngine {
        pub fn worker_count(&self) -> usize {
            self.async_workers.len()
        }
    }

    #[allow(dead_code)]
    fn create_engine(workers: usize) -> ExecutionEngine {
        ExecutionEngineBuilder::new()
            .workers(workers)
            .task_queue_size(8)
            .build()
    }

    #[test]
    fn create_with_wrong_task_queue_size_fails() {
        let mut result1 = panic::catch_unwind(|| {
            ExecutionEngineBuilder::new().task_queue_size(0).build();
        });

        assert!(result1.is_err());

        result1 = panic::catch_unwind(|| {
            ExecutionEngineBuilder::new().task_queue_size(123).build();
        });

        assert!(result1.is_err());

        result1 = panic::catch_unwind(|| {
            ExecutionEngineBuilder::new().task_queue_size(546456).build();
        });

        assert!(result1.is_err());

        result1 = panic::catch_unwind(|| {
            ExecutionEngineBuilder::new().task_queue_size(2_u32.pow(31) - 1).build();
        });

        assert!(result1.is_err());
    }

    #[test]
    #[cfg(not(miri))] // Provenance issues
    fn create_engine_with_worker_and_verify_ids() {
        use crate::scheduler::context::{ctx_get_running_task_id, ctx_get_worker_id};
        let mut engine = ExecutionEngineBuilder::new()
            .workers(1)
            .task_queue_size(16)
            .set_engine_id(1)
            .build();
        let result: Result<bool, ()> = engine
            .run_in_engine(async move {
                let engine_id = ctx_get_worker_id().engine_id();
                let worker_id = ctx_get_worker_id().worker_id();
                let task_id = ctx_get_running_task_id().unwrap();
                let task_created_by_worker = task_id.worker();
                let task_created_by_engine = task_id.engine();
                Ok(engine_id == 1
                    && worker_id == 0
                    && task_created_by_worker == MAIN_THREAD_ID
                    && task_created_by_engine == MAIN_THREAD_ID)
            })
            .join();

        assert_eq!(result, Ok(true));
    }

    #[test]
    #[should_panic]
    #[cfg(not(miri))] // Provenance issues
    fn test_more_than_max_dedicated_worker_cannot_be_created() {
        let mut builder = ExecutionEngineBuilder::new();
        for i in 0..(MAX_NUM_OF_DEDICATED_WORKERS + 1) {
            builder =
                builder.with_dedicated_worker(("id".to_string() + &i.to_string()).into(), ThreadParameters::default());
        }
    }

    #[test]
    #[should_panic]
    #[cfg(not(miri))] // Provenance issues
    fn test_dedicated_workers_with_same_ids_cannot_be_created() {
        let mut builder = ExecutionEngineBuilder::new();
        for _ in 0..2 {
            builder = builder.with_dedicated_worker("dw1".into(), ThreadParameters::default());
        }
    }

    #[test]
    #[cfg(not(miri))] // Provenance issues
    fn create_with_correct_task_queue_size_works() {
        ExecutionEngineBuilder::new().task_queue_size(2).build();
        ExecutionEngineBuilder::new().task_queue_size(8).build();
        ExecutionEngineBuilder::new().task_queue_size(2_u32.pow(5)).build();
        ExecutionEngineBuilder::new().task_queue_size(2_u32.pow(20)).build();
    }

    #[test]
    #[cfg(not(miri))] // Provenance issues
    fn create_with_correct_num_of_workers() {
        ExecutionEngineBuilder::new().workers(1).build();
        ExecutionEngineBuilder::new().workers(MAX_NUM_OF_WORKERS).build();
        ExecutionEngineBuilder::new().workers(MAX_NUM_OF_WORKERS - 1).build();
    }

    #[test]
    #[cfg(not(miri))] // Provenance issues
    fn create_with_wrong_num_of_workers_fails() {
        let mut result1 = panic::catch_unwind(|| {
            ExecutionEngineBuilder::new().workers(0).build();
        });

        assert!(result1.is_err());

        result1 = panic::catch_unwind(|| {
            ExecutionEngineBuilder::new().workers(12345).build();
        });

        assert!(result1.is_err());
    }

    #[test]
    // miri does not like this test for some reason. Disable it for now. The message is
    // ```
    // error: unsupported operation: can't call foreign function `pthread_attr_init` on OS `linux`
    // ```
    // See https://github.com/qorix-group/inc_orchestrator_internal/actions/runs/15675294733/job/44154074863?pr=47
    // for an example CI run.
    #[cfg(not(miri))]
    fn test_block_on_returns_result() {
        let mut engine = create_engine(2);
        let result: Result<u32, ()> = engine.block_on(async { Ok(123u32) });
        assert_eq!(result, Ok(123));
    }

    #[test]
    // miri does not like this test for some reason. Disable it for now. The message is
    // ```
    // error: unsupported operation: can't call foreign function `pthread_attr_init` on OS `linux`
    // ```
    // See https://github.com/qorix-group/inc_orchestrator_internal/actions/runs/15675294733/job/44154074863?pr=47
    // for an example CI run.
    #[cfg(not(miri))]
    fn test_run_in_engine_and_wait_for() {
        let mut engine = create_engine(2);
        let called = Arc::new(AtomicUsize::new(0));
        let called_clone = called.clone();

        let result: Result<u32, ()> = engine
            .run_in_engine(async move {
                called_clone.fetch_add(1, Ordering::SeqCst);
                Ok(42u32)
            })
            .join();

        assert_eq!(result, Ok(42));
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[cfg(not(miri))] // Provenance issues
    fn test_stop_is_idempotent() {
        let mut engine = create_engine(2);
        engine.stop();
        engine.stop(); // Should not panic or error
    }

    #[test]
    // miri does not like this test for some reason. Disable it for now. The message is
    // ```
    // error: unsupported operation: can't call foreign function `pthread_attr_init` on OS `linux`
    // ```
    // See https://github.com/qorix-group/inc_orchestrator_internal/actions/runs/15675294733/job/44154074863?pr=47
    // for an example CI run.
    #[cfg(not(miri))]
    fn test_block_on_multiple_engines_parallel() {
        let mut handles: GrowableVec<thread::JoinHandle<Result<u32, ()>>> = GrowableVec::new(4);
        for i in 0..3 {
            handles.push(thread::spawn(move || {
                let mut engine = create_engine(1);
                engine.block_on(async move {
                    thread::sleep(Duration::from_millis(50));
                    Ok(i as u32)
                })
            }));
        }
        let mut results = GrowableVec::new(4);
        while let Some(handle) = handles.pop() {
            results.push(handle.join().unwrap());
        }

        assert!(results.contains(&Ok(0)));
        assert!(results.contains(&Ok(1)));
        assert!(results.contains(&Ok(2)));
    }

    #[test]
    // miri does not like this test for some reason. Disable it for now. The message is
    // ```
    // error: unsupported operation: can't call foreign function `pthread_attr_init` on OS `linux`
    // ```
    // See https://github.com/qorix-group/inc_orchestrator_internal/actions/runs/15675294733/job/44154074863?pr=47
    // for an example CI run.
    #[cfg(not(miri))]
    fn test_result_ready_before_wait_for() {
        let mut engine = create_engine(1);

        let barrier = Arc::new(ThreadWaitBarrier::new(1));
        let ready_notifier = barrier.get_notifier().unwrap();

        let handle = engine.run_in_engine(async move {
            thread::sleep(Duration::from_millis(50));
            ready_notifier.ready();
            Ok(777u32)
        });

        assert_eq!(Ok(()), barrier.wait_for_all(Duration::from_secs(1)));

        let result: Result<u32, ()> = handle.join();
        assert_eq!(result, Ok(777));
    }
}
