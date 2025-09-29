use crate::internals::execution_barrier::{MultiExecutionBarrier, RuntimeJoiner};
use crate::internals::runtime_helper::Runtime;
use async_runtime::spawn;
use foundation::threading::thread_wait_barrier::ThreadReadyNotifier;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use test_scenarios_rust::scenario::Scenario;
use tracing::info;

async fn blocking_task(id: usize, block_condition: Arc<(Condvar, Mutex<bool>)>, notifier: ThreadReadyNotifier) {
    let (cv, mtx) = &*block_condition;
    let mut block = mtx.lock().expect("Unable to lock mutex");

    // Allow only first batch of tasks to trace.
    if *block {
        let id = format!("worker_{id}");
        info!(id);
    }

    // Notify task done.
    // This should never be satisfied for all workers.
    notifier.ready();

    // Wait until barrier timed out.
    while *block {
        block = cv.wait(block).expect("Unable to wait - poisoned mutex?");
    }
}

pub struct NumWorkers;

impl Scenario for NumWorkers {
    fn name(&self) -> &str {
        "num_workers"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let num_workers = exec_engine.workers;
        let mut rt = builder.build();

        let _ = rt.block_on(async move {
            let mut joiner = RuntimeJoiner::new();
            let mid_barrier = MultiExecutionBarrier::new(num_workers);
            let mut mid_notifiers = mid_barrier.get_notifiers();

            // Condition variable and mutex containing `block` variable.
            let block_condition = Arc::new((Condvar::new(), Mutex::new(true)));

            // Current worker is spawning tasks for other workers.
            info!(id = "worker_0");

            // Spawn tasks.
            for id in 1..=num_workers {
                let notifier = mid_notifiers.pop().expect("Failed to pop notifier");
                joiner.add_handle(spawn(blocking_task(id, block_condition.clone(), notifier)));
            }

            // Wait is expected to fail on timeout.
            // There's always one task more than available workers.
            // Wait time must be longer for more tasks.
            let wait_s = if num_workers > 32 { 10 } else { 3 };
            let result = mid_barrier.wait_for_notification(Duration::from_secs(wait_s));

            // Allow tasks to finish and disable tracing.
            {
                let (cv, mtx) = &*block_condition;
                let mut block = mtx.lock().expect("Unable to lock mutex");
                *block = false;
                cv.notify_all();
            }

            match result {
                // Expected `RuntimeErrors` are not exactly suitable to express intent.
                Ok(_) => info!(wait_result = "ok"),
                // Only timeout is expected.
                Err(e) => {
                    let expected_message = format!("Failed to join tasks after {wait_s} seconds");
                    if e == expected_message {
                        info!(wait_result = "timeout");
                    } else {
                        info!(wait_result = "other", error = e)
                    }
                }
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}
