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

pub mod context;
pub mod execution_engine;
pub mod join_handle;
pub mod scheduler_mt;
pub mod task;
pub mod waker;
pub(crate) mod workers;

#[derive(Clone)]
pub enum SchedulerType {
    Fifo,
    RoundRobin,
    Other,
}

impl From<&SchedulerType> for iceoryx2_bb_posix::scheduler::Scheduler {
    fn from(scheduler_type: &SchedulerType) -> Self {
        match scheduler_type {
            SchedulerType::Fifo => Self::Fifo,
            SchedulerType::RoundRobin => Self::RoundRobin,
            SchedulerType::Other => Self::Other,
        }
    }
}
