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

use async_runtime::spawn;
use async_runtime_macros::main;
use foundation::prelude::*;

// A simple example for main macro usage with all default engine parameters
#[main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).with_max_level(Level::INFO).init();

    let mut handles = Vec::new(10);

    for i in 0..handles.capacity() {
        let handle = spawn(async move {
            info!("Message from task {}", i);
        });
        handles.push(handle);
    }
    for handle in handles.iter_mut() {
        let _ = handle.await;
    }
}
