from typing import Any

import pytest
from testing_utils import LogContainer, ScenarioResult

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)
from component_integration_tests.python_test_cases.tests.result_code import (
    ResultCode,
)


class TestSPMCBroadcastChannelSendReceive(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.send_receive"

    @pytest.fixture(scope="class", params=[1, 2, 3, 4, 8])
    def workers(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, workers: int) -> dict[str, Any]:
        return {
            "runtime": {"workers": workers, "task_queue_size": 256},
            "test": {
                "data_to_send": [11, 12, 13, 14],
                "receivers": ["receiver_1", "receiver_2", "receiver_3"],
                "max_receiver_count": 4,
            },
        }

    def test_if_sends_succeeded(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        send_logs = logs_info_level.get_logs("id", value="send_task")
        send_data = [log.data for log in send_logs]

        assert send_data == test_config["test"]["data_to_send"], "Not all data was sent successfully."

    def test_if_receives_succeeded(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        for receiver in test_config["test"]["receivers"]:
            receive_logs = logs_info_level.get_logs("id", value=receiver)
            receive_data = [log.data for log in receive_logs]

            assert receive_data == test_config["test"]["data_to_send"], (
                f"Not all data was received successfully by {receiver}."
            )


class TestSPMCBroadcastChannelOverflow(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.create_receivers_only"

    @pytest.fixture(scope="class", params=["clone", "subscribe"])
    def population_method(self, request: pytest.FixtureRequest) -> str:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, population_method: str) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "population_method": population_method,
                "receiver_count": 4,
                "max_receiver_count": 2,
            },
        }

    def test_overflow(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        max_receiver_count = test_config["test"]["max_receiver_count"]
        receiver_count = test_config["test"]["receiver_count"]

        error_count = receiver_count - max_receiver_count
        assert len(logs_info_level) == error_count, f"Expected {error_count} errors."

        for log in logs_info_level:
            assert log.id == "receivers_handle_overflow", "Expected 'receivers_handle_overflow' message."


class TestSPMCBroadcastChannelZeroMaxReceivers(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.create_receivers_only"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "population_method": "clone",
                "receiver_count": 1,
                "max_receiver_count": 0,
            },
        }

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_zero_max_receiver_count(self, results: ScenarioResult):
        assert results.return_code == ResultCode.PANIC
        assert "Failed to create initial receiver" in results.stderr


class TestSPMCBroadcastChannelBigMaxReceivers(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.create_receivers_only"

    @pytest.fixture(scope="class", params=[65535, 60000, 4095, 3000, 255])
    def max_receiver_count(self, request: pytest.FixtureRequest) -> int:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, max_receiver_count) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "population_method": "clone",
                "receiver_count": 1,
                "max_receiver_count": max_receiver_count,
            },
        }

    @pytest.fixture(scope="class")
    def execution_timeout(self, request, *args, **kwargs):
        return 15.0

    def test_big_max_receiver_count(self, results: ScenarioResult):
        assert results.return_code == ResultCode.SUCCESS


class TestSPMCBroadcastChannelNumOfSubscribers(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.num_of_subscribers"

    @pytest.fixture(scope="class", params=["clone", "subscribe"])
    def population_method(self, request: pytest.FixtureRequest) -> str:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, population_method) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "population_method": population_method,
                "receiver_count": 4,
                "max_receiver_count": 3,
            },
        }

    def test_initial_num_of_subscribers(self, logs_info_level: LogContainer):
        initial_value_logs = logs_info_level.get_logs("op_type", value="initial")
        assert len(initial_value_logs) == 1, "Expected exactly one initial value log."

        assert initial_value_logs[0].value == 1, "Expected initial number of subscribers to be 1."

    def test_additional_receivers_with_overflow(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        requested_receiver_count = test_config["test"]["receiver_count"]
        max_receiver_count = test_config["test"]["max_receiver_count"]

        successful_add_count = max_receiver_count - 1  # -1 for the initial subscriber
        expected_adds = [2 + add for add in range(successful_add_count)]

        overflow_count = requested_receiver_count - max_receiver_count
        expected_overflows = [max_receiver_count] * overflow_count

        expected_values = expected_adds + expected_overflows
        received_values = [entry.value for entry in logs_info_level.get_logs("op_type", value="add_receiver")]
        assert expected_values == received_values

    def test_remove_receivers(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        requested_receiver_count = test_config["test"]["receiver_count"]
        max_receiver_count = test_config["test"]["max_receiver_count"]

        max_receivers = min(requested_receiver_count, max_receiver_count)
        expected_values = list(reversed(range(0, max_receivers)))

        remove_logs = logs_info_level.get_logs("op_type", value="drop_receiver")

        received_values = [entry.value for entry in remove_logs]
        assert expected_values == received_values


class TestSPMCBroadcastChannelDropAddReceiver(TestSPMCBroadcastChannelSendReceive):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.drop_add_receiver"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "data_to_send": [11, 12, 13, 14],
                "receivers": ["receiver_1", "receiver_2", "receiver_3"],
                "max_receiver_count": 4,
            },
        }


