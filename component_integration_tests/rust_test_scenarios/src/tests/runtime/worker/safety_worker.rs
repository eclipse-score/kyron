use crate::internals::execution_barrier::RuntimeJoiner;
use crate::internals::runtime_helper::Runtime;
use crate::internals::thread_params::{current_thread_priority_params, ThreadPriorityParams};
use kyron::core::types::UniqueWorkerId;
use kyron::futures::reusable_box_future::ReusableBoxFuturePool;
use kyron::{safety, spawn};
use serde::Deserialize;
use serde_json::Value;
use std::cmp::max;
use test_scenarios_rust::scenario::{Scenario, ScenarioGroup, ScenarioGroupImpl};
use tracing::info;

struct EnsureSafetyEnabled;

impl Scenario for EnsureSafetyEnabled {
    fn name(&self) -> &str {
        "ensure_safety_enabled"
    }

    ///
    /// Checks if safety::ensure_safety_enabled works as intended
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();
            info!("Running!");
        });

        Ok(())
    }
}

struct EnsureSafetyEnabledOutisdeAsyncContext;

impl Scenario for EnsureSafetyEnabledOutisdeAsyncContext {
    fn name(&self) -> &str {
        "ensure_safety_enabled_outside_async_context"
    }

    ///
    /// Checks if safety::ensure_safety_enabled fails when called outside of async context
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();
        safety::ensure_safety_enabled();

        rt.block_on(async move {
            info!("Running!");
        });

        Ok(())
    }
}

async fn failing_task() -> Result<(), String> {
    info!(name = "failing_task");
    Err("Intentional failure".to_string())
}
struct SafetyWorkerFailedTaskHandling;

impl Scenario for SafetyWorkerFailedTaskHandling {
    fn name(&self) -> &str {
        "task_handling"
    }

    ///
    /// Checks if safely spawned task is handled by safety worker when it fails
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();

            info!(name = "main_begin");
            let res = safety::spawn(failing_task()).await.expect("Failed to get task result");
            info!(name = "main_end", is_error = res.is_err(), safe_task_handler = true);
        });

        Ok(())
    }
}

async fn outer_task() -> Result<(), String> {
    info!(name = "outer_task_begin");
    let res = safety::spawn(failing_task()).await.expect("Failed to get task result");
    info!(name = "outer_task_end", is_error = res.is_err(), safe_task_handler = true);
    res
}

struct SafetyWorkerNestedFailedTaskHandling;

impl Scenario for SafetyWorkerNestedFailedTaskHandling {
    fn name(&self) -> &str {
        "nested_task_handling"
    }

    ///
    /// Checks if nested safely spawned task is handled by safety worker when it fails
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();

            info!(name = "main_begin");
            let res = spawn(outer_task()).await.expect("Failed to get task result");
            info!(name = "main_end", error = res.is_err());
        });

        Ok(())
    }
}

struct SafeApiWithoutSafetyWorker;

impl Scenario for SafeApiWithoutSafetyWorker {
    fn name(&self) -> &str {
        "safe_api_without_safety_worker"
    }

    ///
    /// Checks if safely spawned task is ran as regular when safety worker is disabled
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let rt_helper = Runtime::from_json(input)?;
        let exec_engine = rt_helper.exec_engines().first().expect("No execution engine found");
        if exec_engine.safety_worker.is_some() {
            return Err("This scenario requires disabled safety worker".to_string());
        }
        let mut rt = rt_helper.build();

        rt.block_on(async move {
            info!(name = "main_begin");
            let res = safety::spawn(failing_task()).await.expect("Failed to get task result");
            info!(name = "main_end", is_error = res.is_err(), safe_task_handler = true);
        });

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct QueueTasksTestInput {
    task_count: usize,
}

impl QueueTasksTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

struct QueueSafetyWorkerTasks;

impl Scenario for QueueSafetyWorkerTasks {
    fn name(&self) -> &str {
        "queue_safety_worker_tasks"
    }

    ///
    /// Checks if queuing multiple safe task errors is safe.
    /// Note: That is not intended use of the feature, but enables to queue multiple safety worker tasks
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();
        let logic = QueueTasksTestInput::new(input);

