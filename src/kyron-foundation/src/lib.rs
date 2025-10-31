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

use ::core::alloc::Layout;
use std::alloc::{self};

pub mod base;
pub mod cell;
pub mod containers;
pub mod macros;
pub mod prelude;
pub mod sync;
pub mod threading;
mod types;

pub fn create_arr_storage<Type, U: Fn(usize) -> Type>(size: usize, init: U) -> Box<[Type]> {
    let layout = Layout::array::<Type>(size).unwrap();

    // SAFETY: We are manually allocating memory here
    let ptr = unsafe { alloc::alloc(layout) as *mut Type };

    assert!(!ptr.is_null(), "Failed to allocate memory for array storage");

    for i in 0..size {
        unsafe {
            ptr.add(i).write(init(i)); // just filling with values as it has to be initialized
        }
    }

    // SAFETY: Create boxed slice from raw parts
    unsafe { Box::from_raw(::core::ptr::slice_from_raw_parts_mut(ptr, size)) }
}
