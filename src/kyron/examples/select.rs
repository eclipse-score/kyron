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

use core::time::Duration;

use kyron::{
    futures::sleep::sleep,
    prelude::*,
    safety::{ensure_safety_enabled, spawn, SafetyResult},
    select,
};
use kyron_foundation::prelude::*;

fn main() {
    tracing_subscriber::fmt()
        // .with_span_events(FmtSpan::FULL) // Ensures span open/close events are logged
        .with_target(false) // Optional: Remove module path
        .with_max_level(Level::DEBUG)
        .with_thread_ids(true)
        .with_thread_names(true)
        .init();

    let (builder, _engine_id) = kyron::runtime::RuntimeBuilder::new().with_engine(
        ExecutionEngineBuilder::new()
            .task_queue_size(256)
            .workers(4)
            .with_dedicated_worker("dedicated".into(), ThreadParameters::default())
            .enable_safety_worker(ThreadParameters::default()),
    );
    let mut runtime = builder.build().unwrap();

    runtime.block_on(async {
        ensure_safety_enabled();

        // Sleeps instead of busy loops, whose runtime differences vanish under CPU load.
        async fn short_job(result: i32) -> SafetyResult<i32, ()> {
            sleep(Duration::from_millis(10)).await;
            Ok(result)
        }

        async fn long_job(result: i32) -> SafetyResult<i32, ()> {
            sleep(Duration::from_millis(300)).await;
            Ok(result)
        }

        async fn very_long_job(result: i32) -> SafetyResult<i32, ()> {
            sleep(Duration::from_millis(1000)).await;
            Ok(result)
        }

        // First job should finish first.
        {
            info!("Test 1");

            let fut1 = spawn(short_job(111));
            let fut2 = spawn(long_job(222));
            let fut3 = spawn(long_job(333));
            let mut result = 0;

            select! {
                Ok(Ok(var1)) = fut1 => {
                    result = var1;
                }
                Ok(Ok(var2)) = fut2 => {
                    result = var2;
                }
                Ok(Ok(var3)) = fut3 => {
                    result = var3;
                }
            }

            assert_eq!(result, 111);
        }

        // Second job should finish first.
        {
            info!("Test 2");

            let fut1 = spawn(long_job(111));
            let fut2 = spawn(short_job(222));
            let fut3 = spawn(long_job(333));
            let mut result = 0;

            select! {
                Ok(Ok(var1)) = fut1 => {
                    result = var1;
                }
                Ok(Ok(var2)) = fut2 => {
                    result = var2;
                }
                Ok(Ok(var3)) = fut3 => {
                    result = var3;
                }
            }

            assert_eq!(result, 222);
        }

        // First job should finish first, but it won't match, so the third job should match first.
        {
            info!("Test 3");

            let fut1 = spawn(short_job(111));
            let fut2 = spawn(very_long_job(222));
            let fut3 = spawn(long_job(333));
            let mut result = 0;

            select! {
                Ok(Err(())) = fut1 => {
                    result = 111;
                }
                Ok(Ok(var2)) = fut2 => {
                    result = var2;
                }
                Ok(Ok(var3)) = fut3 => {
                    result = var3;
                }
            }

            assert_eq!(result, 333);
        }
    });
}
