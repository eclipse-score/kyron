import json
from pathlib import Path
from platform import platform
from typing import Any

import psutil
import pytest
from testing_utils import LogContainer, ScenarioResult, cap_utils

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)
from component_integration_tests.python_test_cases.tests.result_code import (
    ResultCode,
)


class TestOnlyDedicatedWorkers(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.dedicated_worker.only_dedicated_workers"

    @pytest.fixture(scope="class", params=[1, 10])
    def num_dedicated(self, request: pytest.FixtureRequest) -> int:
        # Dedicated workers don't belong to regular workers pool.
        # - Run only one dedicated worker.
        # - Run more dedicated workers than regular workers available.
        return request.param

    @pytest.fixture(scope="class")
    def dedicated_workers(self, num_dedicated: int) -> list[dict[str, Any]]:
        result = []
        for i in range(num_dedicated):
            result.append({"id": f"dedicated_worker_{i}"})
        return result

    @pytest.fixture(scope="class")
    def test_config(self, dedicated_workers: list[dict[str, Any]]) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": dedicated_workers,
            }
        }

    def test_valid(
        self,
        logs_info_level: LogContainer,
        dedicated_workers: list[dict[str, Any]],
    ) -> None:
        # Check dedicated workers barrier wait result.
        wait_result_log = logs_info_level.find_log("wait_result")
        assert wait_result_log is not None
        assert wait_result_log.wait_result == "ok"

        # Check dedicated worker IDs.
        worker_logs = logs_info_level.get_logs("id", pattern="dedicated_worker_.*")
        act_worker_ids = {log.id for log in worker_logs}
        exp_worker_ids = {dw["id"] for dw in dedicated_workers}
        assert act_worker_ids == exp_worker_ids, (
            f"Mismatch between worker IDs expected ({exp_worker_ids}) and actual ({act_worker_ids})"
        )


class TestSpawnToUnregisteredWorker(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.dedicated_worker.spawn_to_unregistered_worker"

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

    def test_invalid(self, results: ScenarioResult) -> None:
        # Panic inside async causes 'SIGABRT'.
        # TODO: determine this should be panic, and not an error.  # noqa: FIX002
        assert results.return_code == ResultCode.SIGABRT

        assert results.stderr
        assert "Tried to spawn on not registered dedicated worker UniqueWorkerId" in results.stderr


class TestReregisterDedicatedWorker(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        # Scenario is reused, should fail on init.
        return "runtime.worker.dedicated_worker.spawn_to_unregistered_worker"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": [{"id": "same_id"}, {"id": "same_id"}],
            }
        }

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.PANIC

        assert results.stderr
        assert "Cannot register same unique worker multiple times!" in results.stderr


class BlockOneUseOther(CitScenario):
    @pytest.fixture(scope="class")
    def num_workers(self) -> int:
        return 4

    @pytest.fixture(scope="class", params=[1, 10])
    def num_dedicated(self, request: pytest.FixtureRequest) -> int:
        # Dedicated workers don't belong to regular workers pool.
        # - Run only one dedicated worker.
        # - Run more dedicated workers than regular workers available.
        return request.param

    @pytest.fixture(scope="class")
    def dedicated_workers(self, num_dedicated: int) -> list[dict[str, Any]]:
        result = []
        for i in range(num_dedicated):
            result.append({"id": f"dedicated_worker_{i}"})
        return result

    @pytest.fixture(scope="class")
    def test_config(self, num_workers: int, dedicated_workers: list[dict[str, Any]]) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": num_workers,
                "dedicated_workers": dedicated_workers,
            }
        }

    def test_valid(
        self,
        logs_info_level: LogContainer,
        num_workers: int,
        dedicated_workers: list[dict[str, Any]],
    ) -> None:
        num_dedicated_workers = len(dedicated_workers)
        # Check for barrier wait results.
        assert logs_info_level.find_log("ded_wait_result", value="ok")
        assert logs_info_level.find_log("reg_wait_result", value="ok")

        # Get logs containing worker IDs.
        worker_logs = logs_info_level.get_logs("id")

        # Check regular worker IDs.
        reg_worker_logs = worker_logs.get_logs("id", pattern=r"^worker_\d*")
        assert len(reg_worker_logs) == num_workers

        act_reg_worker_ids = {log.id for log in reg_worker_logs}
        exp_reg_worker_ids = {f"worker_{i}" for i in range(num_workers)}
        assert act_reg_worker_ids == exp_reg_worker_ids, (
            f"Mismatch between worker IDs expected ({exp_reg_worker_ids}) and actual ({act_reg_worker_ids})"
        )

        # Check dedicated worker IDs.
        ded_worker_logs = worker_logs.get_logs("id", pattern=r"^dedicated_worker_\d*")
        assert len(ded_worker_logs) == num_dedicated_workers

        act_ded_worker_ids = {log.id for log in ded_worker_logs}
        exp_ded_worker_ids = {dw["id"] for dw in dedicated_workers}
        assert act_ded_worker_ids == exp_ded_worker_ids, (
            f"Mismatch between worker IDs expected ({exp_ded_worker_ids}) and actual ({act_ded_worker_ids})"
        )

        # Check all worker thread IDs are unique.
        thread_ids = {log.thread_id for log in worker_logs}
        assert len(thread_ids) == (num_workers + num_dedicated_workers)


