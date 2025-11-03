load("@pip_score_venv_test//:requirements.bzl", "all_requirements")
load("@rules_python//python:pip.bzl", "compile_pip_requirements")
load("@score_python_basics//:defs.bzl", "score_py_pytest", "score_virtualenv")


# Additional requirements for the tests
compile_pip_requirements(
    name = "requirements",
    requirements_txt = "requirements.txt",
    tags = [
        "manual",
    ],
)

score_virtualenv(
    name = "python_tc_venv",
    reqs = all_requirements,
    venv_name = ".python_tc_venv",
)

# Tests targets
score_py_pytest(
    name = "cit",
    srcs = glob(["tests/**/*.py"]),
    args = [
        "--traces=all",
        "--target-path",
        "$(rootpath //component_integration_tests/rust_test_scenarios)",
    ],
    data = [
        ":python_tc_venv",
        "//component_integration_tests/rust_test_scenarios",
    ],
    env = {
        "RUST_BACKTRACE": "1",
    },
    pytest_ini = "//component_integration_tests/python_test_cases:pytest.ini",
    tags = [
        "manual",
    ],
    deps = all_requirements,
)

score_py_pytest(
    name = "cit_repeat",
    srcs = glob(["tests/**/*.py"]),
    args = [
        "--traces=all",
        "--count=5",
        "--repeat-scope=session",
        "--target-path",
        "$(rootpath //component_integration_tests/rust_test_scenarios)",
    ],
    data = [
        ":python_tc_venv",
        "//component_integration_tests/rust_test_scenarios",
    ],
    env = {
        "RUST_BACKTRACE": "1",
    },
    pytest_ini = "//component_integration_tests/python_test_cases:pytest.ini",
    tags = [
        "manual",
    ],
    deps = all_requirements,
)
