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
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::scheduler_mt::*;
use super::task::async_task::TaskRef;
use super::workers::dedicated_worker::DedicatedWorker;
use super::workers::safety_worker::SafetyWorker;
use super::workers::worker::Worker;
use super::workers::worker_types::*;
use crate::scheduler::driver::Drivers;
use crate::{
    box_future,
    core::types::UniqueWorkerId,
    runtime::async_runtime::RuntimeErrors,
    scheduler::{
        workers::{worker::FIRST_WORKER_ID, ThreadParameters},
        SchedulerType,
    },
    AsyncTask, Future,
};
use foundation::containers::growable_vec::GrowableVec;
use foundation::containers::mpmc_queue::MpmcQueue;
use foundation::containers::trigger_queue::{TriggerQueue, TriggerQueueConsumer};
use foundation::prelude::*;
use foundation::threading::thread_wait_barrier::ThreadWaitBarrier;

pub struct JoinHandle {
    recv: Option<TriggerQueueConsumer<Result<u32, RuntimeErrors>>>,
}

impl JoinHandle {
    pub fn join(self) -> Result<u32, RuntimeErrors> {
        self.recv
            .ok_or(RuntimeErrors::ResultNotThere)?
            .pop_blocking_with_timeout(Duration::MAX)
            .unwrap_or(Err(RuntimeErrors::NoResultAvailable))
    }
}

/// The central engine for managing and executing asynchronous and dedicated tasks.
///
/// `ExecutionEngine` encapsulates worker threads, schedulers, and their associated queues.
/// It provides methods to start, stop, and monitor tasks within its own runtime context.
/// Instances are typically created using [`ExecutionEngineBuilder`].
pub struct ExecutionEngine {
    async_workers: Vec<Worker>,
    async_queues: Vec<TaskStealQueue>,
    async_scheduler: Arc<AsyncScheduler>,

    dedicated_workers: Vec<DedicatedWorker>,
    dedicated_scheduler: Arc<DedicatedScheduler>,

    safety_worker: Option<SafetyWorker>,
    thread_params: ThreadParameters,
    current_handle: Option<JoinHandle>,
}

impl ExecutionEngine {
    /// Runs the given future to completion, blocking the calling thread until it finishes.
    ///
    /// Starts the engine, executes the future, and returns its result. The calling thread will
    /// block until the given future is complete.
    /// Returns an error if a task is already running or if no result is available.
    pub(crate) fn block_on<T: Future<Output = Result<u32, RuntimeErrors>> + 'static + Send>(&mut self, future: T) -> Result<u32, RuntimeErrors> {
        self.run_in_engine(future)?;
        self.current_handle.take().ok_or(RuntimeErrors::NoTaskRunning)?.join()
    }

    /// Starts the engine and executes the given future within the runtime context.
    ///
    /// Returns an error if a task is already running.
    /// Execution is asynchronous; the result can be retrieved later using [`wait_for`].
    pub(crate) fn run_in_engine<T: Future<Output = Result<u32, RuntimeErrors>> + 'static + Send>(&mut self, future: T) -> Result<(), RuntimeErrors> {
        if self.current_handle.is_some() {
            return Err(RuntimeErrors::TaskAlreadyRunning);
        }

        let tq = Arc::new(TriggerQueue::new(1));
        let recv = tq.clone().get_consumer();

        self.current_handle = Some(JoinHandle { recv });
        let boxed = box_future(async move {
            let res = future.await;

            tq.push(res);
        });
        let scheduler = self.get_async_scheduler();
        let task = Arc::new(AsyncTask::new(boxed, FIRST_WORKER_ID, scheduler)); // This is the first initial worker we start on this engine, so we can use a constant worker_id
        let entry_task = TaskRef::new(task.clone());

        {
            //TODO: Total hack, injecting task before we run any async_workers so they will pick it
            let pc = self.async_queues[0].get_local().unwrap();
            pc.push(entry_task, &self.async_scheduler.global_queue)
                .unwrap_or_else(|_| panic!("Failed to enter runtime while pushing init task"));
        }

        let safety_worker_count = self.safety_worker.is_some() as u32;

        let start_barrier = Arc::new(ThreadWaitBarrier::new(
            self.async_workers.len() as u32 + self.dedicated_workers.len() as u32 + safety_worker_count,
        ));

        let drivers = Drivers::new();

        if safety_worker_count > 0 {
            self.safety_worker
                .as_mut()
                .expect("Safety worker has to present as check was done above")
                .start(
                    self.async_scheduler.clone(),
                    drivers.clone(),
                    self.dedicated_scheduler.clone(),
                    start_barrier.get_notifier().unwrap(),
                    &self.thread_params,
                );
        }

        self.async_workers.iter_mut().for_each(|w| {
            w.start(
                self.async_scheduler.clone(),
                drivers.clone(),
                self.dedicated_scheduler.clone(),
                start_barrier.get_notifier().unwrap(),
                &self.thread_params,
            );
        });

        self.dedicated_workers.iter_mut().for_each(|w| {
            w.start(
                self.async_scheduler.clone(),
                drivers.clone(),
                self.dedicated_scheduler.clone(),
                start_barrier.get_notifier().unwrap(),
                &self.thread_params,
            );
        });

        debug!("Engine starts waiting for workers to be ready");

        let res = start_barrier.wait_for_all(Duration::new(5, 0));
        match res {
            Ok(_) => {
                debug!("Workers ready, continue...");
            }
            Err(_) => {
                panic!("Timeout on starting engine, not all workers reported ready, stopping...");
            }
        }

        Ok(())
    }

    pub(crate) fn get_async_scheduler(&self) -> Arc<AsyncScheduler> {
        self.async_scheduler.clone()
    }

    /// Waits for the result of the currently running entry-/main-task.
    ///
    /// It is required to asynchronously start a task with [`run_in_engine`] before using this.
    /// Returns the result of a finished task or an error if no task is running.
    pub(crate) fn wait_for(&mut self) -> Result<u32, RuntimeErrors> {
        if let Some(handle) = self.current_handle.take() {
            return handle.join();
        }

        Err(RuntimeErrors::NoTaskRunning)
    }

    /// Stops all worker threads managed by the engine.
    ///
    /// The running tasks are not finished, they are only finishing their currently running
    /// iteration and are then aborted. That means that running taskes are driven until their
    /// current poll iteration finishes, regardless of the return value. Even when a task returns
    /// Poll::Pending, it will be stopped after the current iteration.
    pub(crate) fn stop(&mut self) {
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
}

