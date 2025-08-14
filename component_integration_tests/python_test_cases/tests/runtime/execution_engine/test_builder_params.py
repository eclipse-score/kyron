from abc import abstractmethod
from typing import Any

import psutil
import pytest
from testing_utils import ScenarioResult

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
    ResultCode,
)

# region task_queue_size


class TestTaskQueueSize(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

    @abstractmethod
    def queue_size(self, request: pytest.FixtureRequest) -> int:
        pass

    @pytest.fixture(scope="class")
    def test_config(self, queue_size: int) -> dict[str, Any]:
        return {"runtime": {"task_queue_size": queue_size, "workers": 1}}


class TestTaskQueueSize_Valid(TestTaskQueueSize):
    @pytest.fixture(scope="class", params=range(0, 32))
    def queue_size(self, request: pytest.FixtureRequest) -> int:
        exponent = request.param
        queue_size = 2**exponent

        # Prevent allocation of a queue too large for available memory.
        # Required for testing on smaller devices (e.g., CI runners).
        size_heuristic = queue_size * 8
        total_memory = psutil.virtual_memory().total + psutil.swap_memory().total
        if size_heuristic >= total_memory:
            pytest.xfail(
                reason=f"Requested queue size ({queue_size}) is too large for available memory ({total_memory})"
            )

        return queue_size

    def test_valid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.SUCCESS


class TestTaskQueueSize_Invalid(TestTaskQueueSize):
    @pytest.fixture(scope="class", params=[0, 10, 321, 1234, 2**16 - 1, 2**32 - 1])
    def queue_size(self, request: pytest.FixtureRequest) -> int:
        return request.param

    def capture_stderr(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult, queue_size: int) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert f"Task queue size ({queue_size}) must be power of two" in results.stderr


# endregion


# region workers


class TestWorkers(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

    @abstractmethod
    def workers(self, request: pytest.FixtureRequest) -> int:
        pass

    @pytest.fixture(scope="class")
    def test_config(self, workers: int) -> dict[str, Any]:
        return {"runtime": {"task_queue_size": 256, "workers": workers}}


class TestWorkers_Valid(TestWorkers):
    @pytest.fixture(scope="class", params=[1, 33, 100, 128])
    def workers(self, request: pytest.FixtureRequest) -> int:
        return request.param

    def test_valid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.SUCCESS


class TestWorkers_Invalid(TestWorkers):
    @pytest.fixture(scope="class", params=[0, 129, 1000])
    def workers(self, request: pytest.FixtureRequest) -> int:
        return request.param

    def capture_stderr(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult, workers: int) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert (
            f"Cannot create engine with {workers} workers. Min is 1 and max is 128"
            in results.stderr
        )


# endregion

# region thread_priority


class TestThreadPriority(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

    @pytest.fixture(scope="class", params=[0, 120, 255])
    def test_config(self, request: pytest.FixtureRequest) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "thread_priority": request.param,
            }
        }

    def test_valid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.SUCCESS


# endregion

# region thread_affinity


class TestThreadAffinity(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

    @pytest.fixture(scope="class", params=[0, 2**16, 2**32, 2**48, 2**63])
    def test_config(self, request: pytest.FixtureRequest) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "thread_affinity": request.param,
            }
        }

    def test_valid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.SUCCESS


# endregion

# region thread_stack_size


class TestThreadStackSize(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

    @abstractmethod
    def thread_stack_size(self, request: pytest.FixtureRequest) -> int:
        pass

    @pytest.fixture(scope="class")
    def test_config(self, thread_stack_size: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "thread_stack_size": thread_stack_size,
            }
        }


class TestThreadStackSize_Valid(TestThreadStackSize):
    @pytest.fixture(scope="class", params=[1024 * 128, 1024 * 1024])
    def thread_stack_size(self, request: pytest.FixtureRequest) -> int:
        return request.param

    def test_valid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.SUCCESS


class TestThreadStackSize_TooSmall(TestThreadStackSize):
    # Tested stack size values are lower than platform-specific limit:
    # 'iceoryx2_bb_posix::system_configuration::Limit::MinStackSizeOfThread'.
    #
    # NOTE: it is possible to set stack size over the limit, but too small for requested work.
    # This will cause SIGSEGV due to stack overflow. This is not a bug.

    @pytest.fixture(scope="class", params=[0, 8192])
    def thread_stack_size(self, request: pytest.FixtureRequest) -> int:
        return request.param

    def capture_stderr(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert (
            "called `Result::unwrap()` on an `Err` value: StackSizeTooSmall"
            in results.stderr
        )


# endregion
