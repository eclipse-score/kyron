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

use crate::macros::log::*;
use crate::{
    common::types::{box_future, FutureBox, UniqueWorkerId},
    ctx_get_handler,
    futures::reusable_box_future::ReusableBoxFuture,
    scheduler::{context::ctx_get_worker_id, join_handle::JoinHandle},
};
use ::core::future::Future;
use kyron_foundation::not_recoverable_error;

pub type SafetyResult<T, E> = Result<T, E>;

///
/// Since `SafetyWorker` is optional within engine, all APIs from this file will fallback to regular API if `SafetyWorker` is not configured.
/// If your code requires `SafetyWorker`, please use `ensure_safety_enabled` as initial call in runtime to not allow start without safety worker.
///
///
pub fn ensure_safety_enabled() {
    not_recoverable_error!(
        on_cond(crate::scheduler::context::ctx_get_handler().is_some()),
        "For now we don't allow runtime API to be called outside of runtime"
    );

    not_recoverable_error!(
        on_cond(crate::scheduler::context::ctx_is_with_safety()),
        "Safety API is not enabled, please use runtime API or configure engine with safety"
    );
}

///
/// Spawns a given `future` into runtime and let it execute on any of configured workers
/// This function allocates a `future` dynamically using [`Box`]
///
/// # Safety
/// This API is intended to provide a way to ensure that user can react on errors within a `task` independent  of other workers state (ie. being busy looping etc).
/// This means that if the `task` (aka provided Future) will return Err(_), then the task that is awaiting on JoinHandle will be woken up in `SafetyWorker`.
///
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

///
/// Same as [`spawn`], but from already boxed future. No allocation is done for a future using this API.
///
/// # Safety
/// This API is intended to provide a way to ensure that user can react on errors within a `task` independent  of other workers state (ie. being busy looping etc).
/// This means that if the `task` (aka provided Future) will return Err(_), then the task that is awaiting on JoinHandle will be woken up in `SafetyWorker`.
///
pub fn spawn_from_boxed<T, E>(boxed: FutureBox<SafetyResult<T, E>>) -> JoinHandle<SafetyResult<T, E>>
where
    T: Send + 'static,
    E: Send + 'static,
{
    if let Some(handler) = ctx_get_handler() {
        not_recoverable_error!(
            on_cond(ctx_get_worker_id().typ() != crate::scheduler::workers::worker_types::WorkerType::Dedicated),
            "Cannot do safety spawn on dedicated worker as this may cause movement of task to another worker"
        );

        handler.spawn_safety(boxed)
    } else {
        not_recoverable_error!("For now we don't allow runtime API to be called outside of runtime")
    }
}

///
/// Same as [`spawn`], but from reusable future. No allocation is done for a future using this API.
///
/// # Safety
/// This API is intended to provide a way to ensure that user can react on errors within a `task` independent  of other workers state (ie. being busy looping etc).
/// This means that if the `task` (aka provided Future) will return Err(_), then the task that is awaiting on JoinHandle will be woken up in `SafetyWorker`.
///
pub fn spawn_from_reusable<T, E>(reusable: ReusableBoxFuture<SafetyResult<T, E>>) -> JoinHandle<SafetyResult<T, E>>
where
    T: Send + 'static,
    E: Send + 'static,
{
    if let Some(handler) = ctx_get_handler() {
        not_recoverable_error!(
            on_cond(ctx_get_worker_id().typ() != crate::scheduler::workers::worker_types::WorkerType::Dedicated),
            "Cannot do safety spawn on dedicated worker as this may cause movement of task to another worker"
        );

        handler.spawn_reusable_safety(reusable)
    } else {
        not_recoverable_error!("For now we don't allow runtime API to be called outside of runtime")
    }
}

///
/// Spawns a given `future` into runtime and let it execute on dedicated worker using `worker_id`.
/// This function allocates a `future` dynamically using [`Box`]
///
/// # Safety
/// This API is intended to provide a way to ensure that user can react on errors within a `task` independent  of other workers state (ie. being busy looping etc).
/// This means that if the `task` (aka provided Future) will return Err(_), then the task that is awaiting on JoinHandle will be woken up in `SafetyWorker`.
///
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

///
/// Same as [`spawn_on_dedicated`], but from already boxed future. No allocation is done for a future using this API.
///
/// # Safety
/// This API is intended to provide a way to ensure that user can react on errors within a `task` independent  of other workers state (ie. being busy looping etc).
/// This means that if the `task` (aka provided Future) will return Err(_), then the task that is awaiting on JoinHandle will be woken up in `SafetyWorker`.
///
pub fn spawn_from_boxed_on_dedicated<T, E>(
    boxed: FutureBox<SafetyResult<T, E>>,
    worker_id: UniqueWorkerId,
) -> JoinHandle<SafetyResult<T, E>>
where
    T: Send + 'static,
    E: Send + 'static,
{
    if let Some(handler) = ctx_get_handler() {
        not_recoverable_error!(
            on_cond(ctx_get_worker_id().typ() != crate::scheduler::workers::worker_types::WorkerType::Dedicated),
            "Cannot do safety spawn on dedicated worker as this may cause movement of task to another worker"
        );

        handler.spawn_on_dedicated_safety(boxed, worker_id)
    } else {
        not_recoverable_error!("For now we don't allow runtime API to be called outside of runtime")
    }
}

///
/// Same as [`spawn_on_dedicated`], but from reusable future. No allocation is done for a future using this API.
///
/// # Safety
/// This API is intended to provide a way to ensure that user can react on errors within a `task` independent  of other workers state (ie. being busy looping etc).
/// This means that if the `task` (aka provided Future) will return Err(_), then the task that is awaiting on JoinHandle will be woken up in `SafetyWorker`.
///
pub fn spawn_from_reusable_on_dedicated<T, E>(
    reusable: ReusableBoxFuture<SafetyResult<T, E>>,
    worker_id: UniqueWorkerId,
) -> JoinHandle<SafetyResult<T, E>>
where
    T: Send + 'static,
    E: Send + 'static,
{
    if let Some(handler) = ctx_get_handler() {
        not_recoverable_error!(
            on_cond(ctx_get_worker_id().typ() != crate::scheduler::workers::worker_types::WorkerType::Dedicated),
            "Cannot do safety spawn on dedicated worker as this may cause movement of task to another worker"
        );

        handler.spawn_reusable_on_dedicated_safety(reusable, worker_id)
    } else {
        not_recoverable_error!("For now we don't allow runtime API to be called outside of runtime")
    }
}