/// Dropping the `ExecutionEngine` will stop all workers and wait for them to finish.
///
/// This means that the main task is completed before the engine is destroyed. Main task is the
/// one,  which was started with [`run_in_engine`].
/// We try to `wait_for` it here, but ignore the result, because an error returned from `wait_for`
/// is kind of expected: The user of this `ExecutionEngine` might already have called `stop` or
/// `wait_for` before and this will result in an error. If he did not, we to do it for him.
/// After waiting for the main task to finish, we call `stop` to stop all workers.
impl Drop for ExecutionEngine {
    fn drop(&mut self) {
        let _ = self.wait_for();
        self.stop();
    }
}

pub struct ExecutionEngineBuilder {
    async_workers_cnt: usize,
    queue_size: usize,
    thread_params: ThreadParameters,

    dedicated_workers_ids: GrowableVec<UniqueWorkerId>,
    with_safe_worker: (bool, ThreadParameters), //enabled, params
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
            thread_params: ThreadParameters::default(),
        }
    }

    ///
    /// Will create runtime with `cnt` async workers
    ///
    pub fn workers(mut self, cnt: usize) -> Self {
        self.async_workers_cnt = cnt;
        self
    }

    ///
    /// Configure queue size with `size` for each async worker.
    /// >ATTENTION: `size` has to be power of two
    ///
    pub fn task_queue_size(mut self, size: usize) -> Self {
        assert!(size.is_power_of_two(), "Task queue size must be power of two");
        self.queue_size = size;
        self
    }

    pub fn thread_priority(mut self, thread_prio: u8) -> Self {
        self.thread_params.priority = Some(thread_prio);
        self
    }

    pub fn thread_affinity(mut self, thread_affinity: usize) -> Self {
        self.thread_params.affinity = Some(thread_affinity);
        self
    }

    pub fn thread_scheduler(mut self, thread_scheduler_type: SchedulerType) -> Self {
        self.thread_params.scheduler_type = Some(thread_scheduler_type);
        self
    }

    pub fn thread_stack_size(mut self, thread_stack_size: u64) -> Self {
        self.thread_params.stack_size = Some(thread_stack_size);
        self
    }

    pub fn enable_safety_worker(mut self, params: ThreadParameters) -> Self {
        self.with_safe_worker = (true, params);
        self
    }

    ///
    /// Adds new dedicated worker to the engine identified by `id`
    ///
    #[allow(dead_code)]
    pub fn with_dedicated_worker(mut self, id: UniqueWorkerId) -> Self {
        assert!(
            !self.dedicated_workers_ids.contains(&id),
            "Cannot register same unique worker multiple times!"
        );

        self.dedicated_workers_ids.push(id);
        debug!("Registered worker {:?}", id);
        self
    }

    pub(crate) fn build(self) -> ExecutionEngine {
        // Create async workers part
        let mut worker_interactors = Box::<[WorkerInteractor]>::new_uninit_slice(self.async_workers_cnt);
        let mut async_queues: Vec<TaskStealQueue> = Vec::new(self.async_workers_cnt);

        let safety_worker_queue;
        let safety_worker = {
            if self.with_safe_worker.0 {
                let w = SafetyWorker::new(WorkerId::new("SafetyWorker".into(), 0, 0, WorkerType::Dedicated));
                safety_worker_queue = Some(w.get_queue());
                Some(w)
            } else {
                safety_worker_queue = None;
                None
            }
        };

        for i in 0..self.async_workers_cnt {
            async_queues.push(create_steal_queue(self.queue_size));

            unsafe {
                worker_interactors[i].as_mut_ptr().write(WorkerInteractor::new(async_queues[i].clone()));
            }
        }

        let global_queue = MpmcQueue::new(32);
        let async_scheduler = Arc::new(AsyncScheduler {
            worker_access: unsafe { worker_interactors.assume_init() },
            num_of_searching_workers: FoundationAtomicU8::new(0),
            parked_workers_indexes: Mutex::new(Vec::new(self.async_workers_cnt)),
            global_queue,
            safety_worker_queue,
        });

        let mut async_workers = Vec::new(self.async_workers_cnt);

        for i in 0..self.async_workers_cnt {
            async_workers.push(Worker::new(
                self.thread_params.priority,
                WorkerId::new(format!("arunner{}", i).as_str().into(), 0, i as u8, WorkerType::Async),
                self.with_safe_worker.0,
            ));
        }

        // Create dedicated workers part
        let mut dedicated_workers = Vec::new(self.dedicated_workers_ids.len());
        let mut dedicated_queues = Box::<[(WorkerId, Arc<TriggerQueue<TaskRef>>)]>::new_uninit_slice(self.dedicated_workers_ids.len());

        for i in 0..self.dedicated_workers_ids.len() {
            let id = self.dedicated_workers_ids[i];
            let real_id = WorkerId::new(id, 0, i as u8, WorkerType::Dedicated);

            dedicated_workers.push(DedicatedWorker::new(real_id, self.with_safe_worker.0));
            unsafe {
                dedicated_queues[i]
                    .as_mut_ptr()
                    .write((real_id, Arc::new(TriggerQueue::new(self.queue_size))));
            }
        }

        let dedicated_scheduler = Arc::new(DedicatedScheduler {
            dedicated_queues: unsafe { dedicated_queues.assume_init() },
        });

        ExecutionEngine {
            async_workers,
            async_queues,
            async_scheduler,
            dedicated_workers,
            dedicated_scheduler,
            safety_worker,
            thread_params: self.thread_params,
            current_handle: None,
        }
    }
}