class TestBlockAllRegularWorkOnDedicated(BlockOneUseOther):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.dedicated_worker.block_all_regular_work_on_dedicated"


class TestBlockDedicatedWorkOnRegular(BlockOneUseOther):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.dedicated_worker.block_dedicated_work_on_regular"


# region thread parameters tests


@pytest.mark.root_required
@pytest.mark.skipif("WSL" in platform(), reason="Not supported on WSL")
class TestDedicatedWorkerPriority(CitScenario):
    SCHEDULER = "fifo"

    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.dedicated_worker.thread_priority"

    @pytest.fixture(scope="class", params=[0, 128, 255])
    def priority(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class", params=[1, 10])
    def num_dedicated(self, request: pytest.FixtureRequest) -> int:
        # Dedicated workers don't belong to regular workers pool.
        # - Run only one dedicated worker.
        # - Run more dedicated workers than regular workers available.
        return request.param

    @pytest.fixture(scope="class")
    def dedicated_workers(self, num_dedicated: int, priority: int) -> list[dict[str, Any]]:
        result = []
        for i in range(num_dedicated):
            result.append(
                {
                    "id": f"dedicated_worker_{i}",
                    "thread_priority": priority,
                    "thread_scheduler": self.SCHEDULER,
                }
            )
        return result

    @pytest.fixture(scope="class")
    def test_config(self, dedicated_workers: list[dict[str, Any]]) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": dedicated_workers,
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
        if caps.get("cap_sys_nice", "") != "ep":
            cap_utils.set_caps(resolved_target_path, {"cap_sys_nice": "ep"})

        return self._run_command(command, execution_timeout, args, kwargs)

    def test_valid(
        self,
        logs_info_level: LogContainer,
        priority: int,
        num_dedicated: int,
    ) -> None:
        # Find logs with worker IDs.
        worker_logs = logs_info_level.get_logs(field="id", pattern="dedicated_worker_.*")
        assert len(worker_logs) == num_dedicated

        # Check priority of each worker.
        for worker_log in worker_logs:
            act_priority = worker_log.priority

            # Check priority as expected and in expected bounds.
            assert priority == act_priority, f"Invalid priority, expected: {priority}, found: {act_priority}"


class TestDedicatedWorkerAffinity(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.worker.dedicated_worker.thread_affinity"

    @pytest.fixture(scope="class")
    def num_cores(self) -> int:
        num_cores = psutil.cpu_count()
        if num_cores is None or num_cores == 0:
            raise RuntimeError("Undetermined number of cores")
        return num_cores

    @pytest.fixture(scope="class", params=[1, 10])
    def num_dedicated(self, request: pytest.FixtureRequest) -> int:
        # Dedicated workers don't belong to regular workers pool.
        # - Run only one dedicated worker.
        # - Run more dedicated workers than regular workers available.
        return request.param

    @pytest.fixture(scope="class")
    def dedicated_workers(self, num_dedicated: int, affinity: list[int]) -> list[dict[str, Any]]:
        result = []
        for i in range(num_dedicated):
            result.append({"id": f"dedicated_worker_{i}", "affinity": affinity})
        return result

    @pytest.fixture(scope="class")
    def test_config(self, dedicated_workers: list[dict[str, Any]]) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 4,
                "dedicated_workers": dedicated_workers,
            }
        }