        rt.block_on(async move {
            safety::ensure_safety_enabled();
            let mut joiner = RuntimeJoiner::new();

            info!(name = "main_begin");
            for _ in 0..logic.task_count {
                joiner.add_handle(safety::spawn(failing_task()));
            }
            joiner.wait_for_all().await;
            info!(name = "main_end", is_error = true, safe_task_handler = true);
        });

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct HeavyLoadTestInput {
    successful_tasks: usize,
    failing_tasks: usize,
}

impl HeavyLoadTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

async fn successful_task() -> Result<(), String> {
    info!(name = "successful_task");
    Ok(())
}

async fn nesting_safe_successful_task() -> Result<(), String> {
    info!(name = "nesting_successful_begin");
    let res = safety::spawn(successful_task()).await.expect("Failed to get task result");
    info!(name = "nesting_successful_end", is_error = res.is_err(), safe_task_handler = true);
    res
}

async fn nesting_safe_failing_task() -> Result<(), String> {
    info!(name = "nesting_failing_begin");
    let res = safety::spawn(failing_task()).await.expect("Failed to get task result");
    info!(name = "nesting_failing_end", is_error = res.is_err(), safe_task_handler = true);
    res
}

struct SafetyWorkerHeavyLoad;

impl Scenario for SafetyWorkerHeavyLoad {
    fn name(&self) -> &str {
        "heavy_load"
    }

    ///
    /// Checks if many passing and failing tasks are handled correctly by safety worker
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();
        let logic = HeavyLoadTestInput::new(input);

        rt.block_on(async move {
            safety::ensure_safety_enabled();
            let mut joiner = RuntimeJoiner::new();

            info!(name = "main_begin");

            let max_task_count = max(logic.successful_tasks, logic.failing_tasks);
            for task_id in 0..max_task_count {
                if task_id < logic.successful_tasks {
                    joiner.add_handle(spawn(nesting_safe_successful_task()));
                }

                if task_id < logic.failing_tasks {
                    joiner.add_handle(spawn(nesting_safe_failing_task()));
                }
            }
            joiner.wait_for_all().await;
            info!(name = "main_end");
        });

        Ok(())
    }
}

fn print_id_and_params(id: &str) {
    let ThreadPriorityParams {
        scheduler,
        priority,
        priority_min,
        priority_max,
    } = current_thread_priority_params();
    info!(id, scheduler, priority, priority_min, priority_max);
}
struct SafetyWorkerThreadParameters;

impl Scenario for SafetyWorkerThreadParameters {
    fn name(&self) -> &str {
        "thread_parameters"
    }

    ///
    /// Checks if thread parameters for safety worker are set correctly
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();
        print_id_and_params("main");

        rt.block_on(async move {
            safety::ensure_safety_enabled();

            info!(name = "main_begin");
            let res = safety::spawn(failing_task()).await.expect("Failed to get task result");
            info!(name = "main_end", is_error = res.is_err(), safe_task_handler = true);
            print_id_and_params("safety_worker");
        });

        Ok(())
    }
}

struct SafetyWorkerSpawnFromBoxed;

impl Scenario for SafetyWorkerSpawnFromBoxed {
    fn name(&self) -> &str {
        "spawn_from_boxed"
    }

    ///
    /// Checks if safety::spawn_from_boxed works as intended
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();

            info!(name = "main_begin");
            let res = safety::spawn_from_boxed(Box::pin(failing_task()))
                .await
                .expect("Failed to get task result");
            info!(name = "main_end", is_error = res.is_err(), safe_task_handler = true);
        });

        Ok(())
    }
}

struct SafetyWorkerSpawnFromReusable;

impl Scenario for SafetyWorkerSpawnFromReusable {
    fn name(&self) -> &str {
        "spawn_from_reusable"
    }

    ///
    /// Checks if safety::spawn_from_reusable works as intended
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();

            let mut pool = ReusableBoxFuturePool::for_value(1, failing_task());
            let pool_item = pool.next(failing_task()).expect("Failed to get pool item");

            info!(name = "main_begin");
            let res = safety::spawn_from_reusable(pool_item).await.expect("Failed to get task result");
            info!(name = "main_end", is_error = res.is_err(), safe_task_handler = true);
        });

        Ok(())
    }
}

struct SafetyWorkerSpawnOnDedicated;

impl Scenario for SafetyWorkerSpawnOnDedicated {
    fn name(&self) -> &str {
        "spawn_on_dedicated"
    }

    ///
    /// Checks if safety::spawn_on_dedicated works as intended
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let mut rt = builder.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();
            let unique_worker_id = UniqueWorkerId::from(dedicated_workers[0].id.as_str());

            info!(name = "main_begin");
            let _ = spawn(failing_task()).await;
            let res = safety::spawn_on_dedicated(failing_task(), unique_worker_id)
                .await
                .expect("Failed to get task result");
            info!(name = "main_end", is_error = res.is_err(), safe_task_handler = true);
        });

        Ok(())
    }
}

struct SafetyWorkerSpawnFromBoxedOnDedicated;

impl Scenario for SafetyWorkerSpawnFromBoxedOnDedicated {
    fn name(&self) -> &str {
        "spawn_from_boxed_on_dedicated"
    }

    ///
    /// Checks if safety::spawn_from_boxed_on_dedicated works as intended
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let mut rt = builder.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();
            let unique_worker_id = UniqueWorkerId::from(dedicated_workers[0].id.as_str());

