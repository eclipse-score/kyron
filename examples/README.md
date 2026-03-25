# Kyron Examples

Kyron examples are located in [examples](../src/kyron/examples) directory due to the fact
that Kyron is published to `crates.io` and official project structure needs to be followed.
In this directory there are Bazel aliases located for compatybility.

## Running Examples

All examples can be run using Bazel:

```bash
bazel run //examples:select
bazel run //examples:main_macro
bazel run //examples:mpmc
bazel run //examples:safety_task
```

## Available Examples

### select.rs

Demonstrates the `select!` macro for waiting on multiple async tasks and handling whichever completes first. This example shows:

- Using `select!` to race multiple spawned tasks
- Pattern matching on different future results
- Handling both success (`Ok`) and error (`Err`) cases in select branches
- Working with the safety worker

**Key concepts:** `select!` macro, concurrent task racing, pattern matching futures

### main_macro.rs

A simple example showing how to use the `#[kyron::main]` macro to quickly set up an async runtime with default parameters.

- Spawning multiple async tasks in a loop
- Using the main macro for simplified runtime initialization
- Waiting on task handles with `await`

**Key concepts:** `#[kyron::main]` macro, task spawning, async/await

### mpmc.rs

Demonstrates the work-stealing behavior of Kyron's task scheduler with a Multi-Producer Multi-Consumer (MPMC) queue pattern.

This example illustrates:

- How tasks are distributed between local and global queues
- Work-stealing mechanics when the local queue overflows
- Queue size limits and worker behavior (1 worker with queue size of 8)
- The order in which tasks are executed based on queue management

**Key concepts:** Work stealing, task queue management, worker scheduling

### safety_task.rs

Shows how to use Kyron's safety-critical features, including:

- Spawning safety-critical tasks with `safety::spawn()`
- Using dedicated workers with `spawn_on_dedicated()`
- Handling task failures in safety-critical contexts
- Configuring the safety worker via `RuntimeBuilder`
- Task context introspection (`TaskContext::worker_id()`, `TaskContext::task_id()`)

**Key concepts:** Safety-critical tasks, dedicated workers, task context, failure handling

### logging.rs

Demontrates how to use the Kyron's logging framework (liblogging_tracing), which supports logging through tracing/log/score-log loggers.

The example application can be built as folows,

`bazel build --config=x86_64-linux --//src/flags:logging_feature=<feature> //examples:logging`

where `<feature>` is tracing/log/score-log. The default feature is `tracing`.

For example with score-log, copy the `../src/kyron/examples/config/logging.json` to where executable is present and then execute `logging` application.
Alternatively, the configuration file path can be specified in the source code using `config_path()` function of `LogAndTraceBuilder`.

**Key concepts:** Logging using Kyron's logging framework

## Runtime Configuration

Most examples demonstrate different runtime configurations:

- **Workers:** Number of worker threads for task execution
- **Task queue size:** Size of the per-worker task queue
- **Dedicated workers:** Named workers for specific tasks
- **Safety worker:** Special worker for safety-critical tasks
- **Thread parameters:** Configuration for worker threads

Example runtime setup:

```rust
let (builder, _engine_id) = kyron::runtime::RuntimeBuilder::new().with_engine(
    ExecutionEngineBuilder::new()
        .task_queue_size(256)
        .workers(3)
        .with_dedicated_worker("dedicated".into(), ThreadParameters::default())
        .enable_safety_worker(ThreadParameters::default()),
);
let mut runtime = builder.build().unwrap();
```

## Logging

All examples use `tracing_subscriber` for structured logging. The log output shows:

- Thread IDs and names
- Task execution flow
- Worker assignment
- Runtime events

Adjust the log level by modifying `with_max_level()` in the example code (e.g., `Level::DEBUG`, `Level::INFO`, `Level::TRACE`).