#[cfg(test)]
#[cfg(not(loom))]
// This is because of the disabled miri tests below
#[allow(unused_imports)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::thread;
    use std::time::Duration;

    // used from async_runtime.rs unit test
    impl ExecutionEngine {
        pub fn worker_count(&self) -> usize {
            self.async_workers.len()
        }
    }

    fn create_engine(workers: usize) -> ExecutionEngine {
        ExecutionEngineBuilder::new().workers(workers).task_queue_size(8).build()
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
        let result = engine.block_on(async { Ok(123u32) });
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

        engine
            .run_in_engine(async move {
                called_clone.fetch_add(1, Ordering::SeqCst);
                Ok(42u32)
            })
            .unwrap();

        // Wait for the result
        let result = engine.wait_for();
        assert_eq!(result, Ok(42));
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    // miri does not like this test for some reason. Disable it for now. The message is
    // ```
    // error: unsupported operation: can't call foreign function `pthread_attr_init` on OS `linux`
    // ```
    // See https://github.com/qorix-group/inc_orchestrator_internal/actions/runs/15675294733/job/44154074863?pr=47
    // for an example CI run.
    #[cfg(not(miri))]
    fn test_run_in_engine_twice_should_fail() {
        let mut engine = create_engine(1);

        engine.run_in_engine(async { Ok(1u32) }).expect("First run_in_engine should succeed");

        // Second call should fail because a task is already running
        let err = engine.run_in_engine(async { Ok(2u32) }).unwrap_err();
        assert_eq!(err, RuntimeErrors::TaskAlreadyRunning);

        // Wait for the first task to finish
        let res = engine.wait_for();
        assert_eq!(res, Ok(1));
    }

    #[test]
    fn test_wait_for_without_task_should_fail() {
        let mut engine = create_engine(1);
        let err = engine.wait_for().unwrap_err();
        assert_eq!(err, RuntimeErrors::NoTaskRunning);
    }

    #[test]
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
        let mut handles = GrowableVec::new(4);
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

        engine
            .run_in_engine(async move {
                thread::sleep(Duration::from_millis(50));
                ready_notifier.ready();
                Ok(777u32)
            })
            .unwrap();

        assert_eq!(Ok(()), barrier.wait_for_all(Duration::from_secs(1)));

        let result = engine.wait_for();
        assert_eq!(result, Ok(777));
    }
}
