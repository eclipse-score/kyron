use async_runtime::runtime::async_runtime::{AsyncRuntime, AsyncRuntimeBuilder};
use async_runtime::scheduler::execution_engine::ExecutionEngineBuilder;
use async_runtime::scheduler::SchedulerType;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use tracing::debug;

/// Execution engine configuration.
#[derive(Deserialize, Debug)]
pub struct ExecEngineConfig {
    pub task_queue_size: u32,
    pub workers: usize,
    pub thread_priority: Option<u8>,
    pub thread_affinity: Option<Vec<usize>>,
    pub thread_stack_size: Option<u64>,
    pub thread_scheduler: Option<String>,
}

fn deserialize_exec_engines<'de, D>(deserializer: D) -> Result<Vec<ExecEngineConfig>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RuntimeConfig {
        Object(ExecEngineConfig),
        Array(Vec<ExecEngineConfig>),
    }

    let exec_engines = match RuntimeConfig::deserialize(deserializer)? {
        RuntimeConfig::Object(exec_engine_config) => vec![exec_engine_config],
        RuntimeConfig::Array(exec_engine_configs) => exec_engine_configs,
    };
    Ok(exec_engines)
}

#[derive(Deserialize, Debug)]
#[serde(transparent)]
pub struct Runtime {
    #[serde(deserialize_with = "deserialize_exec_engines")]
    exec_engines: Vec<ExecEngineConfig>,
}

impl Runtime {
    /// Parse `Runtime` from JSON string.
    /// JSON is expected to contain `runtime` field.
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json_str).map_err(|e| e.to_string())?;
        serde_json::from_value(v["runtime"].clone()).map_err(|e| e.to_string())
    }

    pub fn exec_engines(&self) -> &Vec<ExecEngineConfig> {
        &self.exec_engines
    }

    pub fn build(&self) -> AsyncRuntime {
        debug!("Creating AsyncRuntime with {} execution engines", self.exec_engines.len());

        let mut async_rt_builder = AsyncRuntimeBuilder::new();
        for exec_engine in self.exec_engines.as_slice() {
            debug!("Creating ExecutionEngine with: {:?}", exec_engine);

            let mut exec_engine_builder = ExecutionEngineBuilder::new()
                .task_queue_size(exec_engine.task_queue_size)
                .workers(exec_engine.workers);
            if let Some(thread_priority) = exec_engine.thread_priority {
                exec_engine_builder = exec_engine_builder.thread_priority(thread_priority);
            }
            if let Some(thread_affinity) = &exec_engine.thread_affinity {
                exec_engine_builder = exec_engine_builder.thread_affinity(thread_affinity);
            }
            if let Some(thread_stack_size) = exec_engine.thread_stack_size {
                exec_engine_builder = exec_engine_builder.thread_stack_size(thread_stack_size);
            }
            if let Some(thread_scheduler) = &exec_engine.thread_scheduler {
                let thread_scheduler_type = match thread_scheduler.as_str() {
                    "fifo" => SchedulerType::Fifo,
                    "round_robin" => SchedulerType::RoundRobin,
                    "other" => SchedulerType::Other,
                    _ => panic!("Unknown scheduler type"),
                };
                exec_engine_builder = exec_engine_builder.thread_scheduler(thread_scheduler_type);
            }

            let (builder, _) = async_rt_builder.with_engine(exec_engine_builder);
            async_rt_builder = builder;
        }

        async_rt_builder.build().expect("Failed to build async runtime")
    }
}
