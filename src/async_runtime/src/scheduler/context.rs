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

use crate::core::types::BoxCustom;
use crate::core::types::FutureBox;
use crate::futures::reusable_box_future::ReusableBoxFuture;
use crate::safety::SafetyResult;
use crate::scheduler::driver::Drivers;
use core::cell::Cell;
use core::cell::RefCell;
use foundation::containers::spmc_queue::BoundProducerConsumer;
use std::future::Future;

use std::pin::Pin;
use std::{rc::Rc, sync::Arc};

use crate::core::types::TaskId;
use crate::AsyncTask;
use crate::JoinHandle;

use super::scheduler_mt::DedicatedScheduler;
use super::scheduler_mt::DedicatedSchedulerLocal;

use super::workers::worker_types::WorkerId;
use super::{scheduler_mt::AsyncScheduler, task::async_task::TaskRef};
use crate::core::types::UniqueWorkerId;

enum HandlerImpl {
    Async(AsyncInnerHandler),
    Dedicated(DedicatedInnerHandler),
}

///
/// Simple container for a Handler that is crated by async worker
///
struct AsyncInnerHandler {
    scheduler: Arc<AsyncScheduler>,
    prod_con: Rc<BoundProducerConsumer<TaskRef>>, // Worker queue producer-consumer object
    dedicated_scheduler: Arc<DedicatedScheduler>,
}

///
/// Simple container for a Handler that is crated by dedicated worker
///
struct DedicatedInnerHandler {
    scheduler: Arc<AsyncScheduler>,
    dedicated_scheduler: Arc<DedicatedScheduler>,
}

///
/// Class exposed to code via TLS context. This provides all information needed to schedule, track & execute tasks on any type of worker within runtime.
///
pub struct Handler {
    inner: HandlerImpl,
}

///
/// Implements proxy API between Runtime API and schedulers
///
impl Handler {
    pub(crate) fn spawn<T>(&self, boxed: FutureBox<T>) -> JoinHandle<T>
    where
        T: Send + 'static,
    {
        self.internal(boxed, |f, id, scheduler| Arc::new(AsyncTask::new(f, id, scheduler)))
    }

    pub(crate) fn spawn_reusable<T>(&self, reusable: ReusableBoxFuture<T>) -> JoinHandle<T>
    where
        T: Send + 'static,
    {
        self.reusable_safety_internal(reusable, |reusable, id, scheduler| Arc::new(AsyncTask::new(reusable, id, scheduler)))
    }

    pub(crate) fn spawn_on_dedicated<T>(&self, boxed: FutureBox<T>, worker_id: UniqueWorkerId) -> JoinHandle<T>
    where
        T: Send + 'static,
    {
        self.on_dedicated_internal(boxed, worker_id, |f, id, scheduler| Arc::new(AsyncTask::new(f, id, scheduler)))
    }

    pub(crate) fn spawn_reusable_on_dedicated<T>(&self, reusable: ReusableBoxFuture<T>, worker_id: UniqueWorkerId) -> JoinHandle<T>
    where
        T: Send + 'static,
    {
        self.reusable_on_dedicated_internal(reusable, worker_id, |reusable, id, scheduler| {
            Arc::new(AsyncTask::new(reusable, id, scheduler))
        })
    }

