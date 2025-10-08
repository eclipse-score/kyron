from typing import Any

import pytest
from testing_utils import LogContainer

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)


class TestRuntimeOneWorkerOneTask(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.basic"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 1, "task_queue_size": 256},
            "test": {"tasks": ["task_1"]},
        }

    def test_if_task_executed(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        task_name = test_config["test"]["tasks"][0]
        assert logs_info_level.contains_log("id", value=task_name), f"Task {task_name} was not executed"


class TestRuntimeTwoWorkersOneTask(TestRuntimeOneWorkerOneTask):
    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {"tasks": ["task_1"]},
        }


class TestRuntimeOneWorkerManyTasks(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.basic"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 1, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 101)]},
        }

    def test_if_all_tasks_executed(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        all_expected_set = set(test_config["test"]["tasks"])
        expected_len = len(all_expected_set)

        all_executed_set = set(result.id for result in logs_info_level)
        executed_len = len(all_executed_set)

        assert expected_len == executed_len, (
            f"Expected {expected_len} and executed {executed_len} task count should match."
        )
        assert all_expected_set == all_executed_set, "Some expected tasks did not execute."


class TestRuntimeTwoWorkersEvenTasks(TestRuntimeOneWorkerManyTasks):
    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 11)]},
        }


class TestRuntimeTwoWorkersOddTasks(TestRuntimeOneWorkerManyTasks):
    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 10)]},
        }


class TestRuntimeThreeWorkersEvenTasks(TestRuntimeOneWorkerManyTasks):
    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 3, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 11)]},
        }


class TestRuntimeThreeWorkersOddTasks(TestRuntimeOneWorkerManyTasks):
    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 3, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 10)]},
        }
