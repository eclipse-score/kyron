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
from pathlib import Path
from platform import platform
from typing import Any

import pytest
from testing_utils import LogContainer, ScenarioResult, cap_utils

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)
from component_integration_tests.python_test_cases.tests.result_code import (
    ResultCode,
)


def get_safety_worker_tid(logs: LogContainer) -> str:
    return logs.find_log(
        field="message",
        pattern=r"Safety worker UniqueWorkerId\(\d+\) started",
    ).thread_id


# region ensure safety


class TestEnsureSafetyPositive(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.ensure_safety_enabled"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "safety_worker": {},
            }
        }

    def test_safety_enabled(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        assert logs_info_level.find_log(field="message", value="Running!")


class TestEnsureSafetyNegative(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.ensure_safety_enabled"

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
            }
        }

    def test_safety_enabled(
        self,
        results: ScenarioResult,
    ) -> None:
        assert results.return_code == ResultCode.SIGABRT

        assert results.stderr
        assert "Safety API is not enabled, please use runtime API or configure engine with safety" in results.stderr


class TestEnsureSafetyOutsideAsyncContext(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.ensure_safety_enabled_outside_async_context"

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
            }
        }

    def test_safety_enabled(
        self,
        logs: LogContainer,  # to capture kyron logs
        results: ScenarioResult,
    ) -> None:
        assert results.return_code == ResultCode.PANIC

        error_log = logs.find_log(field="level", value="ERROR")
        assert (
            "not_recoverable_error: For now we don't allow runtime API to be called outside of runtime"
            in error_log.message
        )


# endregion
# region safety worker core


