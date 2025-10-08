from typing import Any

import pytest
from testing_utils import LogContainer

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)


class TestSPSCChannelSendReceive(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spsc.send_receive"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {"data_to_send": [11, 12, 13]},
        }

    def test_if_sends_succeeded(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        send_logs = logs_info_level.get_logs("id", value="send_task")
        assert len(send_logs) == len(test_config["test"]["data_to_send"]), "Not all data was sent successfully."

        expected_data = test_config["test"]["data_to_send"]
        for log in send_logs:
            assert log.data in expected_data, f"Sent data {log.data} is not in the expected data list {expected_data}."

    def test_if_receives_succeeded(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        receive_logs = logs_info_level.get_logs("id", value="receive_task")
        assert len(receive_logs) == len(test_config["test"]["data_to_send"]), "Not all data was received successfully."

        expected_data = test_config["test"]["data_to_send"]
        for log in receive_logs:
            assert log.data in expected_data, (
                f"Received data {log.data} is not in the expected data list {expected_data}."
            )


class TestSPSCChannelOverflow(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spsc.send_only"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {"data_to_send": [11, 12, 13, 14, 15, 16, 17]},
        }

    def test_message_completeness(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        data_to_send = test_config["test"]["data_to_send"]
        assert len(logs_info_level) == len(data_to_send), "Not all data was tried to send."

    def test_if_sends_succeeded_until_overflow(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        QUEUE_SIZE = 5  # Compile time constant in backend code

        data_logs = logs_info_level.get_logs("data")
        assert len(data_logs) == QUEUE_SIZE, "Message count not equal to queue size."

        expected_data = test_config["test"]["data_to_send"]
        for log in data_logs:
            assert log.data in expected_data, f"Sent data {log.data} is not in the expected data list {expected_data}."

    def test_if_overflow_was_detected(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        QUEUE_SIZE = 5  # Compile time constant in backend code
        all_data_size = len(test_config["test"]["data_to_send"])
        expected_overflow_count = all_data_size - QUEUE_SIZE

        overflow_logs = logs_info_level.get_logs("error", value="NoSpaceLeft")
        assert len(overflow_logs) == expected_overflow_count, "Queue overflow message count mismatch."


class TestSPSCChannelReceiverDropped(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spsc.drop_receiver"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {"data_to_send": [11, 12]},
        }

    def test_send_errors(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        expected_error_count = len(test_config["test"]["data_to_send"])
        assert len(logs_info_level) == expected_error_count, (
            "Send error count does not match the number of sent messages."
        )

        for log in logs_info_level:
            assert log.error == "GenericError", (
                f"Expected 'GenericError' indicating that receiver is gone, got {log.error}."
            )


class TestSPSCChannelSenderDropped(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spsc.drop_sender"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {"data_to_send": [11, 12]},
        }

    def test_receive_errors(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        expected_error_count = len(test_config["test"]["data_to_send"])
        assert len(logs_info_level) == expected_error_count, (
            "Retrieve error count does not match the number of sent messages."
        )

        for log in logs_info_level:
            assert log.error == "Provider dropped", f"Expected 'Provider dropped' error message, got {log.error}."


class TestSPSCChannelSenderDroppedInTheMiddle(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spsc.drop_sender_in_the_middle"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {"data_to_send": [11, 12, 13], "overread_count": 2},
        }

    def test_valid_transfer(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        send_logs = logs_info_level.get_logs("id", value="send_task").get_logs("data")

        expected_valid_msg_count = len(test_config["test"]["data_to_send"])
        assert len(send_logs) == expected_valid_msg_count, (
            "Not all data was sent successfully before receiver was dropped."
        )

        receive_logs = logs_info_level.get_logs("id", value="receive_task").get_logs("data")
        assert len(receive_logs) == expected_valid_msg_count, (
            "Not all data was received successfully before receiver was dropped."
        )

        for send_log, receive_log in zip(send_logs, receive_logs):
            assert send_log.data == receive_log.data, (
                f"Sent data {send_log.data} does not match received data {receive_log.data}."
            )

    def test_receive_errors(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        receive_error_logs = logs_info_level.get_logs("id", value="receive_task").get_logs(
            "error", value="Provider dropped"
        )

        expected_error_count = test_config["test"]["overread_count"]
        assert len(receive_error_logs) == expected_error_count, (
            f"Expected receive error count is {expected_error_count}."
        )

        for log in receive_error_logs:
            assert log.error == "Provider dropped", f"Expected 'Provider dropped' message, got {log.error}."


class TestSPSCChannelReceiverDroppedInTheMiddle(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spsc.drop_receiver_in_the_middle"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {
                "first_step_data": [11, 12, 13],
                "second_step_data": [14, 15, 16],
            },
        }

    def test_valid_transfer(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        send_logs = logs_info_level.get_logs("id", value="send_task").get_logs("data")

        expected_valid_msg_count = len(test_config["test"]["first_step_data"])
        assert len(send_logs) == expected_valid_msg_count, (
            "Not all data was sent successfully before receiver was dropped."
        )

        receive_logs = logs_info_level.get_logs("id", value="receive_task")
        for send_log, receive_log in zip(send_logs, receive_logs):
            assert send_log.data == receive_log.data, (
                f"Sent data {send_log.data} does not match received data {receive_log.data}."
            )

    def test_send_errors(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        send_error_logs = logs_info_level.get_logs("id", value="send_task").get_logs("error", value="GenericError")

        expected_error_count = len(test_config["test"]["second_step_data"])
        assert len(send_error_logs) == expected_error_count, (
            "Send error count does not match the number of sent messages after receiver is dropped."
        )

        for log in send_error_logs:
            assert log.error == "GenericError", (
                f"Expected 'GenericError' indicating that receiver is gone, got {log.error}."
            )


class TestSPSCChannelHeavyLoad(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.channel.spsc.heavy_load"

    @pytest.fixture(scope="class")
    def test_config(self) -> dict[str, Any]:
        return {
            "runtime": {"workers": 4, "task_queue_size": 256},
            "test": {"send_count": 1000000},
        }

    @pytest.fixture(scope="class")
    def execution_timeout(self, request, *args, **kwargs):
        return 10

    # Test validates transfer on the backend side.
    # Only 1 error is expected for trying to read one more time
    def test_transfer_completeness(self, test_config: dict[str, Any], logs_info_level: LogContainer):
        valid_data_count = test_config["test"]["send_count"]
        assert len(logs_info_level) == 1, (
            "Expected exactly one error message for trying to read 1 more message than transferred."
        )
        assert logs_info_level[0].error == "Provider dropped" and logs_info_level[0].iter == valid_data_count + 1, (
            f"Expected 'Provider dropped' error for the one additional iteration "
            f"over valid data count ({valid_data_count + 1})"
        )
