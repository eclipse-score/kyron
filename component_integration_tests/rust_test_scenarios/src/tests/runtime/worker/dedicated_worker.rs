use crate::internals::execution_barrier::{MultiExecutionBarrier, RuntimeJoiner};
use crate::internals::runtime_helper::{DedicatedWorkerConfig, Runtime};
use crate::internals::thread_params::{current_thread_affinity, current_thread_priority_params, ThreadPriorityParams};
use async_runtime::core::types::UniqueWorkerId;
use async_runtime::runtime::async_runtime::AsyncRuntime;
use async_runtime::{spawn, spawn_on_dedicated};
use foundation::threading::thread_wait_barrier::ThreadReadyNotifier;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use test_scenarios_rust::scenario::{Scenario, ScenarioGroup, ScenarioGroupImpl};
use tracing::info;

/// Allow tasks to finish and disable tracing.
fn unblock_tasks(block_condition: Arc<(Condvar, Mutex<bool>)>) {
    let (cv, mtx) = &*block_condition;
    let mut block = mtx.lock().expect("Unable to lock mutex");
    *block = false;
    cv.notify_all();
}

/// Provide result string to be used with `info!`.
fn wait_result_str(wait_result: Result<(), String>, wait_time: &Duration) -> &'static str {
    match wait_result {
        Ok(_) => "ok",
        Err(e) => {
            let expected_message = format!("Failed to join tasks after {} seconds", wait_time.as_secs());
            if e == expected_message {
                "timeout"
            } else {
                "other"
            }
        }
    }
}

/// Basic ID print.
fn print_id(id: &str) {
    info!(id);
}

async fn blocking_task(id: String, block_condition: Arc<(Condvar, Mutex<bool>)>, notifier: ThreadReadyNotifier, info_fn: fn(&str)) {
    let (cv, mtx) = &*block_condition;
    let mut block = mtx.lock().expect("Unable to lock mutex");

    // Print requested messages.
    info_fn(&id);

    // Notify task done.
    notifier.ready();

    // Wait until barrier timed out.
    while *block {
        block = cv.wait(block).expect("Unable to wait - poisoned mutex?");
    }
}

/// Common run implementation.
fn common_run(mut rt: AsyncRuntime, dedicated_workers: Vec<DedicatedWorkerConfig>, info_fn: fn(&str)) {
    rt.block_on(async move {
        let mut joiner = RuntimeJoiner::new();
        let mid_barrier = MultiExecutionBarrier::new(dedicated_workers.len());
        let mut mid_notifiers = mid_barrier.get_notifiers();

        // Condition variable and mutex containing `block` variable.
        let block_condition = Arc::new((Condvar::new(), Mutex::new(true)));

        // Spawn tasks.
        for dedicated_worker in dedicated_workers {
            let unique_worker_id = UniqueWorkerId::from(dedicated_worker.id.as_str());
            let notifier = mid_notifiers.pop().expect("Failed to pop notifier");
            joiner.add_handle(spawn_on_dedicated(
                blocking_task(dedicated_worker.id.clone(), block_condition.clone(), notifier, info_fn),
                unique_worker_id,
            ));
        }

        // Wait for dedicated workers to finish.
        const WAIT_TIME: Duration = Duration::from_secs(3);
        let wait_result = mid_barrier.wait_for_notification(WAIT_TIME);
        unblock_tasks(block_condition);
        info!(wait_result = wait_result_str(wait_result, &WAIT_TIME));
        joiner.wait_for_all().await
    });
}

struct OnlyDedicatedWorkers;

impl Scenario for OnlyDedicatedWorkers {
    fn name(&self) -> &str {
        "only_dedicated_workers"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let rt = builder.build();

        common_run(rt, dedicated_workers, print_id);

        Ok(())
    }
}

async fn non_blocking_task(id: String) {
    info!(id);
}

struct SpawnToUnregisteredWorker;

