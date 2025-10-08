import sys
from ipaddress import IPv4Address, IPv6Address
from socket import (
    SHUT_RDWR,
    SO_REUSEADDR,
    SOCK_STREAM,
    SOL_SOCKET,
    socket,
)
from string import ascii_lowercase
from threading import Thread
from typing import Any, Generator

import pytest
from testing_utils import LogContainer, ScenarioResult
from testing_utils.net import Address, IPAddress

from component_integration_tests.python_test_cases.tests.cit_scenario import (
    CitScenario,
)
from component_integration_tests.python_test_cases.tests.result_code import (
    ResultCode,
)
from component_integration_tests.python_test_cases.tests.runtime.tcp.ttl_helper import (
    get_default_ttl,
)


class EchoServer:
    def __init__(self, address: Address) -> None:
        # Last connected peer address.
        self._peer_addr: Address | None = None

        # Create and configure socket to be reusable.
        self._socket = socket(address.family(), SOCK_STREAM)
        self._socket.setsockopt(SOL_SOCKET, SO_REUSEADDR, 1)

        # Bind to address.
        self._socket.bind(address.to_raw())

        # Setup thread.
        self._thread = Thread(target=self._run_server, args=(address,))
        self._active = False

    def _handle_client(self, conn: socket, addr: Address) -> None:
        print(f"Connected by: {addr}", file=sys.stderr)
        while self._active:
            message = conn.recv(1024)
            if not message:
                print(f"Disconnected by {addr}", file=sys.stderr)
                break
            print(f"Message received from: {addr}", file=sys.stderr)
            conn.send(message)

    def _run_server(self, address: Address) -> None:
        # Listen for connections.
        self._socket.listen(10)

        # Run thread loop.
        while self._active:
            print(f"Waiting for connection: {address}", file=sys.stderr)
            try:
                conn, addr = self._socket.accept()
                addr = Address.from_raw(*addr)
                self._peer_addr = addr
            except OSError:
                # Allow 'accept' to be interrupted.
                print(
                    "Socket accept interrupted, socket being shutdown",
                    file=sys.stderr,
                )
                break

            client_thread = Thread(target=self._handle_client, args=(conn, addr), daemon=True)
            client_thread.start()

    def start(self) -> None:
        self._active = True
        self._thread.start()

    def stop(self) -> None:
        self._active = False
        self._socket.shutdown(SHUT_RDWR)
        self._socket.close()
        self._thread.join()

    def __enter__(self) -> "EchoServer":
        self.start()
        return self

    def __exit__(self, _type, _value, _traceback) -> None:
        self.stop()

    @property
    def peer_addr(self) -> Address | None:
        """
        Last connected peer address.
        """
        return self._peer_addr

    @property
    def local_addr(self) -> Address:
        """
        Local socket address.
        """
        return Address.from_raw(*self._socket.getsockname())


class TestSmoke(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.tcp.tcp_stream.smoke"

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

    @pytest.fixture(scope="class", params=[0, 1, 20, 1024])
    def message(self, request: pytest.FixtureRequest) -> str:
        # Message consists of repeated alphabet.
        message_len = request.param
        alphabet_len = len(ascii_lowercase)
        times = int(message_len / alphabet_len + 1)
        result = ascii_lowercase * times
        return result[:message_len]

    @pytest.fixture(scope="class")
    def test_config(self, ip: IPAddress, port: int, message: str) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": {"ip": str(ip), "port": port},
            "message": message,
        }

    @pytest.fixture(scope="class")
    def echo_server(self, address: Address) -> Generator[EchoServer, None, None]:
        with EchoServer(address) as server:
            yield server

    def test_message_ok(
        self,
        echo_server: EchoServer,
        logs_info_level: LogContainer,
        message: str,
    ) -> None:
        log = logs_info_level.find_log("message_read")
        assert log is not None
        assert log.message_read == message

    def test_addrs_ok(self, echo_server: EchoServer, logs_info_level: LogContainer) -> None:
        log = logs_info_level.find_log("peer_addr")
        assert log is not None
        # Compare local (from server PoV) to peer (from client PoV).
        assert log.peer_addr == str(echo_server.local_addr)
        assert log.local_addr == str(echo_server.peer_addr)


class TestTTL(CitScenario):
    @pytest.fixture(scope="class")
    def scenario_name(self) -> str:
        return "runtime.tcp.tcp_stream.set_get_ttl"

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
    def test_config(self, ip: IPAddress, port: int, ttl: int | None) -> dict[str, Any]:
        return {
            "runtime": {"task_queue_size": 256, "workers": 4},
            "connection": {"ip": str(ip), "port": port, "ttl": ttl},
        }

    @pytest.fixture(scope="class")
    def echo_server(self, address: Address) -> Generator[EchoServer, None, None]:
        with EchoServer(address) as server:
            yield server


class TestTTL_Valid(TestTTL):
    @pytest.fixture(scope="class", params=[None, 1, 64, 128, 255])
    def ttl(self, request: pytest.FixtureRequest) -> int | None:
        # 'None' represents 'not set'.
        return request.param

    def test_ttl_ok(
        self,
        address: Address,
        echo_server: EchoServer,
        logs_info_level: LogContainer,
        ttl: int | None,
    ) -> None:
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
        echo_server: EchoServer,
        results: ScenarioResult,
    ) -> None:
        # Panic inside async causes 'SIGABRT'.
        assert results.return_code == ResultCode.SIGABRT

        assert results.stderr
        assert (
            'Failed to set TTL value: Os { code: 22, kind: InvalidInput, message: "Invalid argument" }'
            in results.stderr
        )
