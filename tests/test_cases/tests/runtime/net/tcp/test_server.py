# *******************************************************************************
# Copyright (c) 2025 Contributors to the Eclipse Foundation
#
# See the NOTICE file(s) distributed with this work for additional
# information regarding copyright ownership.
#
# This program and the accompanying materials are made available under the
# terms of the Apache License Version 2.0 which is available at
# https://www.apache.org/licenses/LICENSE-2.0
#
# SPDX-License-Identifier: Apache-2.0
# *******************************************************************************
import socket
from typing import Any

import pytest
from cit_runtime_scenario import CitRuntimeScenario, Executable
from testing_utils.net import Address, create_connection


class TestTcpServer(CitRuntimeScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.tcp.server.basic"

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
            lambda log_container: (
                log_container.find_log(
                    "message",
                    pattern=f"TCP server listening on {connection_params['ip']}:{connection_params['port']}",
                )
                is not None
            )
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

    def test_server_logs(self, client_connection: socket.socket, executable: Executable) -> None:
        message = b"Let's check External Traits!"
        client_connection.sendall(message)
        executable.wait_for_log(
            lambda log_container: (
                log_container.find_log(field="message", pattern=f"Written {len(message)} bytes") is not None
            )
        )
        logs = executable.get_stdout_until_now()
        assert logs.find_log(field="message", pattern=f"Read {len(message)} bytes") is not None
        assert logs.find_log(field="message_read", pattern=message.decode()) is not None
        assert logs.find_log(field="message", pattern=f"Written {len(message)} bytes") is not None


class TestTcpNoResponseServer(CitRuntimeScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.tcp.server.no_response"

    @pytest.fixture(scope="class")
    def connection_params(self) -> dict[str, Any]:
        return {"ip": "127.0.0.1", "port": 7878}

    @pytest.fixture(scope="class")
    def test_config(self, connection_params: dict[str, Any]) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": connection_params,
        }

    def test_server_logs(self, client_connection: socket.socket, executable: Executable) -> None:
        message = b"Sending a message, out"
        client_connection.sendall(message)
        executable.wait_for_log(
            lambda log_container: (
                log_container.find_log(
                    "message_read",
                    pattern=message.decode(),
                )
                is not None
            )
        )
        logs = executable.get_stdout_until_now()
        assert logs.find_log(field="message_read", pattern=message.decode()) is not None


class TestTcpPollWriteServer(CitRuntimeScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.tcp.server.poll_read_write"

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
        message = b"Echo with poll methods!"
        client_connection.sendall(message)
        data = client_connection.recv(1024)
        assert message == data

    def test_server_logs(self, client_connection: socket.socket, executable: Executable) -> None:
        message = b"Let's check poll methods!"
        client_connection.sendall(message)
        executable.wait_for_log(
            lambda log_container: (
                log_container.find_log(field="message", pattern=f"Written {len(message)} bytes") is not None
            )
        )
        logs = executable.get_stdout_until_now()
        assert logs.find_log(field="message", pattern=f"Read {len(message)} bytes") is not None
        assert logs.find_log(field="message_read", pattern=message.decode()) is not None
        assert logs.find_log(field="message", pattern=f"Written {len(message)} bytes") is not None