class TestSPMCBroadcastChannelOneLaggingReceiver(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.send_receive_one_lagging"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "data_to_send": [11, 12, 13, 14],
                "receivers": ["receiver_1", "receiver_2", "receiver_3"],
                "max_receiver_count": 3,
            },
        }

    def test_valid_send_tasks(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        valid_send_logs = logs_info_level.get_logs("id", value="send_task").get_logs("data")

        expected_send_values = test_config["test"]["data_to_send"]
        send_values = [log.data for log in valid_send_logs]
        assert expected_send_values == send_values

    def test_overflow_send_tasks(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        overflow_error = "NoSpaceLeft"
        send_count = len(test_config["test"]["data_to_send"])

        overflow_send_logs = logs_info_level.get_logs("id", value="send_task").get_logs("error", value=overflow_error)
        assert len(overflow_send_logs) == send_count, "Expected overflow messages for the second batch of the values."

    def test_receiver_lagging_task(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        unresponsive_receiver = test_config["test"]["receivers"].pop()
        unresponsive_log = logs_info_level.find_log(field="id", value=unresponsive_receiver)
        assert unresponsive_log is None, "Unresponsive receiver should not have any logs."

    @pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/280")
    def test_receiver_valid_tasks(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        receivers = test_config["test"]["receivers"]
        _unresponsive_receiver = receivers.pop()

        for receiver in receivers:
            valid_logs = logs_info_level.get_logs("id", value=receiver)
            expected_values = test_config["test"]["data_to_send"]
            received_values = [log.data for log in valid_logs]
            assert expected_values == received_values, (
                f"Expected only first batch of values for valid receiver {receiver}."
            )


class TestSPMCBroadcastChannelWithDroppingReceivers(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.variable_receivers"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "data_to_send": [11, 12, 13],
                "receivers": ["receiver_1", "receiver_2", "receiver_3"],
                "max_receiver_count": 3,
            },
        }

    def test_valid_send_task(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        valid_send_logs = logs_info_level.get_logs("id", value="send_task").get_logs("data")
        valid_send_values = [log.data for log in valid_send_logs]

        repetition_count = len(test_config["test"]["receivers"])
        expected_send_values = test_config["test"]["data_to_send"] * repetition_count
        assert expected_send_values == valid_send_values

    def test_no_receivers_send_task(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        no_receivers_error = "GenericError"
        send_count = len(test_config["test"]["data_to_send"])

        overflow_send_logs = logs_info_level.get_logs("id", value="send_task").get_logs(
            "error", value=no_receivers_error
        )
        assert len(overflow_send_logs) == send_count, "Expected overflow messages for the last batch of the values."

    def test_receiving_tasks(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        base_expected_values = test_config["test"]["data_to_send"]

        receivers = test_config["test"]["receivers"]
        for ndx, receiver in enumerate(receivers):
            receiver_logs = logs_info_level.get_logs("id", value=receiver)
            received_values = [log.data for log in receiver_logs]
            expected_values = base_expected_values * (ndx + 1)  # After each send one receiver drops
            assert expected_values == received_values


class TestSPMCBroadcastChannelWithDroppingSender(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.drop_sender"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "data_to_send": [11, 12, 13],
                "receivers": ["receiver_1", "receiver_2", "receiver_3"],
                "max_receiver_count": 3,
            },
        }

    def test_send_task(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        send_logs = logs_info_level.get_logs("id", value="send_task")
        valid_send_values = [log.data for log in send_logs]
        expected_send_values = test_config["test"]["data_to_send"]
        assert expected_send_values == valid_send_values

    def test_receiving_tasks(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        base_expected_values = test_config["test"]["data_to_send"]
        expected_values = base_expected_values + ["Provider dropped"]  # Last read should end with error

        receivers = test_config["test"]["receivers"]
        for receiver in receivers:
            receiver_logs = logs_info_level.get_logs("id", value=receiver)

            received_values = [log.data if hasattr(log, "data") else log.error for log in receiver_logs]
            assert expected_values == received_values


class TestSPMCBroadcastChannelHeavyLoad(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spmc_broadcast.heavy_load"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "send_count": 1000000,
                "receivers": ["receiver_1", "receiver_2"],
                "max_receiver_count": 2,
            },
        }

    @pytest.fixture(scope="class")
    def execution_timeout(self, request, *args, **kwargs):
        return 10.0

    @pytest.mark.xfail(reason="https://github.com/qorix-group/inc_orchestrator_internal/issues/280")
    def test_message_completeness(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        receivers = test_config["test"]["receivers"]
        iteration_count = test_config["test"]["send_count"]
        for receiver in receivers:
            receiver_log = logs_info_level.find_log(field="id", value=receiver)
            assert (receiver_log.iter, receiver_log.error) == (
                iteration_count,
                "Provider dropped",
            ), "Expected error for the over-read of the receiver."
