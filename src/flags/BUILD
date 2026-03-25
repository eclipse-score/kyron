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

load("@bazel_skylib//rules:common_settings.bzl", "bool_flag", "string_flag")

bool_flag(
    name = "k_runtime_disable_time_correction",
    build_setting_default = False,
)

# Explicit time correction disabling needed for Linux VMs on Windows. As this applies to all
# x86_64 it may be not needed for native Linux on x86_64
config_setting(
    name = "runtime_disable_time_correction",
    flag_values = {
        ":k_runtime_disable_time_correction": "True",
    },
    visibility = ["//visibility:public"],
)

# Flag to enable IPC mechanisms, it is enabled by default
bool_flag(
    name = "k_enable_ipc",
    build_setting_default = True,
)

config_setting(
    name = "enable_ipc",
    flag_values = {
        ":k_enable_ipc": "True",
    },
    visibility = ["//visibility:public"],
)

# To select logging feature
string_flag(
    name = "logging_feature",
    build_setting_default = "tracing",
    visibility = ["//visibility:public"],
)

config_setting(
    name = "logging_feature_tracing",
    flag_values = {
        ":logging_feature": "tracing",
    },
)

config_setting(
    name = "logging_feature_log",
    flag_values = {
        ":logging_feature": "log",
    },
)

config_setting(
    name = "logging_feature_score_log",
    flag_values = {
        ":logging_feature": "score-log",
    },
)
