# Component Integration Test Scenarios

## Build

### Cargo
```bash
cargo build
```

### Bazel
TBD..

## Standalone execution of Test Scenarios
### Cargo run
```bash
cargo run -- --name TEST_GROUP.TEST_SCENARIO
```
You will be asked to provide TestInput in JSON format. All test scenarios require runtime to be defined, such as: `{"runtime": {"task_queue_size": 256, "workers": 1}}`
e.g.
```bash
cargo run -- --name orchestration.single_sequence <<< '{"runtime": {"task_queue_size": 256, "workers": 1}}'
```
### Direct binary execution
Test Scenario can be run also directly using binary instead of `cargo run`
```bash
./target/debug/rust_test_scenarios --name orchestration.single_sequence <<< '{"runtime": {"task_queue_size": 256, "workers": 1}}'
```
