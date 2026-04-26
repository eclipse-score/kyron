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
from ipaddress import IPv4Address, IPv6Address
from socket import SOCK_DGRAM, socket
from typing import Any

import pytest
from cit_runtime_scenario import CitRuntimeScenario, Executable
from cit_scenario import CitScenario
from result_code import ResultCode
from testing_utils import LogContainer, ScenarioResult
from testing_utils.net import Address, IPAddress
from ttl_helper import get_default_ttl


class TestUdpServer(CitRuntimeScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.udp.server.send_receive_echo"

    @pytest.fixture(
        scope="class",
        params=[IPv4Address("127.0.0.1"), IPv6Address("::1")],
        ids=["v4", "v6"],
    )
    def ip(self, request: pytest.FixtureRequest) -> IPAddress:
        return request.param

    @pytest.fixture(scope="class")
    def port(self) -> int:
        return 7878

    @pytest.fixture(scope="class")
    def connection_params(self, ip: IPAddress, port: int) -> dict[str, Any]:
        return {"ip": str(ip), "port": port}

    @pytest.fixture(scope="class")
    def test_config(self, connection_params: dict[str, Any]) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": connection_params,
        }

    def test_single_client_send_receive(self, connection_params: dict[str, Any], udp_client: socket) -> None:
        message = b"Echo!"
        udp_client.sendto(message, tuple(connection_params.values()))
        received_data, addr = udp_client.recvfrom(1024)

        assert received_data == message
        assert addr[:2] == tuple(connection_params.values())

    def test_multiple_client_send_receive(self, connection_params: dict[str, Any], executable: Executable) -> None:
        executable.wait_for_log(
            lambda log_container: (
                log_container.find_log(
                    "message",
                    pattern=f"UDP server listener running on {connection_params['ip']}:{connection_params['port']}",
                )
                is not None
            )
        )

        address = Address.from_dict(connection_params)

        clients = [socket(address.family(), SOCK_DGRAM) for _ in range(3)]
        messages = [b"One", b"Two", b"Three"]

        for client, message in zip(clients, messages):
            with client:
                connection_tuple = tuple(connection_params.values())
                client.sendto(message, connection_tuple)
                received_data, addr = client.recvfrom(1024)

                assert received_data == message
                assert addr[:2] == connection_tuple


class TestDefaultTTL(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.udp.server.log_ttl"

    @pytest.fixture(
        scope="class",
        params=[IPv4Address("127.0.0.1"), IPv6Address("::1")],
        ids=["v4", "v6"],
    )
    def ip(self, request: pytest.FixtureRequest) -> IPAddress:
        return request.param

    @pytest.fixture(scope="class")
    def port(self) -> int:
        return 7878

    @pytest.fixture(scope="class")
    def address(self, ip: IPAddress, port: int) -> Address:
        return Address(ip, port)

    @pytest.fixture(scope="class")
    def connection_params(self, ip: IPAddress, port: int) -> dict[str, Any]:
        return {"ip": str(ip), "port": port}

    @pytest.fixture(scope="class")
    def test_config(self, connection_params: dict[str, Any]) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": connection_params,
        }

    def test_ttl(self, address: Address, logs_info_level: LogContainer) -> None:
        expected_ttl = get_default_ttl(address.family())
        log = logs_info_level.find_log("ttl")
        assert log is not None
        assert log.ttl == expected_ttl


class TestSetTTL(TestDefaultTTL):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.udp.server.log_ttl"

    @pytest.fixture(scope="class", params=[1, 100, 255])
    def ttl(self, request: pytest.FixtureRequest) -> int | None:
        return request.param

    @pytest.fixture(scope="class")
    def connection_params(self, ip: IPAddress, port: int, ttl: int) -> dict[str, Any]:
        return {"ip": str(ip), "port": port, "ttl": ttl}

    def test_ttl(self, address: Address, ttl: int, logs_info_level: LogContainer) -> None:
        expected_ttl = ttl
        log = logs_info_level.find_log("ttl")
        assert log is not None
        assert log.ttl == expected_ttl


class TestSetInvalidTTL(TestDefaultTTL):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.udp.server.log_ttl"

    @pytest.fixture(
        scope="class",
        params=[0],
    )
    def ttl(self, request: pytest.FixtureRequest) -> int | None:
        return request.param

    @pytest.fixture(scope="class")
    def connection_params(self, ip: IPAddress, port: int, ttl: int) -> dict[str, Any]:
        return {"ip": str(ip), "port": port, "ttl": ttl}

    def expect_command_failure(self) -> bool:
        return True

    def capture_stderr(self) -> bool:
        return True

    def test_ttl(
        self,
        address: Address,
        ttl: int,
        results: ScenarioResult,
        logs_info_level: LogContainer,
    ) -> None:
        # Panic inside async causes 'SIGABRT'.
        assert results.return_code == ResultCode.SIGABRT

        assert results.stderr
        assert "Failed to set TTL value" in results.stderr
