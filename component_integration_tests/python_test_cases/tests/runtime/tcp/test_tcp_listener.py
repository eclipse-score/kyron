from ipaddress import IPv4Address, IPv6Address
from socket import socket
from typing import Any

import pytest
from testing_utils import LogContainer, ScenarioResult
from testing_utils.net import Address, IPAddress, create_connection

from component_integration_tests.python_test_cases.tests.cit_runtime_scenario import (
    CitRuntimeScenario,
    Executable,
)
from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)
from component_integration_tests.python_test_cases.tests.result_code import (
    ResultCode,
)
from component_integration_tests.python_test_cases.tests.runtime.tcp.ttl_helper import (
    get_default_ttl,
)


class TestSmoke(CitRuntimeScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.tcp.tcp_listener.smoke"

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

    @staticmethod
    def _wait_for_listen(address: Address, executable: Executable) -> None:
        executable.wait_for_log(
            lambda lc: lc.find_log("message", value=f"TCP server listening on {address}") is not None
        )

    @staticmethod
    def _decode(message: bytes) -> str:
        return message.decode().rstrip("\0")

    @pytest.fixture(scope="class")
    def test_config(self, address: Address) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": {"ip": str(address.ip), "port": address.port},
        }

    def test_connect_parallel_ok(self, address: Address, executable: Executable) -> None:
        self._wait_for_listen(address, executable)

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

            assert msg1.decode() == self._decode(data1)
            assert msg2.decode() == self._decode(data2)
            assert msg3.decode() == self._decode(data3)

    def test_connect_serially_ok(self, address: Address, executable: Executable) -> None:
        self._wait_for_listen(address, executable)

        for i in range(3):
            conn = create_connection(address)
            with conn:
                message = f"message{i}".encode()
                conn.sendall(message)
                data = conn.recv(1024)

                assert message.decode() == self._decode(data)

    def test_addrs_ok(self, address: Address, executable: Executable) -> None:
        self._wait_for_listen(address, executable)

        for i in range(3):
            conn: socket = create_connection(address)
            with conn:

                def _check_logs(lc: LogContainer) -> bool:
                    # Compare addresses used by server and client.
                    server_peer_addr = str(Address.from_raw(*conn.getsockname()))
                    server_local_addr = str(Address.from_raw(*conn.getpeername()))
                    # Log containing same port number might occur multiple times.
                    # Not an error - just use latest matching.
                    logs = lc.get_logs("peer_addr", value=str(server_peer_addr))
                    log = logs[-1] if logs else None
                    return log is not None and log.peer_addr == server_peer_addr and log.local_addr == server_local_addr

                executable.wait_for_log(_check_logs)

                message = f"message{i}".encode()
                conn.sendall(message)
                data = conn.recv(1024)

                assert message.decode() == self._decode(data)


class TestTTL(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.tcp.tcp_listener.set_get_ttl"

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
    def test_config(self, address: Address, ttl: int | None) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": {
                "ip": str(address.ip),
                "port": address.port,
                "ttl": ttl,
            },
        }


class TestTTL_Valid(TestTTL):
    @pytest.fixture(scope="class", params=[None, 1, 64, 128, 255])
    def ttl(self, request: pytest.FixtureRequest) -> int | None:
        # 'None' represents 'not set'.
        return request.param

    def test_ttl_ok(self, address: Address, logs_info_level: LogContainer, ttl: int | None) -> None:
        expected_ttl = ttl or get_default_ttl(address.family())
        log = logs_info_level.find_log("ttl")
        assert log is not None
        assert log.ttl == expected_ttl


class TestTTL_Invalid(TestTTL):
    @pytest.fixture(scope="class", params=[0, 256, 257, 2**16, 2**32 - 1])
    def ttl(self, request: pytest.FixtureRequest) -> int | None:
        value = request.param
        if value == 2**32 - 1:
            pytest.xfail(
                reason="Max u32 value sets TTL to default - https://github.com/qorix-group/inc_orchestrator_internal/issues/327"
            )

        return value

    def capture_stderr(self) -> bool:
        return True

    def expect_command_failure(self) -> bool:
        return True

    def test_ttl_invalid(
        self,
        results: ScenarioResult,
    ) -> None:
        # Panic inside async causes 'SIGABRT'.
        assert results.return_code == ResultCode.SIGABRT

        assert results.stderr
        assert (
            'Failed to set TTL value: Os { code: 22, kind: InvalidInput, message: "Invalid argument" }'
            in results.stderr
        )
