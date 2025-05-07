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

pub mod dedicated_worker;
pub mod worker;
pub mod worker_types;

use crate::scheduler::{
    workers::worker_types::{WorkerId, WorkerType},
    SchedulerType,
};
use iceoryx2_bb_posix::thread::{Thread, ThreadBuilder, ThreadName, ThreadSpawnError};
use std::fmt::Debug;

#[derive(Default)]
pub(crate) struct ThreadParameters {
    pub(crate) priority: Option<u8>,
    pub(crate) scheduler_type: Option<SchedulerType>,
    pub(crate) affinity: Option<usize>,
    pub(crate) stack_size: Option<u64>,
}

pub(crate) fn spawn_thread<T, F>(id: &WorkerId, f: F, thread_params: &ThreadParameters) -> Result<Thread, ThreadSpawnError>
where
    T: Debug + Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let mut name = match id.typ() {
        WorkerType::Async => ThreadName::from(b"aworker_"),
        WorkerType::Dedicated => ThreadName::from(b"dworker_"),
    };

    for digit in id.worker_id().to_string().into_bytes() {
        let _ = name.push(digit);
    }

    let mut tb = ThreadBuilder::new().name(&name);
    if let Some(priority) = thread_params.priority {
        tb = tb.priority(priority);
    }

    if let Some(scheduler_type) = &thread_params.scheduler_type {
        tb = tb.scheduler(scheduler_type.into());
    }

    if let Some(affinity) = thread_params.affinity {
        tb = tb.affinity(affinity);
    }

    if let Some(stack_size) = thread_params.stack_size {
        return tb.stack_size(stack_size).spawn(f);
    }

    tb.spawn(f)
}
