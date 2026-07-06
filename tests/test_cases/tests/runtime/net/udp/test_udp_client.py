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
import sys
from abc import ABC, abstractmethod
from ipaddress import IPv4Address, IPv6Address
from threading import Thread
from typing import Any, Generator

import pytest
from cit_scenario import CitScenario
from testing_utils import LogContainer
from testing_utils.net import Address, IPAddress


class UdpServer(ABC):
    def __init__(self, address: Address) -> None:
        self._socket = socket.socket(address.family(), socket.SOCK_DGRAM)
        self._socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._socket.bind(address.to_raw())
        self._socket.settimeout(1.0)  # Set a timeout to not block on recvfrom indefinitely

        self._thread = Thread(target=self._run_server)
        self._active = False

    def _run_server(self) -> None:
        while self._active:
            try:
                bytes_data, addr = self._socket.recvfrom(1024)
                decoded_msg = bytes_data.decode("utf-8")
                print(f"Received message: {decoded_msg} from {addr}", file=sys.stderr)
                self._handle_message(decoded_msg, addr)
            except OSError:
                print(
                    "UdpServer: recvfrom interrupted (timeout or closed socket)",
                    file=sys.stderr,
                )

    @abstractmethod
    def _handle_message(self, message: str, address: Address) -> None:
        pass

    def start(self) -> None:
        self._active = True
        self._thread.start()

    def stop(self) -> None:
        print("Stopping UdpServer...", file=sys.stderr)
        self._active = False
        self._socket.close()
        self._thread.join()
        print("UdpServer stopped", file=sys.stderr)

    def __enter__(self) -> "UdpServer":
        self.start()
        return self

    def __exit__(self, _type, _value, _traceback) -> None:
        self.stop()


class UdpServerEcho(UdpServer):
    def _handle_message(self, message: str, address: Address) -> None:
        print(f"Echoing back to {address}", file=sys.stderr)
        self._socket.sendto(message.encode("utf-8"), address)


class UdpClientBase(CitScenario):
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
    def message(self) -> str:
        return "TestMessage"

    @pytest.fixture(scope="class", params=["connected", "unconnected"])
    def client_type(self, request: pytest.FixtureRequest) -> str:
        return request.param

    @pytest.fixture(scope="class")
    def test_config(self, connection_params: dict[str, Any], message: str, client_type: str) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": connection_params,
            "message": message,
            "client_type": client_type,
        }

    @pytest.fixture(scope="class", autouse=True)
    def udp_server_echo(self, address: Address, test_config: dict[str, Any]) -> Generator[UdpServer, None, None]:
        # test_config required to restart the server for each test param
        with UdpServerEcho(address) as server:
            yield server


class TestUdpClient(UdpClientBase):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.udp.client.log_response"

    @pytest.fixture(scope="class")
    def server_ready(self, udp_server_echo: UdpServer) -> None:
        return None

    def test_send_receive(self, message: str, udp_server_echo: UdpServer, logs_info_level: LogContainer) -> None:
        log = logs_info_level.find_log("received_message")
        assert log is not None
        assert log.received_message == message


class TestUnconnectedSend(UdpClientBase):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.net.udp.client.unconnected_send_error"

    @pytest.fixture(scope="class")
    def client_type(self) -> str:
        return "unconnected"

    def test_error(self, logs_info_level: LogContainer) -> None:
        log = logs_info_level.find_log("send_error")
        assert log is not None
        assert (
            log.send_error
            == 'Write error: Os { code: 89, kind: Uncategorized, message: "Destination address required" }'
        )
