import pytest
import testing_tools as tt


@pytest.fixture(scope="class")
def unfiltered_test_results(execute_rust):
    messages = execute_rust
    logs = [tt.ResultOrchestration(msg) for msg in messages]
    return tt.LogContainer.from_entries(logs)
