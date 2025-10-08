from typing import Any

import pytest
from testing_utils import LogContainer

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)


class TestThreeBlockedWorkersWithOneUnblocked(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.with_blocking_tasks"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "blocking_tasks": [
                    "blocking_task_A",
                    "blocking_task_B",
                    "blocking_task_C",
                ],
                "non_blocking_tasks": [f"non_blocking_task_{ndx}" for ndx in range(1, 11)],
            },
        }

    def test_if_blocking_tasks_started_first(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        expected_tasks = test_config["test"]["blocking_tasks"]
        expected_task_count = len(expected_tasks)

        assert all(result.location == "begin" for result in logs_info_level[:expected_task_count]), (
            f"All blocking tasks should start in first {expected_task_count} results."
        )

        task_names_set = {result.id for result in logs_info_level[:expected_task_count]}
        assert set(expected_tasks) == task_names_set, (
            f"All blocking tasks should run in first {expected_task_count} results."
        )

    def test_if_blocking_tasks_ended_last(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        expected_tasks = test_config["test"]["blocking_tasks"]
        expected_task_count = len(expected_tasks)

        assert all(result.location == "end" for result in logs_info_level[-expected_task_count:]), (
            f"All blocking tasks should end in last {expected_task_count} results."
        )

        task_names_set = {result.id for result in logs_info_level[-expected_task_count:]}
        assert set(expected_tasks) == task_names_set, (
            f"All blocking tasks should end in last {expected_task_count} results."
        )

    def test_if_non_blocking_tasks_executed_in_the_middle(
        self, test_config: dict[str, Any], logs_info_level: LogContainer
    ):
        blocking_task_count = len(test_config["test"]["blocking_tasks"])
        expected_tasks = test_config["test"]["non_blocking_tasks"]

        non_blocking_task_set = {result.id for result in logs_info_level[blocking_task_count:-blocking_task_count]}
        assert set(expected_tasks) == non_blocking_task_set, "Non-blocking tasks should run in the middle."

    def test_if_non_blocking_tasks_executed_on_the_same_thread(
        self, test_config: dict[str, Any], logs_info_level: LogContainer
    ):
        expected_tasks = test_config["test"]["non_blocking_tasks"]

        threads_set = {result.thread_id for result in logs_info_level if result.id in expected_tasks}
        assert len(threads_set) == 1, "All non-blocking tasks should run on the same thread."
