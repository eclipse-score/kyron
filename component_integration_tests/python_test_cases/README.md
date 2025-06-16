# Component Integration Test Cases

## Prerequisites
 * python 3.12+
 * pip3
 * venv
 * built binary with Test Scenarios -> [README](../rust_test_scenarios/README.md)

## Executing tests
 1. Create venv
 ```bash
 python3 -m venv .venv
 ```
 2. Activate venv
 ```bash
 source .venv/bin/activate
 ```
 3. Install dependent packages
 ```bash
 python -m pip install -r requirements.txt
 ```
 4. Run the tests
 ```sh
 TEST_BINARY_PATH=../rust_test_scenarios/target/debug/rust_test_scenarios python -m pytest -vv --self-contained-html --html=report.html
 ```
