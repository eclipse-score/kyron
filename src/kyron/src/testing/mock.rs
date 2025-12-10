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

use crate::{
    core::types::{box_future, FutureBox, UniqueWorkerId},
    futures::reusable_box_future::ReusableBoxFuture,
    scheduler::{join_handle::JoinHandle, task::async_task::TaskRef, waker::create_waker},
    testing::*,
};
use ::core::{cell::RefCell, future::Future, sync::atomic, task::Context};
use std::collections::VecDeque;
use std::sync::Arc;

///
/// Mock runtime API with possibility to poll tasks manually.
/// ATTENTION: Instance of runtime is SINGLETON but in TLS so one for each thread.
///
pub struct MockRuntime {
    tasks: VecDeque<TaskRef>,
    sched: Arc<SchedulerMock>,
}

impl Drop for MockRuntime {
    fn drop(&mut self) {
        // Make sure not only drop is called on Task but we also aport it since it might hold resources
        while let Some(task) = self.tasks.pop_front() {
            task.abort();
        }
    }
}
thread_local! {
    static MOCK_RUNTIME: RefCell<Option<MockRuntime>> = const { RefCell::new(None) };
}

static DEV_CODE_ALLOW_ROUTING_OVER_MOCK: atomic::AtomicBool = atomic::AtomicBool::new(false);

pub mod runtime {

    use super::*;

    ///
    /// Initialize the runtime
    ///
    pub fn init() {
        MOCK_RUNTIME.set(Some(MockRuntime {
            tasks: VecDeque::new(),
            sched: Arc::new(SchedulerMock::default()),
        }));
    }

    ///
    /// Get access to the mock runtime via closure
    ///
    pub fn instance<C: FnMut(&mut MockRuntime) -> T, T>(mut c: C) -> T {
        MOCK_RUNTIME.with(|instance| {
            c(instance
                .borrow_mut()
                .as_mut()
                .expect("Runtime mock instance is not initialized. Did you call `init`?"))
        })
    }

    ///
    /// Reset the runtime
    ///
    pub fn reset() {
        MOCK_RUNTIME.with(|instance| {
            instance
                .borrow_mut()
                .as_mut()
                .expect("Runtime mock instance is not initialized. Did you call `init`?")
                .tasks
                .clear();
        });
    }

    ///
    /// Poll each task once to advance its state
    ///
    pub fn step() {
        let mut to_enqueue = vec![];

        while let Some(task) = dequeue_task() {
            let waker = create_waker(task.clone());
            let mut ctx = Context::from_waker(&waker);

            task.poll(&mut ctx);
            if !task.is_done() {
                to_enqueue.push(task);
            }
        }

        // Push tasks back after polling so we don't block with a task that is not finished yet,
        // as step only advances tasks once.
        for task in to_enqueue {
            enqueue_task(task);
        }
    }

    ///
    /// Number of pending tasks
    ///
    pub fn remaining_tasks() -> usize {
        MOCK_RUNTIME.with(|instance| {
            instance
                .borrow()
                .as_ref()
                .expect("Runtime mock instance is not initialized. Did you call `init`?")
                .tasks
                .len()
        })
    }

    ///
    /// Allows to run development process with RUNTIME MOCK compiled in so it will route to real runtime.
    ///
    /// # Safety
    ///
    /// THIS IS ONLY ALLOWED IN DEV CODE, NOT IN PRODUCTION.
    pub unsafe fn allow_routing_over_mock() {
        DEV_CODE_ALLOW_ROUTING_OVER_MOCK.store(true, atomic::Ordering::Relaxed);
        println!(
        "\n{bar}\n\
     {warn:^80}\n\
     {msg:^80}\n\
     {bar}\n",
        bar = "═".repeat(80),
        warn = "⚠️  UNSAFE RUN DETECTED! ⚠️",
        msg =
            "This build include RUNTIME API MOCK AND SHALL NOT BE USED IN ANY PRODUCTION! THIS WORKS BECAUSE YOU EXPLICITLY ALLOWED IT IN DEV CODE.",
    );
    }

    fn dequeue_task() -> Option<TaskRef> {
        MOCK_RUNTIME.with(|instance| {
            instance
                .borrow_mut()
                .as_mut()
                .expect("Runtime mock instance is not initialized. Did you call `init`?")
                .tasks
                .pop_front()
        })
    }

    fn enqueue_task(task: TaskRef) {
        MOCK_RUNTIME.with(|instance| {
            instance
                .borrow_mut()
                .as_mut()
                .expect("Runtime mock instance is not initialized. Did you call `init`?")
                .tasks
                .push_back(task)
        });
    }
}

///
/// Spawns a given `future` into runtime and let it execute on any of configured workers
/// This function allocates a `future` dynamically using [`Box`]
///
pub fn spawn<T>(future: T) -> JoinHandle<T::Output>
where
    T: Future + 'static + Send,
    T::Output: Send + 'static,
{
    let boxed = box_future(future);
    spawn_from_boxed(boxed)
}

