..
   # *******************************************************************************
   # Copyright (c) 2024 Contributors to the Eclipse Foundation
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

Kyron
=============================

This documentation describes the **Kyron** - the functional safety async runtime for Rust

.. contents:: Table of Contents
   :depth: 2
   :local:

Overview
--------

Kyron is an ``async/await`` runtime for Rust, built with Bazel and Cargo. Its configurable
execution engines give fine-grained control over scheduling, thread management and
workload isolation.

Safety classification
---------------------

Kyron is designed for safety-critical use, with ASIL B as the long-term target.
Its current classification in the S-CORE release is **QM**. Kyron makes no ASIL claim
yet, and that includes the safety worker. The deferred safety work products and the
conditions that bring them back are listed in :doc:`module/qm_tailoring`.

.. toctree::
   :maxdepth: 1
   :hidden:

   module/qm_tailoring

Requirements
------------

- TODO add linkage once it works

Quick Start
-----------

To build the module for host platform:

.. code-block:: bash

   bazel build //src/...

To build the module for QNX8:

.. code-block:: bash

   ./scripts/build_qnx8.sh

To run component tests:

.. code-block:: bash

   ./scripts/run_component_tests.sh