@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestTaskHandling(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.task_handling"

    @pytest.fixture(
        scope="class",
        params=[1, 4],
        ids=lambda x: f"workers_{x}",
    )
    def workers(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, workers: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": workers,
                "safety_worker": {},
            }
        }

    def test_safety_worker_handling_failure(
        self,
        logs: LogContainer,  # to capture safety worker thread id
        logs_info_level: LogContainer,
    ) -> None:
        safety_worker_tid = get_safety_worker_tid(logs)

        failed_task_handler_log = logs_info_level.find_log(field="name", value="main_end")
        assert failed_task_handler_log.thread_id == safety_worker_tid, (
            "Failing task handler should be executed on safety worker thread"
        )

    def test_safety_worker_uniqueness(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        failed_task_handler_tid = logs_info_level.find_log(field="name", value="main_end").thread_id
        other_logs = logs_info_level.remove_logs(field="name", value="main_end")
        other_logs_tids = {log.thread_id for log in other_logs}

        assert failed_task_handler_tid not in other_logs_tids, (
            "Safety worker thread should be unique and not used for other tasks"
        )


@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestNestedTaskHandling(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.nested_task_handling"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "safety_worker": {},
            }
        }

    def test_safety_worker_handling_failure(
        self,
        logs: LogContainer,  # to capture safety worker thread id
        logs_info_level: LogContainer,
    ) -> None:
        safety_worker_tid = get_safety_worker_tid(logs)

        failed_task_handler_log = logs_info_level.find_log(field="name", value="outer_task_end")
        assert failed_task_handler_log.thread_id == safety_worker_tid, (
            "Failing task handler should be executed on safety worker thread"
        )

    def test_safety_worker_uniqueness(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        failed_task_handler_tid = logs_info_level.find_log(field="name", value="outer_task_end").thread_id
        other_logs = logs_info_level.remove_logs(field="name", value="outer_task_end")
        other_logs_tids = {log.thread_id for log in other_logs}

        assert failed_task_handler_tid not in other_logs_tids, (
            "Safety worker thread should be unique and not used for other tasks"
        )


class TestSafeApiWithoutSafetyWorker(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.safe_api_without_safety_worker"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
            }
        }

    def test_safety_worker_is_not_created(
        self,
        logs: LogContainer,  # to capture kyron logs
    ) -> None:
        assert (
            logs.find_log(
                field="message",
                pattern=r"Safety worker UniqueWorkerId\(\d+\) started",
            )
            is None
        ), "Safety worker should not be created"

    def test_tasks_executed_correctly(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        task_flow_result = [log.name for log in logs_info_level.get_logs(field="name")]
        expected_task_flow = ["main_begin", "failing_task", "main_end"]
        assert task_flow_result == expected_task_flow, "Tasks did not execute as expected"


class TestQueuedSafetyWorkerTasks(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.queue_safety_worker_tasks"

    @pytest.fixture(
        scope="class",
        params=[
            10,
            30,
            pytest.param(
                31,
                marks=pytest.mark.xfail(
                    reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/395",
                ),
            ),
            pytest.param(
                128,
                marks=pytest.mark.xfail(
                    reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/395",
                ),
            ),
        ],
        ids=lambda x: f"tasks_{x}",
    )
    def task_count(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, task_count: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "safety_worker": {},
            },
            "test": {
                "task_count": task_count,
            },
        }

    def test_safety_worker_execution(
        self,
        logs: LogContainer,  # to capture safety worker thread id
        logs_info_level: LogContainer,
    ) -> None:
        safety_worker_tid = get_safety_worker_tid(logs)

        safety_worker_tasks = logs_info_level.get_logs(field="thread_id", value=safety_worker_tid)
        assert len(safety_worker_tasks) == 1, "Expected one handling task on safety worker thread"
        assert hasattr(safety_worker_tasks[0], "safe_task_handler")
        assert safety_worker_tasks[0].is_error

    def test_regular_worker_execution(
        self,
        task_count: int,
        logs_info_level: LogContainer,
    ) -> None:
        main_begin_task = logs_info_level.find_log(field="name", value="main_begin")
        assert main_begin_task is not None

        failing_tasks = logs_info_level.get_logs(field="name", value="failing_task")
        assert len(failing_tasks) == task_count


@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/396")
class TestSafeWorkerHeavyLoad(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.heavy_load"

    @pytest.fixture(scope="class")
    def successful_task_count(self) -> int:
        return 600

    @pytest.fixture(scope="class")
    def fail_task_count(self) -> int:
        return 300

    @pytest.fixture(scope="class")
    def test_config(self, successful_task_count: int, fail_task_count: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 2048,
                "workers": 4,
                "safety_worker": {},
            },
            "test": {
                "successful_tasks": successful_task_count,
                "failing_tasks": fail_task_count,
            },
        }

    def test_safety_worker_execution(
        self,
        logs: LogContainer,  # to capture safety worker thread id
        logs_info_level: LogContainer,
        fail_task_count: int,
    ) -> None:
        safety_worker_tid = get_safety_worker_tid(logs)

        safety_worker_tasks = logs_info_level.get_logs(field="thread_id", value=safety_worker_tid)
        expected_safety_worker_tasks = logs_info_level.get_logs(field="safe_task_handler").get_logs(
            field="is_error", value=True
        )
        assert len(expected_safety_worker_tasks) == fail_task_count
        assert list(safety_worker_tasks) == list(expected_safety_worker_tasks)

    def test_regular_worker_execution(
        self,
        logs_info_level: LogContainer,
        successful_task_count: int,
        fail_task_count: int,
    ) -> None:
        assert logs_info_level.get_logs(field="name", value="main_begin") is not None
        assert logs_info_level.get_logs(field="name", value="main_end") is not None

        assert len(logs_info_level.get_logs(field="name", value="successful_task")) == successful_task_count
        assert len(logs_info_level.get_logs(field="name", value="nesting_successful_begin")) == successful_task_count
        assert len(logs_info_level.get_logs(field="name", value="nesting_successful_end")) == successful_task_count

        assert len(logs_info_level.get_logs(field="name", value="failing_task")) == fail_task_count
        assert len(logs_info_level.get_logs(field="name", value="nesting_failing_begin")) == fail_task_count


# endregion
# region thread parameters


@pytest.mark.root_required
@pytest.mark.skipif("WSL" in platform(), reason="Not supported on WSL")
class ThreadParametersBase(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.thread_parameters"

    def _resolve_target_path(self, path_to_resolve: Path) -> Path:
        """
        Provide resolved target path.

        Parameters
        ----------
        path_to_resolve : Path
            Path to resolve.
        """
        return path_to_resolve.resolve(strict=True)

    @pytest.fixture(scope="class")
    def results(
        self,
        command: list[str],
        execution_timeout: float,
        target_path: Path,
        *args,
        **kwargs,
    ) -> ScenarioResult:
        """
        Execute test scenario executable and return results.
        Extended with 'cap_sys_nice' setup.

        Parameters
        ----------
        command : list[str]
            Command to invoke.
        execution_timeout : float
            Test execution timeout in seconds.
        target_path : Path
            Path to test scenarios executable.
        """
        # Check and set 'cap_sys_nice'.
        resolved_target_path = self._resolve_target_path(target_path)
        caps = cap_utils.get_caps(resolved_target_path)
        if caps.get("cap_sys_nice", "") != "ep":
            cap_utils.set_caps(resolved_target_path, {"cap_sys_nice": "ep"})

        return self._run_command(command, execution_timeout, args, kwargs)


@pytest.mark.root_required
@pytest.mark.skipif("WSL" in platform(), reason="Not supported on WSL")
@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSafeWorkerThreadParameters(ThreadParametersBase):
    @pytest.fixture(scope="class")
    def scheduler(self) -> str:
        return "fifo"

    @pytest.fixture(scope="class")
    def priority(self) -> int:
        return 128

    @pytest.fixture(scope="class")
    def test_config(self, scheduler: str, priority: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "safety_worker": {
                    "thread_priority": priority,
                    "thread_scheduler": scheduler,
                },
            }
        }

    def test_safety_worker_thread_params(
        self,
        scheduler: str,
        priority: int,
        logs: LogContainer,  # to capture safety worker thread id
        logs_info_level: LogContainer,
    ) -> None:
        safety_worker_tid = get_safety_worker_tid(logs)

        safety_worker_log = logs_info_level.find_log(field="id", value="safety_worker")
        assert safety_worker_log.thread_id == safety_worker_tid
        assert safety_worker_log.scheduler == scheduler
        assert safety_worker_log.priority == priority


@pytest.mark.root_required
@pytest.mark.skipif("WSL" in platform(), reason="Not supported on WSL")
class ThreadParametersNegativeBase(ThreadParametersBase):
    def test_safety_worker_thread_params(
        self,
        logs: LogContainer,  # to capture safety worker thread id
        logs_info_level: LogContainer,
    ) -> None:
        safety_worker_tid = get_safety_worker_tid(logs)
        main_thread_log = logs_info_level.find_log(field="id", value="main")
        safety_worker_log = logs_info_level.find_log(field="id", value="safety_worker")
        assert safety_worker_log.thread_id == safety_worker_tid
        assert safety_worker_log.scheduler == main_thread_log.scheduler
        assert safety_worker_log.priority == main_thread_log.priority


@pytest.mark.root_required
@pytest.mark.skipif("WSL" in platform(), reason="Not supported on WSL")
@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSafeWorkerMissingThreadParameterScheduler(ThreadParametersNegativeBase):
    @pytest.fixture(scope="class")
    def priority(self) -> int:
        return 128

    @pytest.fixture(scope="class")
    def test_config(self, priority: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "safety_worker": {
                    "thread_priority": priority,
                },
            }
        }


