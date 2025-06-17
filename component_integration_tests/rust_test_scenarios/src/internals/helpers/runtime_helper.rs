use async_runtime::runtime::async_runtime::{AsyncRuntime, AsyncRuntimeBuilder};
use async_runtime::scheduler::execution_engine::ExecutionEngineBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

#[derive(Serialize, Deserialize, Debug)]
pub struct Runtime {
    task_queue_size: usize,
    workers: usize,
}

impl Runtime {
    pub fn new(inputs: &Option<String>) -> Self {
        let v: Value = serde_json::from_str(inputs.as_deref().unwrap()).unwrap();
        serde_json::from_value(v["runtime"].clone()).unwrap()
    }

    pub fn build(&self) -> AsyncRuntime {
        debug!(
            "Creating AsyncRuntime with {} queue size and {} workers",
            self.task_queue_size, self.workers
        );
        let (builder, _engine_id) =
            AsyncRuntimeBuilder::new().with_engine(ExecutionEngineBuilder::new().task_queue_size(self.task_queue_size).workers(self.workers));
        builder.build().unwrap()
    }
}
