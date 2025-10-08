from functools import cached_property
from typing import Any

import pytest
from testing_utils import LogContainer

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)


class TestSingleRuntimeMultipleExecEngine(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.execution_engine.single_rt_multiple_exec_engine"

    @cached_property
    def _tasks(self) -> list[str]:
        return [f"task_{i}" for i in range(1, 10 + 1)]

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": [
                {
                    "workers": 4,
                    "task_queue_size": 256,
                },
                {
                    "workers": 4,
                    "task_queue_size": 256,
                },
            ],
            "tasks": [
                {"engine_id": 0, "task_ids": self._tasks[:3]},
                {"engine_id": 1, "task_ids": self._tasks[3:]},
            ],
        }

    def test_all_tasks_executed(self, logs_info_level: LogContainer) -> None:
        task_logs = logs_info_level.get_logs("task_id", pattern="task_*")
        act_tasks = {entry.task_id for entry in task_logs}
        exp_tasks = set(self._tasks)

        tasks_diff = act_tasks.symmetric_difference(exp_tasks)
        assert not tasks_diff, f"Mismatch between tasks assigned ({exp_tasks}) and executed ({act_tasks})"

    def test_tasks_assigned_to_exec_engine(self, test_config: dict[str, Any], logs_info_level: LogContainer) -> None:
        for tasks_data in test_config["tasks"]:
            engine_id = tasks_data["engine_id"]
            task_ids = tasks_data["task_ids"]

            engine_logs = logs_info_level.get_logs("engine_id", value=engine_id)
            task_logs = engine_logs.get_logs("task_id", pattern="task_*")
            act_tasks = {entry.task_id for entry in task_logs}
            exp_tasks = set(task_ids)

            tasks_diff = act_tasks.symmetric_difference(exp_tasks)
            assert not tasks_diff, (
                f"Mismatch between tasks assigned ({exp_tasks}) and executed ({act_tasks}) for execution engine {engine_id}"
            )
