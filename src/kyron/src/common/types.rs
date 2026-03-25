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

use ::core::future::Future;
use ::core::pin::Pin;
use ::core::sync::atomic::AtomicU64;
use std::sync::Arc;

use crate::scheduler::workers::worker_types::WorkerId;

// Used to Box Futures
pub(crate) type BoxCustom<T> = Box<T>; // TODO: We shall replace Global allocator with our own. Since Allocator API is not stable, we shall provide own Box impl (only for internal purpose handling)

pub type FutureBox<T> = Pin<BoxCustom<dyn Future<Output = T> + Send>>;

pub fn box_future<T: Send, U: Future<Output = T> + 'static + Send>(fut: U) -> FutureBox<T> {
    BoxCustom::pin(fut)
}

// Both used internally, that may allocate at runtime from a previously bounded allocator.
pub(crate) type BoxInternal<T> = Box<T>; // TODO: Use mempool allocator, for now we keep default impl
pub(crate) type ArcInternal<T> = Arc<T>; // TODO: Use mempool allocator, for now we keep default impl

///
/// TaskId encodes the worker on which it was created and it is global to the process.
/// This id cannot be used to infer task order creation or anything like that, it's only for identification purpose.
///
#[derive(Copy, Clone, Debug)]
pub struct TaskId(pub(crate) u64);

impl TaskId {
    pub(crate) fn new(worker_id: &WorkerId) -> Self {
        let engine_id = worker_id.engine_id();
        let worker_id = worker_id.worker_id();
        static TASK_COUNTER: AtomicU64 = const { AtomicU64::new(0) };
        // Just increment the global counter, it wraps around on overflow. Only lower 48 bits are used for the TaskId.
        let val = TASK_COUNTER.fetch_add(1, ::core::sync::atomic::Ordering::Relaxed);
        Self((val << 16) | ((engine_id as u64) << 8) | worker_id as u64)
    }

    /// Get the worker id that created this task
    pub fn worker(&self) -> u8 {
        (self.0 & 0xFF_u64) as u8
    }

    /// Get the engine id that created this task
    pub fn engine(&self) -> u8 {
        ((self.0 >> 8) & 0xFF_u64) as u8
    }
}

kyron_foundation::import_score_log!();
kyron_foundation::derive_score_debug_for_struct!(
    #[derive(Copy, Clone, Debug, PartialEq)]
    pub struct UniqueWorkerId(u64);
);

impl From<&str> for UniqueWorkerId {
    fn from(value: &str) -> Self {
        Self(compute_hash(value))
    }
}

impl From<String> for UniqueWorkerId {
    fn from(value: String) -> Self {
        Self(compute_hash(value.as_str()))
    }
}

impl From<&String> for UniqueWorkerId {
    fn from(value: &String) -> Self {
        Self(compute_hash(value))
    }
}

fn compute_hash(data: &str) -> u64 {
    // TODO: for now use DJB2 hash
    let mut hash: u64 = 5381;
    for byte in data.bytes() {
        hash = (hash.wrapping_shl(5)).wrapping_add(hash).wrapping_add(byte as u64);
    }
    hash
}
