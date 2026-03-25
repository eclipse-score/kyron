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

use kyron::{runtime::*, spawn};
use kyron_foundation::containers::vector_extension::VectorExtension;
use kyron_foundation::prelude::*;
use logging_tracing::LogAndTraceBuilder;

//
// This example demonstrates how to use the logging framework, which supports logging through tracing/log/score-log loggers.
//
fn main() {
    // Setup logging framework
    #[allow(unused_mut)]
    let mut logger_builder = LogAndTraceBuilder::new()
        .global_log_level(logging_tracing::Level::DEBUG) // Except for score_log bazel build, which needs to be set in the configuration file.
        .enable_logging(true);

    #[cfg(feature = "score-log")]
    {
        logger_builder = logger_builder
            .context("LOGG")
            .show_module(true)
            .show_file(true)
            .show_line(true);
    }

    let _logger = logger_builder.build().expect("Failed to build tracing library");

    let (builder, _engine_id) =
        kyron::runtime::RuntimeBuilder::new().with_engine(ExecutionEngineBuilder::new().task_queue_size(8).workers(1));
    let mut runtime = builder.build().unwrap();

    runtime.block_on(async {
        debug!("Start of runtime...");
        let mut handles = Vec::new_in_global(10);

        for i in 0..handles.capacity() {
            let handle = spawn(async move {
                info!("Message from task {}", i);
            });
            let _ = handles.push(handle);
        }
        for handle in handles.iter_mut() {
            let _ = handle.await;
        }
        debug!("Exit.")
    });
}