    pub(crate) fn spawn_safety<T, E>(&self, boxed: FutureBox<SafetyResult<T, E>>) -> JoinHandle<SafetyResult<T, E>>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        self.internal(boxed, |f, id, scheduler| {
            Arc::new(AsyncTask::new_safety(ctx_is_with_safety(), f, id, scheduler))
        })
    }

    pub(crate) fn spawn_reusable_safety<T, E>(&self, reusable: ReusableBoxFuture<SafetyResult<T, E>>) -> JoinHandle<SafetyResult<T, E>>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        self.reusable_safety_internal(reusable, |reusable, id, scheduler| {
            Arc::new(AsyncTask::new_safety(ctx_is_with_safety(), reusable, id, scheduler))
        })
    }

    pub(crate) fn spawn_on_dedicated_safety<T, E>(
        &self,
        boxed: FutureBox<SafetyResult<T, E>>,
        worker_id: UniqueWorkerId,
    ) -> JoinHandle<SafetyResult<T, E>>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        self.on_dedicated_internal(boxed, worker_id, |f, id, scheduler| {
            Arc::new(AsyncTask::new_safety(ctx_is_with_safety(), f, id, scheduler))
        })
    }

    pub(crate) fn spawn_reusable_on_dedicated_safety<T, E>(
        &self,
        reusable: ReusableBoxFuture<SafetyResult<T, E>>,
        worker_id: UniqueWorkerId,
    ) -> JoinHandle<SafetyResult<T, E>>
    where
        T: Send + 'static,
        E: Send + 'static,
    {
        self.reusable_on_dedicated_internal(reusable, worker_id, |reusable, id, scheduler| {
            Arc::new(AsyncTask::new_safety(ctx_is_with_safety(), reusable, id, scheduler))
        })
    }

    ///
    /// Calls `a` and tries to provide local producer if You are in async part. Otherwise `None` which means we are in some dedicated worker
    ///
    pub(super) fn try_with_local_producer<T: FnOnce(Option<&BoundProducerConsumer<TaskRef>>)>(&self, a: T) {
        match self.inner {
            HandlerImpl::Async(ref async_inner) => {
                a(Some(&async_inner.prod_con));
            }
            HandlerImpl::Dedicated(_) => {
                a(None);
            }
        }
    }

    fn internal<T>(
        &self,
        boxed: FutureBox<T>,
        c: impl Fn(FutureBox<T>, u8, Arc<AsyncScheduler>) -> Arc<AsyncTask<T, BoxCustom<dyn Future<Output = T> + Send>, Arc<AsyncScheduler>>>,
    ) -> JoinHandle<T>
    where
        T: Send + 'static,
    {
        let task_ref;
        let handle;

        match self.inner {
            HandlerImpl::Async(ref async_inner) => {
                let task = c(boxed, ctx_get_worker_id().worker_id(), async_inner.scheduler.clone());
                task_ref = TaskRef::new(task.clone());
                handle = JoinHandle::new(task_ref.clone());

                async_inner.scheduler.spawn_from_runtime(task_ref, &async_inner.prod_con);
            }
            HandlerImpl::Dedicated(ref dedicated_inner) => {
                let task = c(boxed, ctx_get_worker_id().worker_id(), dedicated_inner.scheduler.clone());
                task_ref = TaskRef::new(task.clone());
                handle = JoinHandle::new(task_ref.clone());

                dedicated_inner.scheduler.spawn_outside_runtime(task_ref);
            }
        }

        handle
    }

    fn reusable_safety_internal<T>(
        &self,
        reusable: ReusableBoxFuture<T>,
        c: impl Fn(Pin<ReusableBoxFuture<T>>, u8, Arc<AsyncScheduler>) -> Arc<AsyncTask<T, ReusableBoxFuture<T>, Arc<AsyncScheduler>>>,
    ) -> JoinHandle<T>
    where
        T: Send + 'static,
    {
        let task_ref;
        let handle;

        match self.inner {
            HandlerImpl::Async(ref async_inner) => {
                let task = c(reusable.into_pin(), ctx_get_worker_id().worker_id(), async_inner.scheduler.clone());
                task_ref = TaskRef::new(task.clone());
                handle = JoinHandle::new(task_ref.clone());

                async_inner.scheduler.spawn_from_runtime(task_ref, &async_inner.prod_con);
            }
            HandlerImpl::Dedicated(ref dedicated_inner) => {
                let task = c(reusable.into_pin(), ctx_get_worker_id().worker_id(), dedicated_inner.scheduler.clone());
                task_ref = TaskRef::new(task.clone());
                handle = JoinHandle::new(task_ref.clone());

                dedicated_inner.scheduler.spawn_outside_runtime(task_ref);
            }
        }

        handle
    }

    fn on_dedicated_internal<T>(
        &self,
        boxed: FutureBox<T>,
        worker_id: UniqueWorkerId,
        c: impl Fn(FutureBox<T>, u8, DedicatedSchedulerLocal) -> Arc<AsyncTask<T, BoxCustom<dyn Future<Output = T> + Send>, DedicatedSchedulerLocal>>,
    ) -> JoinHandle<T>
    where
        T: Send + 'static,
    {
        let scheduler = match self.inner {
            HandlerImpl::Async(ref async_inner) => &async_inner.dedicated_scheduler,
            HandlerImpl::Dedicated(ref dedicated_inner) => &dedicated_inner.dedicated_scheduler,
        };

        let task = c(
            boxed,
            ctx_get_worker_id().worker_id(),
            DedicatedSchedulerLocal::new(worker_id, scheduler.clone()),
        );

        let task_ref = TaskRef::new(task.clone());
        let handle = JoinHandle::new(task_ref.clone());

        let ret = scheduler.spawn(task_ref, worker_id);
        assert!(ret, "Tried to spawn on not registered dedicated worker {:?}", worker_id);

        handle
    }

    fn reusable_on_dedicated_internal<T>(
        &self,
        reusable: ReusableBoxFuture<T>,
        worker_id: UniqueWorkerId,
        c: impl Fn(Pin<ReusableBoxFuture<T>>, u8, DedicatedSchedulerLocal) -> Arc<AsyncTask<T, ReusableBoxFuture<T>, DedicatedSchedulerLocal>>,
    ) -> JoinHandle<T>
    where
        T: Send + 'static,
    {
        let scheduler = match self.inner {
            HandlerImpl::Async(ref async_inner) => &async_inner.dedicated_scheduler,
            HandlerImpl::Dedicated(ref dedicated_inner) => &dedicated_inner.dedicated_scheduler,
        };

        let task = c(
            reusable.into_pin(),
            ctx_get_worker_id().worker_id(),
            DedicatedSchedulerLocal::new(worker_id, scheduler.clone()),
        );

        let task_ref = TaskRef::new(task.clone());

        let handle = JoinHandle::new(task_ref.clone());

        let ret = scheduler.spawn(task_ref, worker_id);
        assert!(ret, "Tried to spawn on not registered dedicated worker {:?}", worker_id);

        handle
    }

    pub(crate) fn unpark_some_async_worker(&self) {
        match self.inner {
            HandlerImpl::Async(ref sched) => {
                sched.scheduler.try_notify_siblings_workers(None);
            }
            HandlerImpl::Dedicated(ref sched) => {
                // Nothing to do here, dedicated workers are not parked
                sched.scheduler.try_notify_siblings_workers(None);
            }
        }
    }
}