            info!(name = "main_begin");
            let _ = spawn(failing_task()).await;
            let res = safety::spawn_from_boxed_on_dedicated(Box::pin(failing_task()), unique_worker_id)
                .await
                .expect("Failed to get task result");
            info!(name = "main_end", is_error = res.is_err(), safe_task_handler = true);
        });

        Ok(())
    }
}

struct SafetyWorkerSpawnFromReusableOnDedicated;

impl Scenario for SafetyWorkerSpawnFromReusableOnDedicated {
    fn name(&self) -> &str {
        "spawn_from_reusable_on_dedicated"
    }

    ///
    /// Checks if safety::spawn_from_reusable_on_dedicated works as intended
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let mut rt = builder.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();
            let unique_worker_id = UniqueWorkerId::from(dedicated_workers[0].id.as_str());

            let mut pool = ReusableBoxFuturePool::for_value(1, failing_task());
            let pool_item = pool.next(failing_task()).expect("Failed to get pool item");

            info!(name = "main_begin");
            let _ = spawn(failing_task()).await;
            let res = safety::spawn_from_reusable_on_dedicated(pool_item, unique_worker_id)
                .await
                .expect("Failed to get task result");
            info!(name = "main_end", is_error = res.is_err(), safe_task_handler = true);
        });

        Ok(())
    }
}

struct SafetyWorkerSpawnOnDedicatedUnregisteredWorker;

impl Scenario for SafetyWorkerSpawnOnDedicatedUnregisteredWorker {
    fn name(&self) -> &str {
        "spawn_on_dedicated_unregistered_worker"
    }

    ///
    /// Checks if safety::spawn_on_dedicated with unregistered worker fails
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();

            let unregistered_worker_id = UniqueWorkerId::from("unregistered_worker");
            let _ = safety::spawn_on_dedicated(failing_task(), unregistered_worker_id).await;
        });

        Ok(())
    }
}

struct SafetyWorkerSpawnFromBoxedOnDedicatedUnregisteredWorker;

impl Scenario for SafetyWorkerSpawnFromBoxedOnDedicatedUnregisteredWorker {
    fn name(&self) -> &str {
        "spawn_from_boxed_on_dedicated_unregistered_worker"
    }

    ///
    /// Checks if safety::spawn_from_boxed_on_dedicated with unregistered worker fails
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();

            let unregistered_worker_id = UniqueWorkerId::from("unregistered_worker");
            let _ = safety::spawn_from_boxed_on_dedicated(Box::pin(failing_task()), unregistered_worker_id)
                .await
                .expect("Failed to get task result");
        });

        Ok(())
    }
}

struct SafetyWorkerSpawnFromReusableOnDedicatedUnregisteredWorker;

impl Scenario for SafetyWorkerSpawnFromReusableOnDedicatedUnregisteredWorker {
    fn name(&self) -> &str {
        "spawn_from_reusable_on_dedicated_unregistered_worker"
    }

    ///
    /// Checks if safety::spawn_from_reusable_on_dedicated with unregistered worker fails
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            safety::ensure_safety_enabled();

            let unregistered_worker_id = UniqueWorkerId::from("unregistered_worker");
            let mut pool = ReusableBoxFuturePool::for_value(1, failing_task());
            let pool_item = pool.next(failing_task()).expect("Failed to get pool item");

            let _ = safety::spawn_from_reusable_on_dedicated(pool_item, unregistered_worker_id)
                .await
                .expect("Failed to get task result");
        });

        Ok(())
    }
}

pub fn safety_worker_group() -> Box<dyn ScenarioGroup> {
    Box::new(ScenarioGroupImpl::new(
        "safety_worker",
        vec![
            Box::new(EnsureSafetyEnabled),
            Box::new(EnsureSafetyEnabledOutisdeAsyncContext),
            Box::new(SafetyWorkerFailedTaskHandling),
            Box::new(SafetyWorkerNestedFailedTaskHandling),
            Box::new(SafeApiWithoutSafetyWorker),
            Box::new(QueueSafetyWorkerTasks),
            Box::new(SafetyWorkerHeavyLoad),
            Box::new(SafetyWorkerThreadParameters),
            Box::new(SafetyWorkerSpawnFromBoxed),
            Box::new(SafetyWorkerSpawnFromReusable),
            Box::new(SafetyWorkerSpawnOnDedicated),
            Box::new(SafetyWorkerSpawnFromBoxedOnDedicated),
            Box::new(SafetyWorkerSpawnFromReusableOnDedicated),
            Box::new(SafetyWorkerSpawnOnDedicatedUnregisteredWorker),
            Box::new(SafetyWorkerSpawnFromBoxedOnDedicatedUnregisteredWorker),
            Box::new(SafetyWorkerSpawnFromReusableOnDedicatedUnregisteredWorker),
        ],
        vec![],
    ))
}
