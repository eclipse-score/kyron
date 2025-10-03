import json
from pathlib import Path
from typing import Any

import psutil
import pytest
from testing_utils import LogContainer, ScenarioResult

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)
from component_integration_tests.python_test_cases.tests.result_code import (
    ResultCode,
)
import component_integration_tests.python_test_cases.tests.cap_utils as cap_utils

# region task_queue_size


class TestTaskQueueSize(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

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

    def expect_command_failure(self) -> bool:
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
        return "runtime.worker.num_workers"

    @pytest.fixture(scope="class")
    def test_config(self, workers: int) -> dict[str, Any]:
        return {"runtime": {"task_queue_size": 256, "workers": workers}}


class TestWorkers_Valid(TestWorkers):
    @pytest.fixture(scope="class", params=[1, 4, 12, 60, 128])
    def workers(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def execution_timeout(self) -> float:
        # Tests with many workers take longer to execute.
        return 15.0

    def test_valid(
        self, results: ScenarioResult, logs_info_level: LogContainer, workers: int
    ) -> None:
        assert results.return_code == ResultCode.SUCCESS

        # Check barrier resulted in timeout.
        wait_result_logs = logs_info_level.get_logs("wait_result", pattern=".*")
        assert len(wait_result_logs) == 1
        assert wait_result_logs[0].wait_result == "timeout"

        # Check number of workers.
        # Exact 'id' content is not checked.
        # Test relies on spawning one too many tasks for available workers.
        # Tracing is no longer active for this extra task, but it's not determinate which one is it.
        worker_logs = logs_info_level.get_logs("id", pattern="worker_.*")
        worker_ids = [log.id for log in worker_logs]
        assert len(worker_ids) == workers


class TestWorkers_Invalid(TestWorkers):
    @pytest.fixture(scope="class", params=[0, 129, 1000])
    def workers(self, request: pytest.FixtureRequest) -> int:
        return request.param

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
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


@pytest.mark.root_required
class TestThreadPriority(CitScenario):
    SCHEDULER = "fifo"
    NUM_WORKERS = 4

    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.thread_priority"

    @pytest.fixture(scope="class", params=[0, 128, 255])
    def priority(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, priority: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": self.NUM_WORKERS,
                "thread_priority": priority,
                "thread_scheduler": self.SCHEDULER,
            }
        }

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
        if not caps.get("cap_sys_nice", "") == "ep":
            cap_utils.set_caps(resolved_target_path, {"cap_sys_nice": "ep"})

        return self._run_command(command, execution_timeout, args, kwargs)

    def test_valid(
        self,
        results: ScenarioResult,
        logs_info_level: LogContainer,
        priority: int,
    ) -> None:
        assert results.return_code == ResultCode.SUCCESS

        # Find logs with worker IDs.
        worker_logs = logs_info_level.get_logs(field="id", pattern="worker_.*")
        assert len(worker_logs) == self.NUM_WORKERS

        # Check priority of each worker.
        for worker_log in worker_logs:
            act_priority = worker_log.priority

            # Check priority as expected and in expected bounds.
            assert priority == act_priority, (
                f"Invalid priority, expected: {priority}, found: {act_priority}"
            )


# endregion

# region thread_affinity


class TestThreadAffinity(CitScenario):
    NUM_WORKERS = 4

    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.thread_affinity"

    @pytest.fixture(scope="class")
    def num_cores(self) -> int:
        num_cores = psutil.cpu_count()
        if num_cores is None or num_cores == 0:
            raise RuntimeError("Undetermined number of cores")
        return num_cores

    @pytest.fixture(scope="class")
    def test_config(self, affinity: list[int]) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": self.NUM_WORKERS,
                "thread_affinity": affinity,
            }
        }


class TestThreadAffinity_Valid(TestThreadAffinity):
    TEST_MODES = ["first", "mid", "last", "multiple", "all"]

    @pytest.fixture(scope="class", params=TEST_MODES, ids=TEST_MODES)
    def affinity(self, request: pytest.FixtureRequest, num_cores: int) -> list[int]:
        # Available affinity tests are dependent on number of cores available.
        mode = request.param

        def check_num_cores(num_required: int):
            if num_cores < num_required:
                pytest.skip(
                    reason="Test requires more CPU cores, "
                    f"required: {num_required}, "
                    f"available: {num_cores}"
                )

        match mode:
            # First available core.
            case "first":
                return [0]

            # Middle available core.
            case "mid":
                check_num_cores(3)
                return [num_cores // 2]

            # Last available core.
            case "last":
                check_num_cores(2)
                return [num_cores - 1]

            # Three cores - first, middle and last.
            case "multiple":
                check_num_cores(4)
                return [0, num_cores // 2, num_cores - 1]

            # All available cores.
            case "all":
                check_num_cores(2)
                return list(range(num_cores))

            case _:
                raise RuntimeError(f"Invalid test mode: {mode}")

    def test_valid(
        self,
        results: ScenarioResult,
        logs_info_level: LogContainer,
        affinity: list[int],
    ) -> None:
        assert results.return_code == ResultCode.SUCCESS

        # Find logs with worker IDs.
        worker_logs = logs_info_level.get_logs(field="id", pattern="worker_.*")
        assert len(worker_logs) == self.NUM_WORKERS

        # Check affinity of each worker.
        for worker_log in worker_logs:
            # Convert affinity string to list.
            act_affinity = json.loads(worker_log.affinity)

            # Check affinity as expected.
            assert affinity == act_affinity, (
                f"Invalid affinity, expected: {affinity}, found: {act_affinity}"
            )


class TestThreadAffinity_OffByOne(TestThreadAffinity):
    @pytest.fixture(scope="class")
    def affinity(self, num_cores: int) -> list[int]:
        return [num_cores]

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid(
        self,
        results: ScenarioResult,
    ) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert (
            "called `Result::unwrap()` on an `Err` value: CpuCoreOutsideOfSupportedCpuRangeForAffinity"
            in results.stderr
        )


class TestThreadAffinity_LargeCoreId(TestThreadAffinity):
    @pytest.fixture(scope="class")
    def affinity(self) -> list[int]:
        return [2**63]

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult, affinity: list[int]) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert (
            "called `Result::unwrap()` on an `Err` value: CpuCoreOutsideOfSupportedCpuRangeForAffinity"
            in results.stderr
        )


class TestThreadAffinity_AffinityMaskTooLarge(TestThreadAffinity):
    @pytest.fixture(scope="class")
    def affinity(self) -> list[int]:
        return list(range(1024 + 1))

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult, affinity: list[int]) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert (
            "called `Result::unwrap()` on an `Err` value: CpuCoreOutsideOfSupportedCpuRangeForAffinity"
            in results.stderr
        )


# endregion

# region thread_stack_size


class TestThreadStackSize(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

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

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert (
            "called `Result::unwrap()` on an `Err` value: StackSizeTooSmall"
            in results.stderr
        )


# endregion
