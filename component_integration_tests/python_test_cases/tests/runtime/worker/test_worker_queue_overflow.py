from typing import Any

import pytest
from testing_utils import ScenarioResult

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
    ResultCode,
)


class TestQueueOverflow(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.basic"

    def capture_stderr(self) -> bool:
        return True

    @pytest.fixture(scope="class", params=[(1, 10), (2, 100), (128, 1000)])
    def test_params(self, request: pytest.FixtureRequest) -> tuple[int, int]:
        # Tuple contains queue size and number of tasks.
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, test_params: tuple[int, int]) -> dict[str, Any]:
        queue_size, num_tasks = test_params
        return {
            "runtime": {"workers": 1, "task_queue_size": queue_size},
            "test": {"tasks": [f"task_{i}" for i in range(num_tasks)]},
        }

    def test_queue_overflow(self, results: ScenarioResult) -> None:
        # SIGKILL is raised intermittently.
        # Behavior to be clarified.
        assert results.return_code in (ResultCode.SIGABRT, ResultCode.SIGKILL)
        assert results.stderr
        assert "Cannot push to queue anymore, overflow!" in results.stderr
