use crate::internals::execution_barrier::RuntimeJoiner;
use crate::internals::runtime_helper::Runtime;
use kyron::core::types::UniqueWorkerId;
use kyron::futures::reusable_box_future::ReusableBoxFuturePool;
use kyron::{core::types::box_future, *};
use kyron_foundation::prelude::CommonErrors;
use test_scenarios_rust::scenario::{Scenario, ScenarioGroup, ScenarioGroupImpl};

use serde::Deserialize;
use serde_json::Value;
use tracing::info;

#[derive(Deserialize, Debug)]
struct TestInput {
    task_count: usize,
}

impl TestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

async fn just_log_task(name: String) {
    info!(name);
}
pub struct Spawn;

impl Scenario for Spawn {
    fn name(&self) -> &str {
        "spawn"
    }

    ///
    /// Spawns logging tasks with spawn method
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let mut joiner = RuntimeJoiner::new();
        rt.block_on(async move {
            for task_ndx in 0..logic.task_count {
                joiner.add_handle(spawn(just_log_task(format!("task_{task_ndx}"))));
            }

            joiner.wait_for_all().await;
        });

        Ok(())
    }
}

pub struct SpawnFromBoxed;

impl Scenario for SpawnFromBoxed {
    fn name(&self) -> &str {
        "spawn_from_boxed"
    }

    ///
    /// Spawns logging tasks with spawn_from_boxed method
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let mut joiner = RuntimeJoiner::new();
        rt.block_on(async move {
            for task_ndx in 0..logic.task_count {
                joiner.add_handle(spawn_from_boxed(box_future(just_log_task(format!("task_{task_ndx}")))));
            }

            joiner.wait_for_all().await;
        });

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct ReusableTestInput {
    task_count: usize,
    pool_size: usize,
    create_method: String,
}

impl ReusableTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

fn create_pool_for_type<T, U>(pool_size: usize, _: U) -> ReusableBoxFuturePool<T>
where
    U: ::core::future::Future<Output = T> + Send + 'static,
{
    ReusableBoxFuturePool::for_type::<U>(pool_size)
}

fn create_pool(logic: &ReusableTestInput) -> ReusableBoxFuturePool<()> {
    match logic.create_method.as_str() {
        "for_type" => {
            let f = just_log_task("".into());
            create_pool_for_type(logic.pool_size, f)
        }
        "for_value" => ReusableBoxFuturePool::for_value(logic.pool_size, just_log_task("".into())),
        _ => {
            panic!("Unknown create_method field: {}", logic.create_method);
        }
    }
}

fn trace_pool_next_error(error: &CommonErrors) {
    info!(
        name = "error",
        value = format!("{:?}", error),
        message = format!("Pool next failed with error: {:?}", error)
    );
}

pub struct SpawnFromReusable;

impl Scenario for SpawnFromReusable {
    fn name(&self) -> &str {
        "spawn_from_reusable"
    }

    ///
    /// Spawns logging tasks with spawn_from_reusable method
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = ReusableTestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let mut joiner = RuntimeJoiner::new();
        rt.block_on(async move {
            let mut pool = create_pool(&logic);

            // Pool items should be prepared upfront to block reusage
            let mut pool_items = Vec::new();
            for task_ndx in 0..logic.task_count {
                let res = pool.next(just_log_task(format!("task_{task_ndx}")));
                match res {
                    Ok(pool_item) => pool_items.push(pool_item),
                    Err(error) => {
                        trace_pool_next_error(&error);
                    }
                }
            }

            for pool_item in pool_items {
                joiner.add_handle(spawn_from_reusable(pool_item));
            }

            joiner.wait_for_all().await;
        });

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct ReusableReuseTestInput {
    task_count: usize,
    pool_size: usize,
    iterations: usize,
}

impl ReusableReuseTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

pub struct SpawnFromReusableReuse;

impl Scenario for SpawnFromReusableReuse {
    fn name(&self) -> &str {
        "spawn_from_reusable_reuse"
    }

