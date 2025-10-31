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
load("@score_orchestrator_crates//:defs.bzl", "all_crate_deps")

_COMMON_DEPS = [
    "//src/kyron-foundation:libkyron_foundation",
] + all_crate_deps(
    normal = True,
)

rust_library(
    name = "libkyron",
    srcs = glob(["src/**/*.rs"]),
    crate_features = select({
        "//conditions:default": [],
        "//src/flags:runtime_disable_time_correction": ["disable_vm_correction"],
    }),
    crate_name = "kyron",
    proc_macro_deps = [
        "//src/kyron-macros:runtime_macros",
    ],
    visibility = ["//visibility:public"],
    deps = _COMMON_DEPS,
)

rust_library(
    name = "libkyron_mocked",
    testonly = True,
    srcs = glob(["src/**/*.rs"]),
    crate_features = ["runtime-api-mock"],
    crate_name = "kyron",
    proc_macro_deps = [
        "//src/kyron-macros:runtime_macros",
    ],
    visibility = ["//visibility:public"],
    deps = _COMMON_DEPS,
)

rust_test(
    name = "tests",
    crate = ":libkyron",
    deps = all_crate_deps(
        normal = True,
    ) + [
        "//src/kyron:libkyron",
        "//src/kyron-testing:test_utils",
    ],
)
