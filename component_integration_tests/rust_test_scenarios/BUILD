load("@rules_rust//rust:defs.bzl", "rust_binary")
load("@score_orchestrator_crates//:defs.bzl", "all_crate_deps")

rust_binary(
    name = "rust_test_scenarios",
    srcs = glob(["src/**/*.rs"]),
    tags = [
        "manual",
    ],
    visibility = ["//component_integration_tests/python_test_cases:__pkg__"],
    deps = [
        "//src/kyron:libkyron",
        "//src/kyron-foundation:libkyron_foundation",
        "//src/orchestration:liborchestration",
    ] + all_crate_deps(
        normal = True,
    ),
)