///
/// Same as [`spawn`], but from already boxed future. No allocation is done for a future using this API.
///
pub fn spawn_from_boxed<T>(boxed: FutureBox<T>) -> JoinHandle<T>
where
    T: Send + 'static,
{
    if DEV_CODE_ALLOW_ROUTING_OVER_MOCK.load(atomic::Ordering::Relaxed) {
        crate::spawn_from_boxed(boxed)
    } else {
        MOCK_RUNTIME.with(|instance| {
            let mut instance = instance.borrow_mut();

            let runtime = instance.as_mut().expect("Runtime mock instance is not initialized. Did you call `init`?");

            let task = Arc::new(AsyncTask::new(boxed, 0, runtime.sched.clone()));
            let task_ref = TaskRef::new(task);
            runtime.tasks.push_back(task_ref.clone());
            JoinHandle::new(task_ref)
        })
    }
}

///
/// Same as [`spawn`], but from reusable future. No allocation is done for a future using this API.
///
pub fn spawn_from_reusable<T>(reusable: ReusableBoxFuture<T>) -> JoinHandle<T>
where
    T: Send + 'static,
{
    if DEV_CODE_ALLOW_ROUTING_OVER_MOCK.load(atomic::Ordering::Relaxed) {
        crate::spawn_from_reusable(reusable)
    } else {
        MOCK_RUNTIME.with(|instance| {
            let mut instance = instance.borrow_mut();

            let runtime = instance.as_mut().expect("Runtime mock instance is not initialized. Did you call `init`?");

            let task = Arc::new(AsyncTask::new(reusable.into_pin(), 0, runtime.sched.clone()));
            let task_ref = TaskRef::new(task);
            runtime.tasks.push_back(task_ref.clone());
            JoinHandle::new(task_ref)
        })
    }
}

// Thread-local storage to track task safety errors
thread_local! {
    static THREAD_CTX_SAFETY_ERROR_FLAG: RefCell<bool> = const { RefCell::new(false) };
}

pub fn ctx_set_task_safety_error(is_error: bool) {
    THREAD_CTX_SAFETY_ERROR_FLAG.with(|flag| {
        *flag.borrow_mut() = is_error;
    });
}

pub fn ctx_check_task_safety_error_and_clear() -> bool {
    THREAD_CTX_SAFETY_ERROR_FLAG.with(|flag| {
        let current = *flag.borrow();
        *flag.borrow_mut() = false;
        current
    })
}

///
/// Spawns a given `future` into runtime and let it execute on dedicated worker using `worker_id`.
/// This function allocates a `future` dynamically using [`Box`]
///
pub fn spawn_on_dedicated<T>(future: T, worker_id: UniqueWorkerId) -> JoinHandle<T::Output>
where
    T: Future + 'static + Send,
    T::Output: Send + 'static,
{
    let boxed = box_future(future);
    spawn_from_boxed_on_dedicated(boxed, worker_id)
}

///
/// Same as [`spawn_on_dedicated`], but from already boxed future. No allocation is done for a future using this API.
///
pub fn spawn_from_boxed_on_dedicated<T>(boxed: FutureBox<T>, _worker_id: UniqueWorkerId) -> JoinHandle<T>
where
    T: Send + 'static,
{
    if DEV_CODE_ALLOW_ROUTING_OVER_MOCK.load(atomic::Ordering::Relaxed) {
        crate::spawn_from_boxed_on_dedicated(boxed, _worker_id)
    } else {
        MOCK_RUNTIME.with(|instance| {
            let mut instance = instance.borrow_mut();

            let runtime = instance.as_mut().expect("Runtime mock instance is not initialized. Did you call `init`?");

            let task = Arc::new(AsyncTask::new(boxed, 0, runtime.sched.clone()));
            let task_ref = TaskRef::new(task);
            runtime.tasks.push_back(task_ref.clone());
            JoinHandle::new(task_ref)
        })
    }
}

///
/// Same as [`spawn_on_dedicated`], but from reusable future. No allocation is done for a future using this API.
///
pub fn spawn_from_reusable_on_dedicated<T>(reusable: ReusableBoxFuture<T>, _worker_id: UniqueWorkerId) -> JoinHandle<T>
where
    T: Send + 'static,
{
    if DEV_CODE_ALLOW_ROUTING_OVER_MOCK.load(atomic::Ordering::Relaxed) {
        crate::spawn_from_reusable_on_dedicated(reusable, _worker_id)
    } else {
        MOCK_RUNTIME.with(|instance| {
            let mut instance = instance.borrow_mut();

            let runtime = instance.as_mut().expect("Runtime mock instance is not initialized. Did you call `init`?");

            let task = Arc::new(AsyncTask::new(reusable.into_pin(), 0, runtime.sched.clone()));
            let task_ref = TaskRef::new(task);
            runtime.tasks.push_back(task_ref.clone());
            JoinHandle::new(task_ref)
        })
    }
}

