import pytest
from testing_tools import LogContainer


class TestRuntimeOneWorkerOneTask:
    @pytest.fixture(scope="class")
    def scenario_name(self):
        return "runtime.worker"

    @pytest.fixture(scope="class")
    def test_config(self):
        return {
            "runtime": {"workers": 1, "task_queue_size": 256},
            "test": {"tasks": ["task_1"]},
        }

    def test_if_task_executed(self, test_config, test_results: LogContainer):
        task_name = test_config["test"]["tasks"][0]
        assert test_results.contains_id(task_name), f"Task {task_name} was not executed"


class TestRuntimeTwoWorkersOneTask(TestRuntimeOneWorkerOneTask):
    @pytest.fixture(scope="class")
    def test_config(self):
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {"tasks": ["task_1"]},
        }


class TestRuntimeOneWorkerManyTasks:
    @pytest.fixture(scope="class")
    def scenario_name(self):
        return "runtime.worker"

    @pytest.fixture(scope="class")
    def test_config(self):
        return {
            "runtime": {"workers": 1, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 101)]},
        }

    def test_if_all_tasks_executed(self, test_config, test_results: LogContainer):
        all_expected_set = set(test_config["test"]["tasks"])
        expected_len = len(all_expected_set)

        all_executed_set = set(result.id for result in test_results)
        executed_len = len(all_executed_set)

        assert expected_len == executed_len, (
            f"Expected {expected_len} and executed {executed_len} task count should match."
        )
        assert all_expected_set == all_executed_set, (
            "Some expected tasks did not execute."
        )


class TestRuntimeTwoWorkersEvenTasks(TestRuntimeOneWorkerManyTasks):
    @pytest.fixture(scope="class")
    def test_config(self):
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 11)]},
        }


class TestRuntimeTwoWorkersOddTasks(TestRuntimeOneWorkerManyTasks):
    @pytest.fixture(scope="class")
    def test_config(self):
        return {
            "runtime": {"workers": 2, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 10)]},
        }


class TestRuntimeThreeWorkersEvenTasks(TestRuntimeOneWorkerManyTasks):
    @pytest.fixture(scope="class")
    def test_config(self):
        return {
            "runtime": {"workers": 3, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 11)]},
        }


class TestRuntimeThreeWorkersOddTasks(TestRuntimeOneWorkerManyTasks):
    @pytest.fixture(scope="class")
    def test_config(self):
        return {
            "runtime": {"workers": 3, "task_queue_size": 256},
            "test": {"tasks": [f"task_{ndx}" for ndx in range(1, 10)]},
        }
