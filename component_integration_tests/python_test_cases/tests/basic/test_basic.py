from typing import Any

import pytest
from testing_utils import LogContainer

from component_integration_tests.python_test_cases.tests.cit_scenario import CitScenario


class TestOnlyShutdown1W2Q(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {"runtime": {"task_queue_size": 2, "workers": 1}}

    def test_engine_start_executed(self, logs_info_level: LogContainer):
        assert logs_info_level.contains_log(
            field="message", value="Program entered engine"
        ), "Program did not start as expected, no AsyncRuntime created"

    def test_shutdown_action_executed(self, logs_info_level: LogContainer):
        assert logs_info_level.contains_log(
            field="message", value="Program execution finished"
        ), "Program did not start as expected, no AsyncRuntime created"

    def test_no_actions_executed(self, logs_info_level: LogContainer):
        expected_messages = [
            "Program entered engine",
            "Program execution finished",
        ]
        assert len(logs_info_level) == len(expected_messages), (
            "Test case executed actions that were not expected."
        )


class TestOnlyShutdown2W2Q(TestOnlyShutdown1W2Q):
    @pytest.fixture(scope="class")
    def test_config(self):
        return {"runtime": {"task_queue_size": 2, "workers": 2}}