///
/// This is an entry point for public API that is filled by each worker once it's created
///
pub(crate) struct WorkerContext {
    #[allow(dead_code)] // used in the tests
    /// The ID of task that is currently run by worker
    running_task_id: Cell<Option<TaskId>>,

    /// WorkerID and EngineID
    worker_id: Cell<WorkerId>,

    /// Access to scheduler and others, mounted in pre_run phase of each Worker
    pub(super) handler: RefCell<Option<Rc<Handler>>>,

    pub(super) drivers: Option<Drivers>,

    /// Helper flag to check if safety was enabled in runtime builder
    is_safety_enabled: bool,

    wakeup_time: Cell<Option<u64>>,
}

thread_local! {
    static CTX: RefCell<Option<WorkerContext>> = const {RefCell::new(None)}
}

pub(crate) struct ContextBuilder {
    tid: u64,
    handle: Option<Handler>,
    worker_id: Option<WorkerId>,
    is_with_safety: bool,
    drivers: Drivers,
}

impl ContextBuilder {
    pub(crate) fn new(drivers: Drivers) -> Self {
        Self {
            tid: 0,
            handle: None,
            worker_id: None,
            is_with_safety: false,
            drivers,
        }
    }

    pub(crate) fn thread_id(mut self, id: u64) -> Self {
        self.tid = id;
        self
    }

    pub(crate) fn with_worker_id(mut self, t: WorkerId) -> Self {
        self.worker_id = Some(t);
        self
    }

    #[allow(dead_code)]
    pub(crate) fn with_safety(mut self) -> Self {
        self.is_with_safety = true;
        self
    }

    pub(super) fn with_async_handle(
        mut self,
        pc: Rc<BoundProducerConsumer<TaskRef>>,
        s: Arc<AsyncScheduler>,
        dedicated_scheduler: Arc<DedicatedScheduler>,
    ) -> Self {
        assert!(self.handle.is_none(), "Cannot set two handlers for single context");
        let handle = Handler {
            inner: HandlerImpl::Async(AsyncInnerHandler {
                prod_con: pc,
                scheduler: s,
                dedicated_scheduler,
            }),
        };

        self.handle = Some(handle);
        self
    }

    pub(super) fn with_dedicated_handle(mut self, s: Arc<AsyncScheduler>, dedicated_scheduler: Arc<DedicatedScheduler>) -> Self {
        assert!(self.handle.is_none(), "Cannot set two handlers for single context");

        let handle = Handler {
            inner: HandlerImpl::Dedicated(DedicatedInnerHandler {
                scheduler: s,
                dedicated_scheduler,
            }),
        };

        self.handle = Some(handle);
        self
    }

