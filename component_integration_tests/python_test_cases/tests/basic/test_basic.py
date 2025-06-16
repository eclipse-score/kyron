import pytest
from qtesting_tools.log_container import LogContainer


class TestOnlyShutdown1W2Q:
    @pytest.fixture(scope="class")
    def test_case_name(self):
        return "basic.only_shutdown"

    @pytest.fixture(scope="class")
    def test_config(self):
        return {"runtime": {"task_queue_size": 2, "workers": 1}}

    def test_engine_start_executed(self, test_results: LogContainer):
        assert test_results.contains_log(
            field="message", pattern="Program entered engine"
        ), "Program did not start as expected, no AsyncRuntime created"

    def test_shutdown_action_executed(self, test_results: LogContainer):
        assert test_results.contains_log(
            field="message", pattern="Program execution finished"
        ), "Program did not start as expected, no AsyncRuntime created"

    def test_no_actions_executed(self, test_results: LogContainer):
        expected_messages = [
            "Program entered engine",
            "Program execution finished",
        ]
        assert len(test_results) == len(expected_messages), (
            "Test case executed actions that were not expected."
        )


class TestOnlyShutdown2W2Q(TestOnlyShutdown1W2Q):
    @pytest.fixture(scope="class")
    def test_config(self):
        return {"runtime": {"task_queue_size": 2, "workers": 2}}