class TestDedicatedWorkerAffinity_Valid(TestDedicatedWorkerAffinity):
    TEST_MODES = ["first", "mid", "last", "multiple", "all"]

    @pytest.fixture(scope="class", params=TEST_MODES, ids=TEST_MODES)
    def affinity(
        self,
        request: pytest.FixtureRequest,
        num_cores: int,
    ) -> list[int]:
        # Available affinity tests are dependent on number of cores available.
        mode = request.param

        def check_num_cores(num_required: int):
            if num_cores < num_required:
                pytest.skip(reason=f"Test requires more CPU cores, required: {num_required}, available: {num_cores}")

        if mode != "all":
            pytest.xfail(
                reason="Affinity not set - https://github.com/qorix-group/inc_orchestrator_internal/issues/331"
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
        logs_info_level: LogContainer,
        affinity: list[int],
        num_dedicated: int,
    ) -> None:
        # Find logs with worker IDs.
        worker_logs = logs_info_level.get_logs(field="id", pattern="dedicated_worker_.*")
        assert len(worker_logs) == num_dedicated

        # Check affinity of each worker.
        for worker_log in worker_logs:
            # Convert affinity string to list.
            act_affinity = json.loads(worker_log.affinity)

            # Check affinity as expected.
            assert affinity == act_affinity, f"Invalid affinity, expected: {affinity}, found: {act_affinity}"


@pytest.mark.xfail(reason="Affinity not set - https://github.com/qorix-group/inc_orchestrator_internal/issues/331")
class TestDedicatedWorkerAffinity_OffByOne(TestDedicatedWorkerAffinity):
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


@pytest.mark.xfail(reason="Affinity not set - https://github.com/qorix-group/inc_orchestrator_internal/issues/331")
class TestDedicatedWorkerAffinity_LargeCoreId(TestDedicatedWorkerAffinity):
    @pytest.fixture(scope="class")
    def affinity(self) -> list[int]:
        return [2**63]

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert (
            "called `Result::unwrap()` on an `Err` value: CpuCoreOutsideOfSupportedCpuRangeForAffinity"
            in results.stderr
        )


@pytest.mark.xfail(reason="Affinity not set - https://github.com/qorix-group/inc_orchestrator_internal/issues/331")
class TestDedicatedWorkerAffinity_AffinityMaskTooLarge(TestDedicatedWorkerAffinity):
    @pytest.fixture(scope="class")
    def affinity(self) -> list[int]:
        return list(range(1024 + 1))

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_invalid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.PANIC
        assert results.stderr is not None
        assert (
            "called `Result::unwrap()` on an `Err` value: CpuCoreOutsideOfSupportedCpuRangeForAffinity"
            in results.stderr
        )


class TestDedicatedWorkerStackSize(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.only_shutdown"

    @pytest.fixture(scope="class")
    def test_config(self, thread_stack_size: int) -> dict[str, Any]:
        return {
            "runtime": {
                "task_queue_size": 256,
                "workers": 1,
                "dedicated_workers": [
                    {
                        "id": "dedicated_worker_0",
                        "thread_stack_size": thread_stack_size,
                    }
                ],
            }
        }


class TestDedicatedWorkerStackSize_Valid(TestDedicatedWorkerStackSize):
    @pytest.fixture(scope="class", params=[1024 * 128, 1024 * 1024])
    def thread_stack_size(self, request: pytest.FixtureRequest) -> int:
        return request.param

    def test_valid(self, results: ScenarioResult) -> None:
        assert results.return_code == ResultCode.SUCCESS


class TestDedicatedWorkerStackSize_TooSmall(TestDedicatedWorkerStackSize):
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
        assert "called `Result::unwrap()` on an `Err` value: StackSizeTooSmall" in results.stderr


# endregion
