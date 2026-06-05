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
load("@score_qnx_unit_tests//:defs.bzl", "rust_test_qnx")

rust_library(
    name = "libkyron_foundation",
    srcs = glob(["src/**/*.rs"]),
    crate_features = [
        "bazel_build_iceoryx2_qnx8",
        "tracing",
    ],
    crate_name = "kyron_foundation",
    proc_macro_deps = [
        "@score_crates//:iceoryx2_bb_derive_macros_qnx8",
    ],
    visibility = ["//visibility:public"],
    deps = [
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
        "@score_crates//:tracing",
        "@score_crates//:tracing_subscriber",
    ],
)

rust_test(
    name = "tests",
    crate = ":libkyron_foundation",
    crate_features = [
        "bazel_build_iceoryx2_qnx8",
        "tracing",
    ],
)

rust_test_qnx(
    name = "tests_qnx",
    rust_test = ":tests",
)
