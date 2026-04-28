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
load("@bazel_skylib//lib:selects.bzl", "selects")
load("@rules_rust//rust:defs.bzl", "miri_binary", "miri_test", "rust_binary", "rust_library", "rust_test")

_COMMON_DEPS = [
    "//src/kyron-foundation:libkyron_foundation",
    "@score_crates//:libc",
]

_KYRON_DEPS = (
    _COMMON_DEPS +
    select({
        "//conditions:default": [],
        "//src/flags:enable_ipc": [
            "@score_crates//:iceoryx2_qnx8",
            "@score_crates//:iceoryx2_cal_qnx8",
        ],
    })
)

_EXAMPLE_DEPS = _COMMON_DEPS + [
    ":libkyron",
    "@score_crates//:tracing_subscriber",
]

rust_library(
    name = "libkyron",
    srcs = glob(["src/**/*.rs"]),
    crate_features = selects.with_or({
        "//src/flags:runtime_disable_time_correction": ["disable_vm_correction"],
        "//src/flags:enable_ipc": ["iceoryx2-ipc"],
    }) + [
        "bazel_build_iceoryx2_qnx8",
        "tracing",
    ],
    crate_name = "kyron",
    proc_macro_deps = [
        "//src/kyron-macros:runtime_macros",
    ],
    visibility = ["//visibility:public"],
    deps = _KYRON_DEPS,
)

rust_library(
    name = "libkyron_mocked",
    testonly = True,
    srcs = glob(["src/**/*.rs"]),
    crate_features = [
        "runtime-api-mock",
        "tracing",
    ],
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
    deps = [
        "//src/kyron:libkyron",
        "//src/kyron-testing:test_utils",
    ] + _COMMON_DEPS,
)

miri_test(
    name = "tests_miri",
    crate = ":tests",
    tags = ["manual"],
    visibility = ["//visibility:public"],
)

# Examples binaries
rust_binary(
    name = "select",
    srcs = [
        "examples/select.rs",
    ],
    proc_macro_deps = [
        "//src/kyron-macros:runtime_macros",
    ],
    visibility = ["//visibility:public"],
    deps = _EXAMPLE_DEPS,
)

miri_binary(
    name = "select_miri",
    crate = ":select",
    tags = ["manual"],
    visibility = ["//visibility:public"],
)

rust_binary(
    name = "main_macro",
    srcs = [
        "examples/main_macro.rs",
    ],
    proc_macro_deps = [
        "//src/kyron-macros:runtime_macros",
    ],
    visibility = ["//visibility:public"],
    deps = _EXAMPLE_DEPS,
)

miri_binary(
    name = "main_macro_miri",
    crate = ":main_macro",
    tags = ["manual"],
    visibility = ["//visibility:public"],
)

rust_binary(
    name = "mpmc",
    srcs = [
        "examples/mpmc.rs",
    ],
    proc_macro_deps = [
        "//src/kyron-macros:runtime_macros",
    ],
    visibility = ["//visibility:public"],
    deps = _EXAMPLE_DEPS,
)

miri_binary(
    name = "mpmc_miri",
    crate = ":mpmc",
    tags = ["manual"],
    visibility = ["//visibility:public"],
)

rust_binary(
    name = "safety_task",
    srcs = [
        "examples/safety_task.rs",
    ],
    proc_macro_deps = [
        "//src/kyron-macros:runtime_macros",
    ],
    visibility = ["//visibility:public"],
    deps = _EXAMPLE_DEPS,
)

miri_binary(
    name = "safety_task_miri",
    crate = ":safety_task",
    tags = ["manual"],
    visibility = ["//visibility:public"],
)
