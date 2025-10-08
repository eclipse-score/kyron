import math
from typing import Any

import pytest
from testing_utils import LogContainer

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)


# Due to OS related condition variable wait behavior including scheduling, thread priority,
# hardware, load and other factors, sleep can spike and wait longer than expected.
# There is a bug filled for this topic: https://github.com/qorix-group/inc_orchestrator_internal/issues/142
def get_threshold_ms(expected_sleep_ms: int) -> int:
    """
    Calculate the threshold for sleep duration checks.
    """
    if expected_sleep_ms > 500:
        return math.ceil(expected_sleep_ms * 0.5)
    elif expected_sleep_ms > 100:
        return math.ceil(expected_sleep_ms * 1.5)
    else:
        return math.ceil(expected_sleep_ms * 5)


class TestSingleSleep(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.sleep.basic"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 1, "task_queue_size": 256},
            "test": {
                "non_blocking_sleep_tasks": [{"id": "non_blocking_sleep_1", "delay_ms": 2000}],
                "blocking_sleep_tasks": [],
                "non_sleep_tasks": [],
            },
        }

    def test_completeness(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        task_id = test_config["test"]["non_blocking_sleep_tasks"][0]["id"]

        assert logs_info_level[0].id == task_id and logs_info_level[0].location == "begin"
        assert logs_info_level[1].id == task_id and logs_info_level[1].location == "end"

    def test_sleep_duration(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        expected_sleep_ms = test_config["test"]["non_blocking_sleep_tasks"][0]["delay_ms"]
        threshold_ms = get_threshold_ms(expected_sleep_ms)

        sleep_duration_ms = (logs_info_level[1].timestamp - logs_info_level[0].timestamp).total_seconds() * 1000

        assert expected_sleep_ms <= sleep_duration_ms <= expected_sleep_ms + threshold_ms, (
            f"Expected sleep duration {expected_sleep_ms} ms, "
            f"but got {sleep_duration_ms} ms. Threshold: {threshold_ms} ms."
        )


class TestZeroSleep(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.sleep.basic"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 1, "task_queue_size": 256},
            "test": {
                "non_blocking_sleep_tasks": [{"id": "non_blocking_sleep_1", "delay_ms": 0}],
                "blocking_sleep_tasks": [],
                "non_sleep_tasks": [],
            },
        }

    def test_sleep_duration(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        task_id = test_config["test"]["non_blocking_sleep_tasks"][0]["id"]

        entries = logs_info_level.get_logs(field="id", value=task_id)
        assert len(entries) == 2, "Expected two entries for the task."

        begin_entry = entries[0]
        end_entry = entries[1]

        max_sleep_ms = 20
        sleep_duration_ms = (end_entry.timestamp - begin_entry.timestamp).total_seconds() * 1000

        assert sleep_duration_ms <= max_sleep_ms, (
            f"Expected sleep duration to be less than or equal to {max_sleep_ms} ms, "
        )


class TestMultipleSleepsWithRegularTasks(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.sleep.basic"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 1, "task_queue_size": 256},
            "test": {
                "non_blocking_sleep_tasks": [
                    {"id": "non_blocking_sleep_1", "delay_ms": 3000},
                    {"id": "non_blocking_sleep_2", "delay_ms": 2500},
                    {"id": "non_blocking_sleep_3", "delay_ms": 2000},
                    {"id": "non_blocking_sleep_4", "delay_ms": 1500},
                    {"id": "non_blocking_sleep_5", "delay_ms": 1000},
                    {"id": "non_blocking_sleep_6", "delay_ms": 500},
                    {"id": "non_blocking_sleep_7", "delay_ms": 100},
                ],
                "blocking_sleep_tasks": [],
                "non_sleep_tasks": [f"non_sleep_{x}" for x in range(1, 11)],
            },
        }

    def test_task_completeness(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        non_blocking_sleep_tasks = logs_info_level.get_logs(field="id", pattern="non_blocking_sleep*")

        begin_tasks = non_blocking_sleep_tasks.get_logs(field="location", value="begin")
        assert len(begin_tasks) == len(test_config["test"]["non_blocking_sleep_tasks"]), (
            "Not all non-blocking sleep tasks started."
        )

        end_tasks = non_blocking_sleep_tasks.get_logs(field="location", value="end")
        assert len(end_tasks) == len(test_config["test"]["non_blocking_sleep_tasks"]), (
            "Not all non-blocking sleep tasks finished."
        )

        non_sleep_tasks = logs_info_level.get_logs(field="id", pattern="non_sleep*")
        assert len(non_sleep_tasks) == len(test_config["test"]["non_sleep_tasks"]), "Not all non-sleep tasks executed."

    def test_sleep_duration(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        for task in test_config["test"]["non_blocking_sleep_tasks"]:
            entries = logs_info_level.get_logs(field="id", value=task["id"])
            assert len(entries) == 2, "Expected two entries for the task."

            begin_entry = entries[0]
            end_entry = entries[1]

            expected_sleep_ms = task["delay_ms"]
            threshold_ms = get_threshold_ms(expected_sleep_ms)

            sleep_duration_ms = (end_entry.timestamp - begin_entry.timestamp).total_seconds() * 1000

            assert expected_sleep_ms <= sleep_duration_ms <= expected_sleep_ms + threshold_ms, (
                f"Expected sleep duration {expected_sleep_ms} ms, "
                f"but got {sleep_duration_ms} ms. Threshold: {threshold_ms} ms."
            )


class TestMultipleSleepsWithRegularTasksForManyWorkers(TestMultipleSleepsWithRegularTasks):
    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "non_blocking_sleep_tasks": [
                    {"id": "non_blocking_sleep_1", "delay_ms": 3000},
                    {"id": "non_blocking_sleep_2", "delay_ms": 2500},
                    {"id": "non_blocking_sleep_3", "delay_ms": 2000},
                    {"id": "non_blocking_sleep_4", "delay_ms": 1500},
                    {"id": "non_blocking_sleep_5", "delay_ms": 1000},
                    {"id": "non_blocking_sleep_6", "delay_ms": 500},
                    {"id": "non_blocking_sleep_7", "delay_ms": 100},
                ],
                "blocking_sleep_tasks": [],
                "non_sleep_tasks": [f"non_sleep_{x}" for x in range(1, 11)],
            },
        }


class TestMultipleSleepsWithBlockedWorkers(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.sleep.basic"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "non_blocking_sleep_tasks": [
                    {"id": "non_blocking_sleep_1", "delay_ms": 3000},
                    {"id": "non_blocking_sleep_2", "delay_ms": 2500},
                    {"id": "non_blocking_sleep_3", "delay_ms": 2000},
                    {"id": "non_blocking_sleep_4", "delay_ms": 1500},
                    {"id": "non_blocking_sleep_5", "delay_ms": 1000},
                    {"id": "non_blocking_sleep_6", "delay_ms": 500},
                    {"id": "non_blocking_sleep_7", "delay_ms": 500},
                ],
                "blocking_sleep_tasks": [
                    {"id": "blocking_sleep_1", "delay_ms": 3300},
                    {"id": "blocking_sleep_2", "delay_ms": 3300},
                    {"id": "blocking_sleep_3", "delay_ms": 3300},
                ],
                "non_sleep_tasks": [f"non_sleep_{x}" for x in range(1, 11)],
            },
        }

    def test_task_completeness(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        non_blocking_sleep_tasks = logs_info_level.get_logs(field="id", pattern="non_blocking_sleep*")
        assert len(non_blocking_sleep_tasks) == len(
            test_config["test"]["non_blocking_sleep_tasks"] * 2  # For begin and end
        ), "Not all non-blocking sleep tasks executed."

        blocking_sleep_tasks = logs_info_level.get_logs(field="id", pattern="^blocking_sleep*")
        assert len(blocking_sleep_tasks) == len(
            test_config["test"]["blocking_sleep_tasks"] * 2  # For begin and end
        ), "Not all blocking sleep tasks executed."

        non_sleep_tasks = logs_info_level.get_logs(field="id", pattern="non_sleep*")
        assert len(non_sleep_tasks) == len(test_config["test"]["non_sleep_tasks"]), "Not all non-sleep tasks executed."

    def test_sleep_duration(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        for task in test_config["test"]["non_blocking_sleep_tasks"]:
            entries = logs_info_level.get_logs(field="id", value=task["id"])
            assert len(entries) == 2, "Expected two entries for the task."

            begin_entry = entries[0]
            end_entry = entries[1]

            expected_sleep_ms = task["delay_ms"]
            threshold_ms = get_threshold_ms(expected_sleep_ms)

            sleep_duration_ms = (end_entry.timestamp - begin_entry.timestamp).total_seconds() * 1000

            assert expected_sleep_ms <= sleep_duration_ms <= expected_sleep_ms + threshold_ms, (
                f"Expected sleep duration {expected_sleep_ms} ms, "
                f"but got {sleep_duration_ms} ms. Threshold: {threshold_ms} ms. Task: {task['id']}"
            )

    def test_blocking_sleeps_finished_last(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        blocking_task_count = len(test_config["test"]["blocking_sleep_tasks"])
        for task in logs_info_level[-blocking_task_count:]:
            assert task.id.startswith("blocking_sleep") and task.location == "end", (
                f"Blocking sleep task {task.id} did not finish as one of last {blocking_task_count}."
            )
