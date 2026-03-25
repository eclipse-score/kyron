# *******************************************************************************
# Copyright (c) 2025 Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
# SPDX-License-Identifier: Apache-2.0
# *******************************************************************************
load("@rules_rust//rust:defs.bzl", "rust_library")

rust_library(
    name = "liblogging_tracing",
    srcs = glob(["src/**/*.rs"]),
    crate_features = select({
        "//src/flags:logging_feature_tracing": [
            "bazel_build_iceoryx2_qnx8",
            "tracing",
        ],
        "//src/flags:logging_feature_log": [
            "bazel_build_iceoryx2_qnx8",
            "log",
        ],
        "//src/flags:logging_feature_score_log": [
            "score-log",
            "score-log-bridge",
        ],
        "//conditions:default": [],
    }),
    crate_name = "logging_tracing",
    visibility = ["//visibility:public"],
    deps = select({
        "//src/flags:logging_feature_tracing": [
            "@score_crates//:iceoryx2_qnx8",
            "@score_crates//:tracing",
            "@score_crates//:tracing_subscriber",
        ],
        "//src/flags:logging_feature_log": [
            "//src/kyron-foundation:libkyron_foundation",
            "@score_crates//:env_logger",
            "@score_crates//:iceoryx2_qnx8",
        ],
        "//src/flags:logging_feature_score_log": [
            "@score_baselibs_rust//src/log/score_log",
            "@score_logging//score/mw/log/rust/score_log_bridge",
        ],
        "//conditions:default": [],
    }),
)
