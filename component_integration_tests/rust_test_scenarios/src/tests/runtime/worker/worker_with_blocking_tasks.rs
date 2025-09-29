use crate::internals::execution_barrier::{MultiExecutionBarrier, RuntimeJoiner};
use crate::internals::runtime_helper::Runtime;
use test_scenarios_rust::scenario::Scenario;

use async_runtime::spawn;
use foundation::threading::thread_wait_barrier::ThreadReadyNotifier;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
struct TestInput {
    blocking_tasks: Vec<String>,
    non_blocking_tasks: Vec<String>,
}

impl TestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

fn simple_checkpoint(id: &str) {
    info!(id = id);
}

fn location_checkpoint(id: &str, location: &str) {
    info!(id = id, location = location);
}

async fn non_blocking_task(name: String, counter: Arc<AtomicUsize>) {
    simple_checkpoint(name.as_str());
    counter.fetch_add(1, Ordering::Release);
}

async fn blocking_task(name: String, counter: Arc<AtomicUsize>, counter_unblock_value: usize, notifier: ThreadReadyNotifier) {
    location_checkpoint(name.as_str(), "begin");
    counter.fetch_add(1, Ordering::Release);
    notifier.ready();

    while counter.load(Ordering::Acquire) != counter_unblock_value {} // Blocking loop
    location_checkpoint(name.as_str(), "end");
}

pub struct WorkerWithBlockingTasks;

impl Scenario for WorkerWithBlockingTasks {
    fn name(&self) -> &str {
        "with_blocking_tasks"
    }

    ///
    /// Spawns all blocking_tasks first, which will be unblocked once all nonblocking_tasks are executed.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let _ = rt.block_on(async move {
            let mut joiner = RuntimeJoiner::new();
            let mid_barrier = MultiExecutionBarrier::new(logic.blocking_tasks.len());
            let mut mid_notifiers = mid_barrier.get_notifiers();

            let counter = Arc::new(AtomicUsize::new(0));
            let all_tasks_count: usize = logic.blocking_tasks.len() + logic.non_blocking_tasks.len();
            for name in logic.blocking_tasks.as_slice() {
                let notifier = mid_notifiers.pop().expect("Failed to pop notifier");
                joiner.add_handle(spawn(blocking_task(name.to_string(), counter.clone(), all_tasks_count, notifier)));
            }
            mid_barrier.wait_for_notification(Duration::from_secs(5)).expect("Failed to join tasks");

            for name in logic.non_blocking_tasks.as_slice() {
                joiner.add_handle(spawn(non_blocking_task(name.to_string(), counter.clone())));
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}