pub mod safety {
    use super::*;

    use ::core::future::Future;

    use crate::{
        core::types::{box_future, FutureBox},
        futures::reusable_box_future::ReusableBoxFuture,
        scheduler::join_handle::JoinHandle,
    };

    use crate::safety::SafetyResult;

    pub fn ensure_safety_enabled() {}

    pub fn spawn<F, T, E>(future: F) -> JoinHandle<F::Output>
    where
        F: Future<Output = SafetyResult<T, E>> + 'static + Send,
        F::Output: Send + 'static,
        E: Send + 'static,
        T: Send + 'static,
    {
        let boxed = box_future(future);
        spawn_from_boxed(boxed)
    }

    pub fn spawn_from_boxed<T, E>(boxed: FutureBox<SafetyResult<T, E>>) -> JoinHandle<SafetyResult<T, E>>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        if DEV_CODE_ALLOW_ROUTING_OVER_MOCK.load(atomic::Ordering::Relaxed) {
            crate::safety::spawn_from_boxed(boxed)
        } else {
            MOCK_RUNTIME.with(|instance| {
                let mut instance = instance.borrow_mut();

                let runtime = instance.as_mut().expect("Runtime mock instance is not initialized. Did you call `init`?");

                let task = Arc::new(AsyncTask::new_safety(true, boxed, 0, runtime.sched.clone()));
                let task_ref = TaskRef::new(task);
                runtime.tasks.push_back(task_ref.clone());
                JoinHandle::new(task_ref)
            })
        }
    }

    pub fn spawn_from_reusable<T, E>(reusable: ReusableBoxFuture<SafetyResult<T, E>>) -> JoinHandle<SafetyResult<T, E>>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        if DEV_CODE_ALLOW_ROUTING_OVER_MOCK.load(atomic::Ordering::Relaxed) {
            crate::safety::spawn_from_reusable(reusable)
        } else {
            MOCK_RUNTIME.with(|instance| {
                let mut instance = instance.borrow_mut();

                let runtime = instance.as_mut().expect("Runtime mock instance is not initialized. Did you call `init`?");

                let task = Arc::new(AsyncTask::new_safety(true, reusable.into_pin(), 0, runtime.sched.clone()));
                let task_ref = TaskRef::new(task);
                runtime.tasks.push_back(task_ref.clone());
                JoinHandle::new(task_ref)
            })
        }
    }

    pub fn spawn_on_dedicated<F, T, E>(future: F, worker_id: UniqueWorkerId) -> JoinHandle<F::Output>
    where
        F: Future<Output = SafetyResult<T, E>> + 'static + Send,
        F::Output: Send + 'static,
        E: Send + 'static,
        T: Send + 'static,
    {
        let boxed = box_future(future);
        spawn_from_boxed_on_dedicated(boxed, worker_id)
    }

    pub fn spawn_from_boxed_on_dedicated<T, E>(boxed: FutureBox<SafetyResult<T, E>>, _worker_id: UniqueWorkerId) -> JoinHandle<SafetyResult<T, E>>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        if DEV_CODE_ALLOW_ROUTING_OVER_MOCK.load(atomic::Ordering::Relaxed) {
            crate::safety::spawn_from_boxed_on_dedicated(boxed, _worker_id)
        } else {
            MOCK_RUNTIME.with(|instance| {
                let mut instance = instance.borrow_mut();

                let runtime = instance.as_mut().expect("Runtime mock instance is not initialized. Did you call `init`?");

                let task = Arc::new(AsyncTask::new_safety(true, boxed, 0, runtime.sched.clone()));
                let task_ref = TaskRef::new(task);
                runtime.tasks.push_back(task_ref.clone());
                JoinHandle::new(task_ref)
            })
        }
    }

    pub fn spawn_from_reusable_on_dedicated<T, E>(
        reusable: ReusableBoxFuture<SafetyResult<T, E>>,
        _worker_id: UniqueWorkerId,
    ) -> JoinHandle<SafetyResult<T, E>>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        if DEV_CODE_ALLOW_ROUTING_OVER_MOCK.load(atomic::Ordering::Relaxed) {
            crate::safety::spawn_from_reusable_on_dedicated(reusable, _worker_id)
        } else {
            MOCK_RUNTIME.with(|instance| {
                let mut instance = instance.borrow_mut();

                let runtime = instance.as_mut().expect("Runtime mock instance is not initialized. Did you call `init`?");

                let task = Arc::new(AsyncTask::new_safety(true, reusable.into_pin(), 0, runtime.sched.clone()));
                let task_ref = TaskRef::new(task);
                runtime.tasks.push_back(task_ref.clone());
                JoinHandle::new(task_ref)
            })
        }
    }
}
