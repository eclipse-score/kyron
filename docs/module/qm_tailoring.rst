..
   # *******************************************************************************
   # Copyright (c) 2026 Contributors to the Eclipse Foundation
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

QM Classification and Safety Tailoring
======================================

.. note::

   **Status: draft.** This page needs agreement from the S-CORE Safety Manager and
   Security Manager. Once the process needs are wired into the documentation build, it
   becomes a ``doc__`` need that realizes ``wp__safety_tailoring``.

.. contents:: Table of Contents
   :depth: 2
   :local:

Purpose
-------

This page records the safety classification of the Kyron module for the current S-CORE
release. It also lists which work products of the
`S-CORE process <https://eclipse-score.github.io/process_description/main/>`_ Kyron provides
now, and which it defers.

Scope
-----

The released element consists of the following crates:

* ``src/kyron``: the runtime (engines, scheduler and workers, the safety worker, I/O driver,
  networking, time, channels, futures and ``select!``, and iceoryx2 IPC).
* ``src/kyron-macros``: the ``#[kyron::main]`` and ``#[kyron::test]`` procedural macros.
* ``src/kyron-foundation``: containers, synchronisation and threading primitives.

The following crates are test and development support. They are not part of the released
element: ``src/kyron-testing``, ``src/kyron-testing-macros``, ``src/logging_tracing`` and
``src/xtask``.

Classification
--------------

Kyron is **designed for safety-critical use**, with **ASIL B** as the long-term target.

For the current S-CORE release, Kyron is classified as **QM**:

* All Kyron requirements carry ``safety: QM``. Following the S-CORE linkage rules, they derive
  only from QM parent requirements.
* Kyron makes **no ASIL claim** in this release. This includes the *safety worker* and every
  other feature designed for safety use. An integrator must not rely on these features to
  achieve a safety goal until Kyron is qualified.

Work products provided for QM
-----------------------------

.. list-table::
   :header-rows: 1
   :widths: 30 15 55

   * - Work product
     - Status
     - Notes
   * - ``wp__requirements_comp``
     - Required
     - Component requirements (``comp_req``) derived from the orchestration feature requirements.
   * - ``wp__requirements_comp_aou``
     - Required
     - Assumptions of use for integrators (QM).
   * - ``wp__requirements_inspect``
     - Required
     - Checklist-based review in the pull request, with the ``inspected`` tag on the
       requirement. A separate manual inspection report is required only for ASIL work products.
   * - ``wp__component_arch``
     - Required
     - Static view and interfaces. Dynamic views are kept for non-trivial flows, including the
       safety worker, so the ASIL B path stays open.
   * - ``wp__sw_implementation``
     - Required
     - Source code. The detailed design lives in rustdoc next to the code.
   * - ``wp__sw_implementation_inspection``
     - Required
     - Pull request review with the implementation checklist.
   * - ``wp__verification_sw_unit_test``
     - Required
     - Rust unit tests, plus Miri and Loom runs.
   * - ``wp__verification_comp_int_test``
     - Required
     - Component integration tests (``tests/test_cases``) linked to requirements.
   * - ``wp__verification_module_ver_report``
     - Required
     - Includes structural coverage (C0/C1), static analysis, and complexity results against
       both the QM and the ASIL B limits.
   * - ``wp__module_sw_release_note``
     - Required
     - One per released version.
   * - ``wp__module_sw_release_plan``
     - Required
     -
   * - ``wp__module_user_manual``
     - Required
     -
   * - ``wp__sw_module_sbom``
     - To be decided
     - Security Manager decision. Kyron exposes TCP/UDP networking and iceoryx2 IPC.
   * - ``wp__module_security_plan``, ``wp__sw_component_security_analysis``,
       ``wp__module_security_package``, ``wp__module_security_manual``
     - To be decided
     - Security Manager decision, as for the SBOM.

Deferred safety work products
-----------------------------

The following work products are **deferred**, not dropped. They become required once Kyron is
qualified for ASIL B.

.. list-table::
   :header-rows: 1
   :widths: 35 65

   * - Work product
     - Rationale for deferral
   * - ``wp__module_safety_plan``
     - There is no ASIL claim in this release, so there are no safety activities to plan. The
       safety plan is the first work product created when qualification starts.
   * - ``wp__module_safety_package``
     - No safety argument is made for the current release.
   * - ``wp__module_safety_manual``
     - No safety-related use is supported yet. Integrators get the QM assumptions of use instead.
   * - ``wp__sw_component_fmea``, ``wp__sw_component_dfa``
     - Safety analyses are needed only for safety requirements, and the current release has none.
   * - ``wp__sw_component_class``
     - Classification of the pre-existing code is needed only for an ASIL claim.
   * - ``wp__fdr_reports``
     - There is no safety plan, package or analysis to review.
   * - Safety assumptions of use (``aou_req`` with ``safety: ASIL_B``)
     - No safety-related use is supported yet.
   * - Manual inspection reports
     - The process requires them only for ASIL-related artifacts.

Measures kept for the ASIL B path
---------------------------------

The following measures go beyond the QM minimum. They keep the cost of later qualification low:

* ``unsafe`` code is documented with ``SAFETY`` justifications.
* Concurrency-sensitive code is checked with Miri and Loom in CI.
* Complexity is reported against both the QM limits and the ASIL B limits.
* Dynamic architecture views are kept for the safety worker and the scheduler.

Re-activation trigger
---------------------

The deferred work products are re-activated by a change request to qualify Kyron for ASIL B,
agreed with the S-CORE Safety Manager. The first step is to create ``wp__module_safety_plan``,
which then schedules every deferred work product listed above.

Approval
--------

.. list-table::
   :header-rows: 1

   * - Role
     - Decision
   * - S-CORE Safety Manager
     - *pending*
   * - S-CORE Security Manager
     - *pending*
