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
load("@rules_rust//rust:defs.bzl", "rust_library", "rust_test")

rust_library(
    name = "libkyron_foundation",
    srcs = glob(["src/**/*.rs"]),
    crate_features = select({
        "//src/flags:logging_feature_tracing": [
            "tracing",
        ],
        "//src/flags:logging_feature_log": [
            "log",
        ],
        "//src/flags:logging_feature_score_log": [
            "score-log",
        ],
        "//conditions:default": [],
    }) + [
        "bazel_build_iceoryx2_qnx8",
    ],
    crate_name = "kyron_foundation",
    proc_macro_deps = [
        "@score_crates//:iceoryx2_bb_derive_macros_qnx8",
    ],
    visibility = ["//visibility:public"],
    deps = select({
        "//src/flags:logging_feature_tracing": [
            "@score_crates//:tracing",
        ],
        "//src/flags:logging_feature_log": [
            "@score_crates//:log",
        ],
        "//src/flags:logging_feature_score_log": [
            "@score_baselibs_rust//src/log/score_log",
        ],
        "//conditions:default": [],
    }) + [
        "@score_crates//:iceoryx2_bb_container_qnx8",
        "@score_crates//:iceoryx2_bb_elementary_qnx8",
        "@score_crates//:iceoryx2_bb_elementary_traits_qnx8",
        "@score_crates//:iceoryx2_bb_lock_free_qnx8",
        "@score_crates//:iceoryx2_bb_memory_qnx8",
        "@score_crates//:iceoryx2_bb_posix_qnx8",
        "@score_crates//:iceoryx2_bb_system_types_qnx8",
        "@score_crates//:iceoryx2_bb_testing_qnx8",
        "@score_crates//:iceoryx2_bb_threadsafe_qnx8",
        "@score_crates//:iceoryx2_pal_concurrency_sync_qnx8",
    ],
)

rust_test(
    name = "tests",
    crate = ":libkyron_foundation",
    crate_features = select({
        "//src/flags:logging_feature_tracing": [
            "tracing",
        ],
        "//src/flags:logging_feature_log": [
            "log",
        ],
        "//src/flags:logging_feature_score_log": [
            "score-log",
        ],
        "//conditions:default": [],
    }) + [
        "bazel_build_iceoryx2_qnx8",
    ],
)