    pub(crate) fn build(self) -> WorkerContext {
        WorkerContext {
            running_task_id: Cell::new(None),
            worker_id: Cell::new(self.worker_id.expect("Worker type must be set in context builder!")),
            handler: RefCell::new(Some(Rc::new(self.handle.expect("Handler type must be set in context builder!")))),
            is_safety_enabled: self.is_with_safety,
            wakeup_time: Cell::new(None),
            drivers: Some(self.drivers),
        }
    }
}

///
/// Needs to be called at worker thread init, so runtime API is functional from worker context
///
pub(crate) fn ctx_initialize(builder: ContextBuilder) {
    let _ = CTX
        .try_with(|ctx| {
            let prev = ctx.replace(Some(builder.build()));
            if prev.is_some() {
                panic!("Double init of WorkerContext is not allowed!");
            }
        })
        .map_err(|e| {
            panic!("Something is really bad here, error {}!", e);
        });
}

///
/// Returns `Handler` for scheduler or None if not in context.
///
pub(crate) fn ctx_get_handler() -> Option<Rc<Handler>> {
    CTX.try_with(|ctx| ctx.borrow().as_ref()?.handler.borrow().as_ref().cloned())
        .unwrap_or(None)
}

///
/// Worker id bound to this context
///
pub(crate) fn ctx_get_worker_id() -> WorkerId {
    CTX.try_with(|ctx| {
        let handler = ctx.borrow();
        let b = handler.as_ref().expect("Called before CTX init?");

        b.worker_id.get()
    })
    .unwrap_or_else(|e| {
        panic!("Something is really bad here, error {}!", e);
    })
}

///
/// Check if safety was enabled
///
pub(crate) fn ctx_is_with_safety() -> bool {
    CTX.try_with(|ctx| ctx.borrow().as_ref().expect("Called before CTX init?").is_safety_enabled)
        .unwrap_or_default()
}

pub(crate) fn ctx_set_wakeup_time(time: u64) {
    CTX.try_with(|ctx| ctx.borrow().as_ref().expect("Called before CTX init?").wakeup_time.set(Some(time)))
        .unwrap_or_default();
}

///
/// Returns currently set wakeup time for worker
///
pub(crate) fn ctx_get_wakeup_time() -> u64 {
    CTX.try_with(|ctx| {
        ctx.borrow()
            .as_ref()
            .expect("Called before CTX init?")
            .wakeup_time
            .get()
            .unwrap_or_default()
    })
    .unwrap_or_default()
}

pub(crate) fn ctx_unset_wakeup_time() {
    CTX.try_with(|ctx| ctx.borrow().as_ref().expect("Called before CTX init?").wakeup_time.take())
        .unwrap_or_default();
}

pub(crate) fn ctx_get_drivers() -> Drivers {
    CTX.try_with(|ctx| ctx.borrow().as_ref().expect("Called before CTX init?").drivers.clone().unwrap())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    ///
    /// Sets currently running `task id`
    ///
    pub(crate) fn ctx_set_running_task_id(id: TaskId) {
        let _ = CTX
            .try_with(|ctx| {
                ctx.borrow().as_ref().expect("Called before CTX init?").running_task_id.replace(Some(id));
            })
            .map_err(|e| {
                panic!("Something is really bad here, error {}!", e);
            });
    }

    ///
    /// Clears currently running `task id`
    ///
    pub(crate) fn ctx_unset_running_task_id() {
        let _ = CTX
            .try_with(|ctx| {
                ctx.borrow().as_ref().expect("Called before CTX init?").running_task_id.replace(None);
            })
            .map_err(|_| {});
    }

    ///
    /// Gets currently running `task id`
    ///
    pub(crate) fn ctx_get_running_task_id() -> Option<TaskId> {
        CTX.try_with(|ctx| ctx.borrow().as_ref().expect("Called before CTX init?").running_task_id.get())
            .unwrap_or_else(|e| {
                panic!("Something is really bad here, error {}!", e);
            })
    }

    #[test]
    fn test_context_no_init_panic_handler() {
        assert!(ctx_get_handler().is_none());
    }

    #[test]
    #[should_panic]
    fn test_context_no_init_panic_task_id() {
        ctx_get_running_task_id();
    }

    #[test]
    #[should_panic]
    fn test_context_no_init_panic() {
        ctx_get_worker_id();
    }

    #[test]
    #[should_panic]
    fn test_context_no_init_panic_set_task_id() {
        ctx_set_running_task_id(TaskId::new(1));
    }

    #[test]
    #[should_panic]
    fn test_context_no_init_panic_unset_task_id() {
        ctx_unset_running_task_id();
    }
}
