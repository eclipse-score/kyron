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

pub mod growable_vec;
pub mod intrusive_linked_list;
pub mod mpmc_queue;
mod queue;
pub mod reusable_objects;
pub mod reusable_vec_pool;
pub mod spmc_queue;
pub mod trigger_queue;
pub mod vector_extension;

pub use iceoryx2_bb_container::vector::*;

pub type Vec<T> = iceoryx2_bb_container::vector::PolymorphicVec<'static, T, iceoryx2_bb_memory::heap_allocator::HeapAllocator>;
pub use iceoryx2_bb_container::vector::Vector;
