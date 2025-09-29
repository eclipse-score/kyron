use crate::internals::execution_barrier::RuntimeJoiner;
use crate::internals::runtime_helper::Runtime;
use test_scenarios_rust::scenario::Scenario;

use async_runtime::futures::sleep;
use async_runtime::spawn;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::thread;
use std::time::Duration;
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
struct NameWithDelayInput {
    id: String,
    delay_ms: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct TestInput {
    non_blocking_sleep_tasks: Vec<NameWithDelayInput>,
    blocking_sleep_tasks: Vec<NameWithDelayInput>,
    non_sleep_tasks: Vec<String>,
}

impl TestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

fn checkpoint(id: &str) {
    info!(id = id);
}

fn location_checkpoint(id: &str, location: &str) {
    info!(id = id, location = location);
}

async fn non_blocking_sleep_task(name: String, delay_ms: u64) {
    location_checkpoint(name.as_str(), "begin");
    sleep::sleep(Duration::from_millis(delay_ms)).await;
    location_checkpoint(name.as_str(), "end");
}

async fn blocking_sleep_task(name: String, delay_ms: u64) {
    location_checkpoint(name.as_str(), "begin");
    thread::sleep(Duration::from_millis(delay_ms));
    location_checkpoint(name.as_str(), "end");
}

async fn non_sleep_task(name: String) {
    checkpoint(name.as_str());
}

pub struct SleepBasic;

impl Scenario for SleepBasic {
    fn name(&self) -> &str {
        "basic"
    }

    ///
    /// Start non-blocking sleep tasks, blocking sleep tasks, and non-sleep tasks in given order.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            for task in logic.non_blocking_sleep_tasks.as_slice() {
                joiner.add_handle(spawn(non_blocking_sleep_task(task.id.to_string(), task.delay_ms)));
            }

            for task in logic.blocking_sleep_tasks.as_slice() {
                joiner.add_handle(spawn(blocking_sleep_task(task.id.to_string(), task.delay_ms)));
            }

            for name in logic.non_sleep_tasks.as_slice() {
                joiner.add_handle(spawn(non_sleep_task(name.to_string())));
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}
