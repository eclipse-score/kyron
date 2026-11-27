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
# project_config.bzl
PROJECT_CONFIG = {
    # Current S-CORE release classification. The design targets ASIL_B; the
    # deferred safety work products are listed in docs/module/qm_tailoring.rst.
    "asil_level": "QM",
    "source_code": ["rust"],
}
