use crate::internals::execution_barrier::RuntimeJoiner;
use crate::internals::runtime_helper::Runtime;
use test_scenarios_rust::scenario::Scenario;

use async_runtime::spawn;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
struct TestInput {
    tasks: Vec<String>,
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

async fn simple_task(name: String) {
    checkpoint(name.as_str());
}

pub struct BasicWorker;

impl Scenario for BasicWorker {
    fn name(&self) -> &str {
        "basic"
    }

    ///
    /// Spawns just logging tasks
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            for name in logic.tasks.as_slice() {
                joiner.add_handle(spawn(simple_task(name.to_string())));
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}