@pytest.mark.root_required
@pytest.mark.skipif("WSL" in platform(), reason="Not supported on WSL")
@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSafeWorkerMissingThreadParameterPriority(ThreadParametersNegativeBase):
    @pytest.fixture(scope="class")
    def scheduler(self) -> str:
        return "fifo"

    @pytest.fixture(scope="class")
    def test_config(self, scheduler: str) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "safety_worker": {
                    "thread_scheduler": scheduler,
                },
            }
        }


@pytest.mark.root_required
@pytest.mark.skipif("WSL" in platform(), reason="Not supported on WSL")
@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSafeWorkerMissingThreadParameters(ThreadParametersNegativeBase):
    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "safety_worker": {},
            }
        }


# endregion
# region spawn methods


@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSpawnFromBoxed(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.spawn_from_boxed"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "safety_worker": {},
            }
        }

    def test_safety_worker_handler(
        self,
        logs: LogContainer,  # to capture safety worker thread id
        logs_info_level: LogContainer,
    ) -> None:
        safety_worker_tid = get_safety_worker_tid(logs)

        failed_task_handler_log = logs_info_level.find_log(field="name", value="main_end")
        assert failed_task_handler_log.thread_id == safety_worker_tid, (
            "Failing task handler should be executed on safety worker thread"
        )


@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSpawnFromReusable(TestSpawnFromBoxed):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.spawn_from_reusable"


@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSpawnOnDedicated(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.spawn_on_dedicated"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "dedicated_workers": [{"id": "my_dedicated_worker"}],
                "safety_worker": {},
            }
        }

    def test_task_to_worker_assignment(
        self,
        logs: LogContainer,  # to capture safety worker thread id
        logs_info_level: LogContainer,
    ) -> None:
        safety_worker_tid = get_safety_worker_tid(logs)

        regular_worker_log, dedicated_worker_log = logs_info_level.get_logs(field="name", value="failing_task")
        safety_worker_log = logs_info_level.find_log(field="name", value="main_end")
        assert safety_worker_log.thread_id == safety_worker_tid, (
            "Failing task handler should be executed on safety worker thread"
        )

        all_thread_ids = {item.thread_id for item in [regular_worker_log, dedicated_worker_log, safety_worker_log]}
        assert len(all_thread_ids) == 3, "Each task should be executed on its own thread"


@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSpawnFromBoxedOnDedicated(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.spawn_from_boxed_on_dedicated"


@pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/397")
class TestSpawnFromReusableOnDedicated(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.spawn_from_reusable_on_dedicated"


class TestSpawnOnDedicatedUnregisteredWorker(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.spawn_on_dedicated_unregistered_worker"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "safety_worker": {},
            }
        }

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult) -> None:
        # Panic inside async causes 'SIGABRT'.
        # TODO: determine this should be panic, and not an error.  # noqa: FIX002
        assert results.return_code == ResultCode.SIGABRT

        assert results.stderr
        assert "Tried to spawn on not registered dedicated worker UniqueWorkerId" in results.stderr


class TestSpawnFromBoxedOnDedicatedUnregisteredWorker(TestSpawnOnDedicatedUnregisteredWorker):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.spawn_from_boxed_on_dedicated_unregistered_worker"


class TestSpawnFromReusableOnDedicatedUnregisteredWorker(TestSpawnOnDedicatedUnregisteredWorker):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.safety_worker.spawn_from_reusable_on_dedicated_unregistered_worker"


# endregion
