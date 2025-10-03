use crate::internals::execution_barrier::{MultiExecutionBarrier, RuntimeJoiner};
use crate::internals::runtime_helper::{DedicatedWorkerConfig, Runtime};
use crate::internals::thread_params::{current_thread_affinity, current_thread_priority_params, ThreadPriorityParams};
use async_runtime::core::types::UniqueWorkerId;
use async_runtime::runtime::async_runtime::AsyncRuntime;
use async_runtime::spawn_on_dedicated;
use foundation::threading::thread_wait_barrier::ThreadReadyNotifier;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use test_scenarios_rust::scenario::{Scenario, ScenarioGroup, ScenarioGroupImpl};
use tracing::info;

async fn blocking_task(id: String, block_condition: Arc<(Condvar, Mutex<bool>)>, notifier: ThreadReadyNotifier, info_fn: fn(&str)) {
    let (cv, mtx) = &*block_condition;
    let mut block = mtx.lock().expect("Unable to lock mutex");

    // Allow only first batch of tasks to trace.
    if *block {
        info_fn(&id);
    }

    // Notify task done.
    notifier.ready();

    // Wait until barrier timed out.
    while *block {
        block = cv.wait(block).expect("Unable to wait - poisoned mutex?");
    }
}

/// Common run implementation.
fn common_run(mut rt: AsyncRuntime, dedicated_workers: Vec<DedicatedWorkerConfig>, info_fn: fn(&str)) {
    let _ = rt.block_on(async move {
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

        // Allow tasks to finish and disable tracing.
        {
            let (cv, mtx) = &*block_condition;
            let mut block = mtx.lock().expect("Unable to lock mutex");
            *block = false;
            cv.notify_all();
        }

        match wait_result {
            Ok(_) => info!(wait_result = "ok"),
            Err(e) => {
                let expected_message = format!("Failed to join tasks after {} seconds", WAIT_TIME.as_secs());
                if e == expected_message {
                    info!(wait_result = "timeout");
                } else {
                    info!(wait_result = "other", error = e)
                }
            }
        }

        Ok(joiner.wait_for_all().await)
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

        common_run(rt, dedicated_workers, |id| info!(id));

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

        let _ = rt.block_on(async move {
            let unregistered_worker_id = "unregistered_worker".to_string();
            let unique_worker_id = UniqueWorkerId::from(unregistered_worker_id.clone());
            let _ = spawn_on_dedicated(non_blocking_task(unregistered_worker_id), unique_worker_id).await;
            Ok(0)
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

pub fn dedicated_worker_group() -> Box<dyn ScenarioGroup> {
    Box::new(ScenarioGroupImpl::new(
        "dedicated_worker",
        vec![
            Box::new(OnlyDedicatedWorkers),
            Box::new(SpawnToUnregisteredWorker),
            Box::new(ThreadPriority),
            Box::new(ThreadAffinity),
        ],
        vec![],
    ))
}
