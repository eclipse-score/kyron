from typing import Any

import pytest
from testing_utils import LogContainer
from testing_utils.scenario import ScenarioResult

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)
from component_integration_tests.python_test_cases.tests.result_code import (
    ResultCode,
)


class TestProgramRun(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.program_run"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {
                "run_type": "run",
                "run_count": 0,  # not used in this scenario
                "run_delay": 0,  # not used in this scenario
            },
        }

    @pytest.fixture(scope="class")
    def execution_timeout(self, request, *args, **kwargs):
        return 0.5

    def expect_command_failure(self) -> bool:
        return True

    def test_program_run(self, logs_info_level: LogContainer):
        assert logs_info_level.contains_log(field="id", value="start"), "Program did not start as expected"
        task_logs = logs_info_level.get_logs(field="id", value="basic_task")
        assert len(task_logs) > 1, "Program did not execute tasks as expected"

    def test_program_run_until_timeout(self, logs_info_level: LogContainer, results: ScenarioResult):
        assert not logs_info_level.contains_log(field="id", value="stop"), (
            "Program execution was finished, where it should run indefinitely"
        )

        assert results.hang, "Program execution was not running infinitely as expected"
        # The program should run infinitely, so we kill it after a execution_timeout
        assert results.return_code == ResultCode.SIGKILL, "Program execution was not killed as expected"


class TestProgramRunCycle(TestProgramRun):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.program_run"

    @pytest.fixture(scope="class", params=[0, 10, 100])
    def run_delay(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, run_delay: int) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {
                "run_type": "run_cycle",
                "run_count": 0,  # not used in this scenario
                "run_delay": run_delay,
            },
        }

    @pytest.fixture(scope="class")
    def execution_timeout(self, request, *args, **kwargs):
        return 0.5

    def test_delay_betweeen_runs(self, logs_info_level: LogContainer, run_delay: int):
        task_logs = logs_info_level.get_logs(field="id", value="basic_task")
        execution_timestamps = [log.timestamp for log in task_logs]
        execution_delays = [
            (t2 - t1).total_seconds() * 1000  # convert to ms
            for t1, t2 in zip(execution_timestamps, execution_timestamps[1:])
        ]
        average_delay_ms = sum(execution_delays) / len(execution_delays)
        assert run_delay <= average_delay_ms, "Average delay is less than expected"


class TestProgramRunNTimes(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.program_run"

    @pytest.fixture(scope="class", params=[0, 1, 42])
    def run_count(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, run_count: int) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {
                "run_type": "run_n",
                "run_count": run_count,
                "run_delay": 0,  # not used in this scenario
            },
        }

    def test_program_start_and_finish(self, logs_info_level: LogContainer):
        assert logs_info_level.contains_log(field="id", value="start"), "Program did not start as expected"
        assert logs_info_level.contains_log(field="id", value="stop"), "Program did not stop as expected"

    @pytest.mark.skip(
        reason="Flaky test, needs investigation: https://github.com/qorix-group/inc_orchestrator_internal/issues/278"
    )
    def test_program_run_given_times(self, logs_info_level: LogContainer, test_config: dict[str, Any]):
        expected_run_count = test_config["test"]["run_count"]

        run_logs = logs_info_level.get_logs(field="id", value="basic_task")

        assert len(run_logs) == expected_run_count, f"Expected {expected_run_count} runs, but got {len(run_logs)}"


class TestProgramRunNTimesCycle(TestProgramRunNTimes):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.program_run"

    @pytest.fixture(scope="class", params=[2, 3, 6])
    def run_count(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class", params=[0, 10, 100])
    def run_delay(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, run_count: int, run_delay: int) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {
                "run_type": "run_n_cycle",
                "run_count": run_count,
                "run_delay": run_delay,
            },
        }

    @pytest.mark.skip(
        reason="Flaky test, needs investigation: https://github.com/qorix-group/inc_orchestrator_internal/issues/278"
    )
    def test_delay_betweeen_runs(self, logs_info_level: LogContainer, run_delay: int):
        task_logs = logs_info_level.get_logs(field="id", value="basic_task")
        execution_timestamps = [log.timestamp for log in task_logs]
        execution_delays = [
            (t2 - t1).total_seconds() * 1000  # convert to ms
            for t1, t2 in zip(execution_timestamps, execution_timestamps[1:])
        ]
        average_delay_ms = sum(execution_delays) / len(execution_delays)
        assert run_delay <= average_delay_ms, "Average delay is less than expected"


class TestProgramRunMetered(TestProgramRun):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.program_run_metered"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {
                "run_type": "run_metered",
                "run_count": 0,  # not used in this scenario
                "run_delay": 0,  # not used in this scenario
            },
        }

    @pytest.fixture(scope="class")
    def execution_timeout(self, request, *args, **kwargs):
        return 0.5

    def test_program_meter_output(self, logs_info_level: LogContainer):
        # Meter is a debug utility and accuracy is not checked
        # We don't know how many iterations were executed, so we just check if there were any meter outputs
        assert logs_info_level.contains_log(field="meter_id", pattern=r"simple_run_program"), (
            "Expected meter output not found"
        )


class TestProgramRunCycleMetered(TestProgramRunCycle):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.program_run_metered"

    @pytest.fixture(scope="class", params=[0, 10, 100])
    def run_delay(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, run_delay: int) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {
                "run_type": "run_cycle_metered",
                "run_count": 0,  # not used in this scenario
                "run_delay": run_delay,
            },
        }

    @pytest.fixture(scope="class")
    def execution_timeout(self, request, *args, **kwargs):
        return 0.5

    def test_program_meter_output(self, logs_info_level: LogContainer):
        # Meter is a debug utility and accuracy is not checked
        # We don't know how many iterations were executed, so we just check if there were any meter outputs
        assert logs_info_level.contains_log(field="meter_id", pattern=r"simple_run_program"), (
            "Expected meter output not found"
        )


class TestProgramRunNTimesMetered(TestProgramRunNTimes):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.program_run_metered"

    @pytest.fixture(scope="class", params=[2, 5, 42])
    def run_count(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, run_count: int) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {
                "run_type": "run_n_metered",
                "run_count": run_count,
                "run_delay": 0,  # not used in this scenario
            },
        }

    def test_program_meter_output(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        # Meter is a debug utility and accuracy is not checked
        expected_meter_outputs = test_config["test"]["run_count"]
        meter_outputs = logs_info_level.get_logs(field="meter_id", pattern=r"simple_run_program")

        assert expected_meter_outputs == len(meter_outputs), (
            f"Expected {expected_meter_outputs} meter outputs, but got {len(meter_outputs)}"
        )


class TestProgramRunNTimesCycleMetered(TestProgramRunNTimesCycle):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "basic.program_run_metered"

    @pytest.fixture(scope="class", params=[2, 3, 6])
    def run_count(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class", params=[0, 10, 100])
    def run_delay(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, run_count: int, run_delay: int) -> dict[str, Any]:
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {
                "run_type": "run_n_cycle_metered",
                "run_count": run_count,
                "run_delay": run_delay,
            },
        }

    def test_program_meter_output(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        # Meter is a debug utility and accuracy is not checked
        expected_meter_outputs = test_config["test"]["run_count"]
        meter_outputs = logs_info_level.get_logs(field="meter_id", pattern=r"simple_run_program")

        assert expected_meter_outputs == len(meter_outputs), (
            f"Expected {expected_meter_outputs} meter outputs, but got {len(meter_outputs)}"
        )
