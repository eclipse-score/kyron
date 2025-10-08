"""
Runtime test scenario runner for component integration tests.
"""

import json
import time
from collections.abc import Callable
from datetime import timedelta
from queue import Empty, Queue
from subprocess import PIPE, Popen
from sys import stderr
from threading import Thread
from typing import Generator

import pytest
from testing_utils import (
    BuildTools,
    CargoTools,
    LogContainer,
    ResultEntry,
    Scenario,
)

from component_integration_tests.python_test_cases.tests.result_code import (
    ResultCode,
)


class Executable:
    def __init__(self, command: list[str]) -> None:
        self._proc: Popen[str] = Popen(command, stdout=PIPE, stderr=PIPE, text=True, bufsize=1)
        self._stdout: LogContainer = LogContainer()
        self._queue: Queue[ResultEntry] = Queue()
        self._stdout_reader: Thread = Thread(
            target=Executable.process_output,
            args=(self._proc.stdout, self._queue),
        )
        self._stdout_reader.start()

    @staticmethod
    def process_output(stream, queue):
        """
        Read output line by line and put it to the queue.
        Reading stream of the running process can block, so it should be done in a separate thread.
        """
        for line in iter(stream.readline, ""):
            try:
                json_line = json.loads(line.strip())
                json_line["timestamp"] = timedelta(microseconds=int(json_line["timestamp"]))
                queue.put(ResultEntry(json_line))
            except json.decoder.JSONDecodeError:
                print(
                    f"Executable: Ignoring non-JSON stdout line: {line.strip()}",
                    file=stderr,
                )

    def _read_stdout_entry(self, timeout) -> ResultEntry | None:
        """
        Read a ResultEntry from stdout queue with timeout.

        Parameters
        ----------
        timeout : float
            Timeout in seconds.
        """
        try:
            line = self._queue.get(block=True, timeout=timeout)
            self._stdout.add_log(line)
            return line
        except Empty:
            return None

    def _update_stdout(self) -> LogContainer:
        """
        Update stdout with all available lines.
        """
        new_data = LogContainer()
        while not self._queue.empty():
            line = self._read_stdout_entry(timeout=0)
            if line is not None:
                new_data.add_log(line)
        return new_data

    def _kill(self, timeout) -> int | None:
        """
        Kill the process.
        """
        ret_code = None
        self._proc.kill()
        try:
            ret_code = self._proc.wait(timeout)
            print(
                f"Executable: Process finished with return code {ret_code}",
                file=stderr,
            )
        except TimeoutError:
            print(
                f"Executable: Process did not die after kill within {timeout}s, giving up",
                file=stderr,
            )

        return ret_code

    def terminate(self, timeout=3.0) -> int | None:
        """
        Terminate the process and update stdout.
        """
        ret_code = None
        self._proc.terminate()
        try:
            ret_code = self._proc.wait(timeout)
            print(
                f"Executable: Process finished with return code {ret_code}",
                file=stderr,
            )
        except TimeoutError:
            print(
                f"Process did not terminate in {timeout}s, killing it",
                file=stderr,
            )
            ret_code = self._kill(timeout)
        self._update_stdout()
        self._stdout_reader.join()
        return ret_code

    def get_stdout_until_now(self) -> LogContainer:
        """
        Get all stdout lines until now.
        """
        self._update_stdout()
        return self._stdout

    def wait_for_log(
        self,
        found_predicate: Callable[[LogContainer], bool],
        timeout: float = 5.0,
    ):
        """
        Wait for a specific message in stdout until timeout.

        Parameters
        ----------
        found_predicate : Callable[[LogContainer], bool]
            Function to check if expected log is present.
        timeout : float
            Timeout in seconds.
        """
        start = time.time()
        stdout = self.get_stdout_until_now()
        if found_predicate(stdout):
            return

        now = time.time()
        while now - start < timeout:
            to_timeout = timeout - (now - start)
            stdout_line = self._read_stdout_entry(to_timeout)
            if stdout_line is None:
                break

            if found_predicate(LogContainer([stdout_line])):
                return
            now = time.time()

        raise TimeoutError("Timeout while waiting for expected message in the stdout.")


class CitRuntimeScenario(Scenario):
    """
    CIT test scenario definition for interactive testing with binary running continuously.
    It requires executable to be running for the duration of the tests.
    """

    @pytest.fixture(scope="class")
    def build_tools(self, *args, **kwargs) -> BuildTools:
        """
        Build tools used to handle test scenario.
        """
        return CargoTools()

    @pytest.fixture(scope="class")
    def results(
        self,
        process,
        execution_timeout: float,
        *args,
        **kwargs,
    ):
        raise NotImplementedError("Not used in runtime scenarios.")

    @pytest.fixture(scope="class")
    def logs(self, results, *args, **kwargs):
        raise NotImplementedError("Not used in runtime scenarios.")

    def expect_command_failure(self, *args, **kwargs) -> bool:
        """
        Expect executable failure (non-SIGTERM (-15) return code or hang).
        """
        return False

    @pytest.fixture(scope="function")
    def executable(self, command: list[str], *args, **kwargs) -> Generator[Executable, None, None]:
        """
        Start the executable process and terminate it after tests.
        """
        exec = Executable(command)
        yield exec

        ret_code = exec.terminate()
        if not self.expect_command_failure() and ret_code != ResultCode.SIGTERM:
            raise RuntimeError(f"Executable terminated with unexpected return code: {ret_code}")

    @pytest.fixture(autouse=True)
    def print_to_report(
        self,
        request: pytest.FixtureRequest,
        executable: Executable,
    ) -> Generator[None, None, None]:
        """
        Print traces to stdout.

        Allowed "--traces" values:
        - "none" - show no traces.
        - "target" - show traces generated by test code.
        - "all" - show all traces.

        Parameters
        ----------
        request : FixtureRequest
            Test request built-in fixture.
        executable : Executable
            Executable instance.
        """
        traces_param = request.config.getoption("--traces")
        if traces_param not in ("none", "target", "all"):
            raise RuntimeError(f'Invalid "--traces" value: {traces_param}')

        yield  # Traces shoud be printed after test execution

        match traces_param:
            case "all":
                traces = executable.get_stdout_until_now()
            case "target":
                raise NotImplementedError("Not used in runtime scenarios.")
            case "none":
                traces = []
            case _:
                raise RuntimeError(f'Invalid "--traces" value: {traces_param}')

        if traces:
            print("\n")
            for trace in traces:
                print(trace)