    ///
    /// Spawns logging tasks with spawn_from_reusable method and reuses pool items
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = ReusableReuseTestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            let mut pool = ReusableBoxFuturePool::for_value(logic.pool_size, just_log_task("".into()));

            for _ in 0..logic.iterations {
                let mut pool_items = Vec::new();
                let mut joiner = RuntimeJoiner::new();

                for task_ndx in 0..logic.task_count {
                    let res = pool.next(just_log_task(format!("task_{task_ndx}")));
                    match res {
                        Ok(pool_item) => {
                            pool_items.push(pool_item);
                        }
                        Err(error) => {
                            trace_pool_next_error(&error);
                        }
                    }
                }

                for pool_item in pool_items {
                    joiner.add_handle(spawn_from_reusable(pool_item));
                }

                joiner.wait_for_all().await;
            }
        });

        Ok(())
    }
}

pub struct SpawnOnDedicated;

impl Scenario for SpawnOnDedicated {
    fn name(&self) -> &str {
        "spawn_on_dedicated"
    }

    ///
    /// Spawns logging tasks with spawn_on_dedicated method
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let mut rt = builder.build();

        let mut joiner = RuntimeJoiner::new();
        rt.block_on(async move {
            // Spawn a regular task for thread id comparison
            spawn(just_log_task("regular_task".into())).await.expect("spawn() failed unexpectedly");

            let unique_worker_id = UniqueWorkerId::from(dedicated_workers[0].id.as_str());
            for task_ndx in 0..logic.task_count {
                joiner.add_handle(spawn_on_dedicated(just_log_task(format!("task_{task_ndx}")), unique_worker_id));
            }

            joiner.wait_for_all().await;
        });

        Ok(())
    }
}

pub struct SpawnFromBoxedOnDedicated;

impl Scenario for SpawnFromBoxedOnDedicated {
    fn name(&self) -> &str {
        "spawn_from_boxed_on_dedicated"
    }

    ///
    /// Spawns logging tasks with spawn_from_boxed_on_dedicated method
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let mut rt = builder.build();

        let mut joiner = RuntimeJoiner::new();
        rt.block_on(async move {
            // Spawn a regular task for thread id comparison
            spawn(just_log_task("regular_task".into())).await.expect("spawn() failed unexpectedly");

            let unique_worker_id = UniqueWorkerId::from(dedicated_workers[0].id.as_str());
            for task_ndx in 0..logic.task_count {
                joiner.add_handle(spawn_from_boxed_on_dedicated(
                    box_future(just_log_task(format!("task_{task_ndx}"))),
                    unique_worker_id,
                ));
            }

            joiner.wait_for_all().await;
        });

        Ok(())
    }
}

struct SpawnFromBoxedOnDedicatedInvalidWorker;

impl Scenario for SpawnFromBoxedOnDedicatedInvalidWorker {
    fn name(&self) -> &str {
        "spawn_from_boxed_on_dedicated_invalid_worker"
    }

    ///
    /// Checks if spawn_from_boxed_on_dedicated with invalid worker fails
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            let invalid_worker_id = UniqueWorkerId::from("invalid_worker");
            let _ = spawn_from_boxed_on_dedicated(box_future(just_log_task("dedicated_task".into())), invalid_worker_id).await;
        });

        Ok(())
    }
}
pub struct SpawnFromReusableOnDedicated;

impl Scenario for SpawnFromReusableOnDedicated {
    fn name(&self) -> &str {
        "spawn_from_reusable_on_dedicated"
    }

    ///
    /// Spawns logging tasks with spawn_from_reusable_on_dedicated method
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = ReusableTestInput::new(input);
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let mut rt = builder.build();

