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

from typing import Any

import pytest
from cit_scenario import CitScenario
from result_code import ResultCode
from testing_utils import LogContainer, ScenarioResult


class TestSpawn(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn"

    @pytest.fixture(scope="class")
    def task_count(self) -> int:
        return 100

    @pytest.fixture(scope="class")
    def test_config(self, task_count: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
            },
            "test": {
                "task_count": task_count,
            },
        }

    def test_task_execution(
        self,
        task_count: int,
        logs_info_level: LogContainer,
    ) -> None:
        executed_tasks = logs_info_level.get_logs(field="name", pattern=r"task_\d+")
        assert len(executed_tasks) == task_count, "All spawned tasks should be executed"

        task_names = [task.name for task in executed_tasks]
        for task_id in range(task_count):
            assert f"task_{task_id}" in task_names, f"Task 'task_{task_id}' should be executed"


class TestSpawnFromBoxed(TestSpawn):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_boxed"


class TestSpawnFromReusable(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_reusable"

    @pytest.fixture(scope="class")
    def task_count(self) -> int:
        return 20

    @pytest.fixture(scope="class", params=["for_value", "for_type"])
    def create_method(self, request: pytest.FixtureRequest) -> str:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, task_count: int, create_method: str) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
            },
            "test": {
                "task_count": task_count,
                "pool_size": 20,
                "create_method": create_method,
            },
        }

    def test_no_pool_overflow(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        error_message = logs_info_level.find_log(field="name", value="error")
        assert error_message is None, "No error is expected during run"

    def test_task_execution(
        self,
        task_count: int,
        logs_info_level: LogContainer,
    ) -> None:
        executed_tasks = logs_info_level.get_logs(field="name", pattern=r"task_\d+")
        assert len(executed_tasks) == task_count, "All spawned tasks should be executed"

        task_names = [task.name for task in executed_tasks]
        for task_id in range(task_count):
            assert f"task_{task_id}" in task_names, f"Task 'task_{task_id}' should be executed"


class TestSpawnFromReusableWithOverflow(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_reusable"

    @pytest.fixture(scope="class")
    def pool_size(self) -> int:
        return 5

    @pytest.fixture(scope="class", params=["for_value", "for_type"])
    def create_method(self, request: pytest.FixtureRequest) -> str:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, pool_size: int, create_method: str) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
            },
            "test": {
                "task_count": 6,
                "pool_size": pool_size,
                "create_method": create_method,
            },
        }

    def test_pool_overflow(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        error_message = logs_info_level.find_log(field="name", value="error")
        assert error_message is not None
        assert error_message.value == "NoData", "Expected error 'NoData'"

    def test_task_execution(
        self,
        pool_size: int,
        logs_info_level: LogContainer,
    ) -> None:
        executed_tasks = logs_info_level.get_logs(field="name", pattern=r"task_\d+")
        assert len(executed_tasks) == pool_size, "All spawned tasks should be executed"

        task_names = [task.name for task in executed_tasks]
        for task_id in range(pool_size):
            assert f"task_{task_id}" in task_names, f"Task 'task_{task_id}' should be executed"


class TestSpawnFromReusableWithReuse(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_reusable_reuse"

    @pytest.fixture(scope="class", params=[1, 15, 20])
    def task_count(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def iterations(self) -> int:
        return 2

    @pytest.fixture(scope="class")
    def test_config(self, task_count: int, iterations: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
            },
            "test": {
                "task_count": task_count,
                "pool_size": 20,
                "iterations": iterations,
            },
        }

    def test_no_pool_overflow(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        error_message = logs_info_level.find_log(field="name", value="error")
        assert error_message is None, "No error is expected during task reuse"

    def test_task_execution(
        self,
        task_count: int,
        iterations: int,
        logs_info_level: LogContainer,
    ) -> None:
        executed_tasks = logs_info_level.get_logs(field="name", pattern=r"task_\d+")
        assert len(executed_tasks) == task_count * iterations, "All spawned tasks should be executed"

        task_name_to_count = {}
        for task in executed_tasks:
            task_name_to_count[task.name] = task_name_to_count.get(task.name, 0) + 1

        for task_id in range(task_count):
            assert f"task_{task_id}" in task_name_to_count, f"Task 'task_{task_id}' should be executed"
            assert task_name_to_count[f"task_{task_id}"] == iterations, (
                f"Task 'task_{task_id}' should be executed {iterations} times"
            )


class TestSpawnOnDedicated(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_on_dedicated"

    @pytest.fixture(scope="class")
    def task_count(self) -> int:
        return 100

    @pytest.fixture(scope="class")
    def test_config(self, task_count: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": [{"id": "my_dedicated_worker"}],
            },
            "test": {
                "task_count": task_count,
            },
        }

    def test_task_to_worker_assignment(
        self,
        task_count: int,
        logs_info_level: LogContainer,
    ) -> None:
        regular_worker_task = logs_info_level.find_log(field="name", value="regular_task")
        dedicated_worker_tasks = logs_info_level.get_logs(field="name", pattern=r"task_\d+")
        assert len(dedicated_worker_tasks) == task_count, "All dedicated tasks should be executed"

        dedicated_worker_threads = {task.thread_id for task in dedicated_worker_tasks}
        assert len(dedicated_worker_threads) == 1, (
            "All dedicated tasks should be executed on the same dedicated worker thread"
        )
        assert regular_worker_task.thread_id not in dedicated_worker_threads, (
            "Regular task should be executed on a different thread than dedicated tasks"
        )


class TestSpawnFromBoxedOnDedicated(TestSpawnOnDedicated):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_boxed_on_dedicated"


class TestSpawnFromBoxedOnDedicatedInvalidWorker(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_boxed_on_dedicated_invalid_worker"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
            }
        }

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid_usage(self, results: ScenarioResult) -> None:
        # Panic inside async causes 'SIGABRT'.
        # TODO: determine this should be panic, and not an error.  # noqa: FIX002
        assert results.return_code == ResultCode.SIGABRT

        assert results.stderr
        assert "Tried to spawn on not registered dedicated worker UniqueWorkerId" in results.stderr


class TestSpawnFromReusableOnDedicated(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_reusable_on_dedicated"

    @pytest.fixture(scope="class")
    def task_count(self) -> int:
        return 20

    @pytest.fixture(scope="class", params=["for_value", "for_type"])
    def create_method(self, request: pytest.FixtureRequest) -> str:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, task_count: int, create_method: str) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": [{"id": "my_dedicated_worker"}],
            },
            "test": {
                "task_count": task_count,
                "pool_size": 20,
                "create_method": create_method,
            },
        }

    def test_no_pool_overflow(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        error_message = logs_info_level.find_log(field="name", value="error")
        assert error_message is None, "No error is expected during run"

    def test_task_to_worker_assignment(
        self,
        task_count: int,
        logs_info_level: LogContainer,
    ) -> None:
        dedicated_worker_tasks = logs_info_level.get_logs(field="name", pattern=r"task_\d+")
        assert len(dedicated_worker_tasks) == task_count, "All dedicated tasks should be executed"

        dedicated_worker_threads = {task.thread_id for task in dedicated_worker_tasks}
        assert len(dedicated_worker_threads) == 1, (
            "All dedicated tasks should be executed on the same dedicated worker thread"
        )
        regular_worker_task = logs_info_level.find_log(field="name", value="regular_task")
        assert regular_worker_task.thread_id not in dedicated_worker_threads, (
            "Regular task should be executed on a different thread than dedicated tasks"
        )


class TestSpawnFromReusableOnDedicatedWithOverflow(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_reusable_on_dedicated"

    @pytest.fixture(scope="class")
    def pool_size(self) -> int:
        return 5

    @pytest.fixture(scope="class", params=["for_value", "for_type"])
    def create_method(self, request: pytest.FixtureRequest) -> str:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, pool_size: int, create_method: str) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": [{"id": "my_dedicated_worker"}],
            },
            "test": {
                "task_count": 6,
                "pool_size": pool_size,
                "create_method": create_method,
            },
        }

    def test_pool_overflow(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        error_message = logs_info_level.find_log(field="name", value="error")
        assert error_message is not None
        assert error_message.value == "NoData", "Expected error 'NoData'"

    def test_task_execution(
        self,
        pool_size: int,
        logs_info_level: LogContainer,
    ) -> None:
        executed_dedicated_tasks = logs_info_level.get_logs(field="name", pattern=r"task_\d+")
        assert len(executed_dedicated_tasks) == pool_size, "All spawned tasks should be executed"

        task_names = [task.name for task in executed_dedicated_tasks]
        for task_id in range(pool_size):
            assert f"task_{task_id}" in task_names, f"Task 'task_{task_id}' should be executed"

        regular_worker_task = logs_info_level.find_log(field="name", value="regular_task")
        assert regular_worker_task is not None, "Regular task should be executed"


class TestSpawnFromReusableOnDedicatedWithReuse(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_reusable_on_dedicated_reuse"

    @pytest.fixture(scope="class", params=[1, 15, 20])
    def task_count(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def iterations(self) -> int:
        return 2

    @pytest.fixture(scope="class")
    def test_config(self, task_count: int, iterations: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": [{"id": "my_dedicated_worker"}],
            },
            "test": {
                "task_count": task_count,
                "pool_size": 20,
                "iterations": iterations,
            },
        }

    def test_no_pool_overflow(
        self,
        logs_info_level: LogContainer,
    ) -> None:
        error_message = logs_info_level.find_log(field="name", value="error")
        assert error_message is None, "No error is expected during task reuse"

    def test_regular_task_execution(
        self,
        iterations: int,
        logs_info_level: LogContainer,
    ) -> None:
        regular_worker_task = logs_info_level.get_logs(field="name", value="regular_task")
        assert len(regular_worker_task) == iterations, f"Regular task should be executed {iterations} times"

    def test_dedicated_task_execution(
        self,
        task_count: int,
        iterations: int,
        logs_info_level: LogContainer,
    ) -> None:
        executed_dedicated_tasks = logs_info_level.get_logs(field="name", pattern=r"task_\d+")
        assert len(executed_dedicated_tasks) == task_count * iterations, "All spawned tasks should be executed"

        task_name_to_count = {}
        for task in executed_dedicated_tasks:
            task_name_to_count[task.name] = task_name_to_count.get(task.name, 0) + 1

        for task_id in range(task_count):
            assert f"task_{task_id}" in task_name_to_count, f"Task 'task_{task_id}' should be executed"
            assert task_name_to_count[f"task_{task_id}"] == iterations, (
                f"Task 'task_{task_id}' should be executed {iterations} times"
            )


class TestSpawnFromReusableOnDedicatedInvalidWorker(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.spawn_methods.spawn_from_reusable_on_dedicated_invalid_worker"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": [{"id": "my_dedicated_worker"}],
            },
            "test": {
                "task_count": 0,  # Not used
                "pool_size": 20,
                "create_method": "",  # Not used
            },
        }

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid_usage(self, results: ScenarioResult) -> None:
        # Panic inside async causes 'SIGABRT'.
        # TODO: determine this should be panic, and not an error.  # noqa: FIX002
        assert results.return_code == ResultCode.SIGABRT

        assert results.stderr
        assert "Tried to spawn on not registered dedicated worker UniqueWorkerId" in results.stderr
