use crate::internals::execution_barrier::RuntimeJoiner;
use crate::internals::runtime_helper::Runtime;
use async_runtime::runtime::async_runtime::RuntimeErrors;
use async_runtime::spawn;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::string::String;
use test_scenarios_rust::scenario::Scenario;
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
struct ExecEngineTasks {
    engine_id: usize,
    task_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct TestInput {
    tasks: Vec<ExecEngineTasks>,
}

impl TestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        let tasks = serde_json::from_value(v["tasks"].clone()).expect("Failed to parse \"tasks\" field");
        Self { tasks }
    }
}

async fn basic_task(engine_id: usize, task_id: String) {
    info!(engine_id = engine_id, task_id = task_id);
}

fn rt_errors_to_string(e: RuntimeErrors) -> String {
    format!("RuntimeErrors::{e:?}")
}

pub struct SingleRtMultipleExecEngine;

impl Scenario for SingleRtMultipleExecEngine {
    fn name(&self) -> &str {
        "single_rt_multiple_exec_engine"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        for tasks_data in logic.tasks.iter() {
            let engine_id = tasks_data.engine_id;
            let task_ids = tasks_data.task_ids.clone();
            let spawn_result = rt.spawn_in_engine(engine_id, async move {
                let mut joiner = RuntimeJoiner::new();
                for task_id in task_ids {
                    joiner.add_handle(spawn(basic_task(engine_id, task_id)));
                }
                Ok(joiner.wait_for_all().await)
            });

            if let Err(e) = spawn_result {
                return Err(rt_errors_to_string(e));
            }
        }

        for tasks_data in logic.tasks.iter() {
            let engine_id = tasks_data.engine_id;
            if let Err(e) = rt.wait_for_engine(engine_id) {
                return Err(rt_errors_to_string(e));
            }
        }

        Ok(())
    }
}