impl Scenario for SpawnToUnregisteredWorker {
    fn name(&self) -> &str {
        "spawn_to_unregistered_worker"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        rt.block_on(async move {
            let unregistered_worker_id = "unregistered_worker".to_string();
            let unique_worker_id = UniqueWorkerId::from(unregistered_worker_id.clone());
            let _ = spawn_on_dedicated(non_blocking_task(unregistered_worker_id), unique_worker_id).await;
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

struct ThreadPriority;

impl Scenario for ThreadPriority {
    fn name(&self) -> &str {
        "thread_priority"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let rt = builder.build();

        common_run(rt, dedicated_workers, print_id_and_params);

        Ok(())
    }
}

fn print_id_and_affinity(id: &str) {
    let affinity = format!("{:?}", current_thread_affinity());
    info!(id, affinity);
}

struct ThreadAffinity;

impl Scenario for ThreadAffinity {
    fn name(&self) -> &str {
        "thread_affinity"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let rt = builder.build();

        common_run(rt, dedicated_workers, print_id_and_affinity);

        Ok(())
    }
}

struct BlockAllRegularWorkOnDedicated;

impl Scenario for BlockAllRegularWorkOnDedicated {
    fn name(&self) -> &str {
        "block_all_regular_work_on_dedicated"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let num_workers = exec_engine.workers;
        let mut rt = builder.build();

        let _ = rt.block_on(async move {
            const WAIT_TIME: Duration = Duration::from_secs(3);
            let mut joiner = RuntimeJoiner::new();
            // Regular and dedicated workers are handled and waited for separately.
            let reg_barrier = MultiExecutionBarrier::new(num_workers - 1);
            let mut reg_notifiers = reg_barrier.get_notifiers();
            let ded_barrier = MultiExecutionBarrier::new(dedicated_workers.len());
            let mut ded_notifiers = ded_barrier.get_notifiers();

            // Condition variable and mutex containing `block` variable.
            let reg_block_condition = Arc::new((Condvar::new(), Mutex::new(true)));
            let ded_block_condition = Arc::new((Condvar::new(), Mutex::new(true)));

            // Current worker is spawning tasks for other workers.
            info!(id = "worker_0");

            // Spawn tasks on regular workers.
            for id in 1..num_workers {
                let reg_worker_id = format!("worker_{id}");
                let notifier = reg_notifiers.pop().expect("Failed to pop notifier");

                joiner.add_handle(spawn(blocking_task(reg_worker_id, reg_block_condition.clone(), notifier, print_id)));
            }

            // Wait for regular workers to be blocked.
            let reg_wait_result = reg_barrier.wait_for_notification(WAIT_TIME);
            info!(reg_wait_result = wait_result_str(reg_wait_result, &WAIT_TIME));

            // Spawn tasks on dedicated workers.
            for dedicated_worker in dedicated_workers {
                let unique_worker_id = UniqueWorkerId::from(dedicated_worker.id.as_str());
                let notifier = ded_notifiers.pop().expect("Failed to pop notifier");
                joiner.add_handle(spawn_on_dedicated(
                    blocking_task(dedicated_worker.id.clone(), ded_block_condition.clone(), notifier, print_id),
                    unique_worker_id,
                ));
            }

            // Wait for dedicated workers to be blocked.
            let ded_wait_result = ded_barrier.wait_for_notification(WAIT_TIME);
            info!(ded_wait_result = wait_result_str(ded_wait_result, &WAIT_TIME));

            // Unblock all tasks.
            unblock_tasks(ded_block_condition);
            unblock_tasks(reg_block_condition);

            joiner.wait_for_all().await
        });

        Ok(())
    }
}

struct BlockDedicatedWorkOnRegular;

impl Scenario for BlockDedicatedWorkOnRegular {
    fn name(&self) -> &str {
        "block_dedicated_work_on_regular"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let dedicated_workers = exec_engine.dedicated_workers.clone().expect("No dedicated workers configuration found");
        let num_workers = exec_engine.workers;
        let mut rt = builder.build();

        let _ = rt.block_on(async move {
            const WAIT_TIME: Duration = Duration::from_secs(3);
            let mut joiner = RuntimeJoiner::new();
            // Regular and dedicated workers are handled and waited for separately.
            let reg_barrier = MultiExecutionBarrier::new(num_workers - 1);
            let mut reg_notifiers = reg_barrier.get_notifiers();
            let ded_barrier = MultiExecutionBarrier::new(dedicated_workers.len());
            let mut ded_notifiers = ded_barrier.get_notifiers();

            // Condition variable and mutex containing `block` variable.
            let reg_block_condition = Arc::new((Condvar::new(), Mutex::new(true)));
            let ded_block_condition = Arc::new((Condvar::new(), Mutex::new(true)));

            // Current worker is spawning tasks for other workers.
            info!(id = "worker_0");

            // Spawn tasks on dedicated workers.
            for dedicated_worker in dedicated_workers {
                let unique_worker_id = UniqueWorkerId::from(dedicated_worker.id.as_str());
                let notifier = ded_notifiers.pop().expect("Failed to pop notifier");
                joiner.add_handle(spawn_on_dedicated(
                    blocking_task(dedicated_worker.id.clone(), ded_block_condition.clone(), notifier, print_id),
                    unique_worker_id,
                ));
            }

            // Wait for dedicated workers to be blocked.
            let ded_wait_result = ded_barrier.wait_for_notification(WAIT_TIME);
            info!(ded_wait_result = wait_result_str(ded_wait_result, &WAIT_TIME));

            // Spawn tasks on regular workers.
            for id in 1..num_workers {
                let reg_worker_id = format!("worker_{id}");
                let notifier = reg_notifiers.pop().expect("Failed to pop notifier");
                joiner.add_handle(spawn(blocking_task(reg_worker_id, reg_block_condition.clone(), notifier, print_id)));
            }

            // Wait for regular workers to be blocked.
            let reg_wait_result = reg_barrier.wait_for_notification(WAIT_TIME);
            info!(reg_wait_result = wait_result_str(reg_wait_result, &WAIT_TIME));

            // Unblock all tasks.
            unblock_tasks(reg_block_condition);
            unblock_tasks(ded_block_condition);

            joiner.wait_for_all().await
        });

        Ok(())
    }
}

pub fn dedicated_worker_group() -> Box<dyn ScenarioGroup> {
    Box::new(ScenarioGroupImpl::new(
        "dedicated_worker",
        vec![
            Box::new(OnlyDedicatedWorkers),
            Box::new(SpawnToUnregisteredWorker),
            Box::new(ThreadPriority),
            Box::new(ThreadAffinity),
            Box::new(BlockAllRegularWorkOnDedicated),
            Box::new(BlockDedicatedWorkOnRegular),
        ],
        vec![],
    ))
}
