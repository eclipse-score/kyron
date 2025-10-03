use crate::internals::execution_barrier::MultiExecutionBarrier;
use crate::internals::execution_barrier::RuntimeJoiner;
use crate::internals::runtime_helper::Runtime;
use crate::internals::thread_params::{current_thread_priority_params, ThreadPriorityParams};
use async_runtime::spawn;
use foundation::threading::thread_wait_barrier::ThreadReadyNotifier;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use test_scenarios_rust::scenario::Scenario;
use tracing::info;

fn show_thread_params(id: usize) {
    let id = format!("worker_{id}");
    let ThreadPriorityParams {
        scheduler,
        priority,
        priority_min,
        priority_max,
    } = current_thread_priority_params();
    info!(id, scheduler, priority, priority_min, priority_max);
}

async fn blocking_task(id: usize, block_condition: Arc<(Condvar, Mutex<bool>)>, notifier: ThreadReadyNotifier) {
    let (cv, mtx) = &*block_condition;
    let mut block = mtx.lock().expect("Unable to lock mutex");

    show_thread_params(id);

    // Notify task done.
    notifier.ready();

    // Block until allowed to finish.
    while *block {
        block = cv.wait(block).expect("Unable to wait - poisoned mutex?");
    }
}

pub struct ThreadPriority;

impl Scenario for ThreadPriority {
    fn name(&self) -> &'static str {
        "thread_priority"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let num_workers = exec_engine.workers;
        let mut rt = builder.build();

        let _ = rt.block_on(async move {
            let mut joiner = RuntimeJoiner::new();
            let mid_barrier = MultiExecutionBarrier::new(num_workers - 1);
            let mut mid_notifiers = mid_barrier.get_notifiers();

            // Condition variable and mutex containing `block` variable.
            let block_condition = Arc::new((Condvar::new(), Mutex::new(true)));

            // Show parameters of current thread.
            show_thread_params(0);

            // Spawn tasks for other threads.
            for id in 1..num_workers {
                let notifier = mid_notifiers.pop().expect("Failed to pop notifier");
                joiner.add_handle(spawn(blocking_task(id, block_condition.clone(), notifier)));
            }

            // Wait until all tasks shown params.
            const WAIT_TIME: Duration = Duration::from_secs(1);
            let result = mid_barrier.wait_for_notification(WAIT_TIME);

            // Allow tasks to finish.
            {
                let (cv, mtx) = &*block_condition;
                let mut block = mtx.lock().expect("Unable to lock mutex");
                *block = false;
                cv.notify_all();
            }

            result.expect("Failed to join tasks in given time");

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}
