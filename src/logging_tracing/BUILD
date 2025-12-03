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
load("@score_kyron_crates//:defs.bzl", "all_crate_deps")

rust_library(
    name = "liblogging_tracing",
    srcs = glob(["src/**/*.rs"]),
    crate_features = [
        "tracing",
    ],
    crate_name = "logging_tracing",
    visibility = ["//visibility:public"],
    deps = all_crate_deps(
        normal = True,
    ),
)
