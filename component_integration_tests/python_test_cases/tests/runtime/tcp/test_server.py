import socket
from typing import Any

import pytest
from testing_utils.net import Address, create_connection

from component_integration_tests.python_test_cases.tests.cit_runtime_scenario import (
    CitRuntimeScenario,
    Executable,
)


class TestTcpServer(CitRuntimeScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.tcp.basic_server"

    @pytest.fixture(scope="class")
    def connection_params(self) -> dict[str, Any]:
        return {"ip": "127.0.0.1", "port": 7878}

    @pytest.fixture(scope="class")
    def test_config(self, connection_params: dict[str, Any]) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": connection_params,
        }

    def test_tcp_echo(self, client_connection: socket.socket) -> None:
        message = b"Echo!"
        client_connection.sendall(message)
        data = client_connection.recv(1024)
        assert message == data

    def test_multiple_connections(self, connection_params: dict[str, Any], executable: Executable) -> None:
        executable.wait_for_log(
            lambda log_container: log_container.find_log(
                "message",
                pattern=f"TCP server listening on {connection_params['ip']}:{connection_params['port']}",
            )
            is not None
        )

        address = Address.from_dict(connection_params)
        conn1 = create_connection(address)
        conn2 = create_connection(address)
        conn3 = create_connection(address)

        with conn1, conn2, conn3:
            msg1 = b"Uno"
            msg2 = b"Dos"
            msg3 = b"Tres"

            conn1.sendall(msg1)
            conn2.sendall(msg2)
            conn3.sendall(msg3)

            data1 = conn1.recv(1024)
            data2 = conn2.recv(1024)
            data3 = conn3.recv(1024)

            assert msg1 == data1
            assert msg2 == data2
            assert msg3 == data3