        let mut joiner = RuntimeJoiner::new();
        rt.block_on(async move {
            let mut pool = create_pool(&logic);
            let mut pool_items = Vec::new();

            let unique_worker_id = UniqueWorkerId::from(dedicated_workers[0].id.as_str());
            for task_ndx in 0..logic.task_count {
                let res = pool.next(just_log_task(format!("task_{task_ndx}")));
                match res {
                    Ok(pool_item) => {
                        pool_items.push(pool_item);
                    }
                    Err(error) => {
                        trace_pool_next_error(&error);
                    }
                }
            }

            // Spawn a regular task for thread id comparison
            spawn(just_log_task("regular_task".into())).await.expect("spawn() failed unexpectedly");
            for pool_item in pool_items {
                joiner.add_handle(spawn_from_reusable_on_dedicated(pool_item, unique_worker_id));
            }

            joiner.wait_for_all().await;
        });

        Ok(())
    }
}

pub struct SpawnFromReusableOnDedicatedReuse;

impl Scenario for SpawnFromReusableOnDedicatedReuse {
    fn name(&self) -> &str {
        "spawn_from_reusable_on_dedicated_reuse"
    }

    ///
    /// Spawns logging tasks with spawn_from_reusable_on_dedicated method and reuses pool items
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = ReusableReuseTestInput::new(input);
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let mut rt = builder.build();

        rt.block_on(async move {
            let mut pool = ReusableBoxFuturePool::for_value(logic.pool_size, just_log_task("".into()));

            for _ in 0..logic.iterations {
                let mut pool_items = Vec::new();
                let mut joiner = RuntimeJoiner::new();

                let unique_worker_id = UniqueWorkerId::from(dedicated_workers[0].id.as_str());
                for task_ndx in 0..logic.task_count {
                    let res = pool.next(just_log_task(format!("task_{task_ndx}")));
                    match res {
                        Ok(pool_item) => {
                            pool_items.push(pool_item);
                        }
                        Err(error) => {
                            trace_pool_next_error(&error);
                        }
                    }
                }

                // Spawn a regular task for thread id comparison
                spawn(just_log_task("regular_task".into())).await.expect("spawn() failed unexpectedly");
                for pool_item in pool_items {
                    joiner.add_handle(spawn_from_reusable_on_dedicated(pool_item, unique_worker_id));
                }

                joiner.wait_for_all().await;
            }
        });

        Ok(())
    }
}

struct SpawnFromReusableOnDedicatedInvalidWorker;

impl Scenario for SpawnFromReusableOnDedicatedInvalidWorker {
    fn name(&self) -> &str {
        "spawn_from_reusable_on_dedicated_invalid_worker"
    }

    ///
    /// Checks if spawn_from_reusable_on_dedicated with invalid worker fails
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = ReusableTestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            let mut pool = ReusableBoxFuturePool::for_value(logic.pool_size, just_log_task("".into()));
            let pool_item = pool.next(just_log_task("dedicated_task".into())).expect("Failed to get pool item");
            let invalid_worker_id = UniqueWorkerId::from("invalid_worker");

            let _ = spawn_from_reusable_on_dedicated(pool_item, invalid_worker_id).await;
        });

        Ok(())
    }
}

pub fn spawn_methods_group() -> Box<dyn ScenarioGroup> {
    Box::new(ScenarioGroupImpl::new(
        "spawn_methods",
        vec![
            Box::new(Spawn),
            Box::new(SpawnFromBoxed),
            Box::new(SpawnFromReusable),
            Box::new(SpawnFromReusableReuse),
            Box::new(SpawnOnDedicated),
            Box::new(SpawnFromBoxedOnDedicated),
            Box::new(SpawnFromBoxedOnDedicatedInvalidWorker),
            Box::new(SpawnFromReusableOnDedicated),
            Box::new(SpawnFromReusableOnDedicatedReuse),
            Box::new(SpawnFromReusableOnDedicatedInvalidWorker),
        ],
        vec![],
    ))
}
