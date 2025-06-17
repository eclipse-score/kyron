use crate::internals::helpers::runtime_helper::Runtime;
use crate::internals::test_case::TestCase;

use async_runtime::spawn;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
struct TestInput {
    tasks: Vec<String>,
}

impl TestInput {
    pub fn new(inputs: &Option<String>) -> Self {
        let v: Value = serde_json::from_str(inputs.as_deref().unwrap()).unwrap();
        serde_json::from_value(v["test"].clone()).unwrap()
    }
}

fn checkpoint(id: &str) {
    info!(id = id);
}

async fn simple_task(name: String) {
    checkpoint(name.as_str());
}

pub struct BasicWorkerTest;

impl TestCase for BasicWorkerTest {
    fn get_name(&self) -> &'static str {
        "worker"
    }

    ///
    /// Spawns just logging tasks
    ///
    fn run(&self, input: Option<String>) -> Result<(), String> {
        let logic = TestInput::new(&input);
        let mut rt = Runtime::new(&input).build();

        let _ = rt
            .block_on(async move {
                for name in logic.tasks.as_slice() {
                    spawn(simple_task(name.to_string()));
                }

                Ok(0)
            })
            .unwrap();

        Ok(())
    }
}
