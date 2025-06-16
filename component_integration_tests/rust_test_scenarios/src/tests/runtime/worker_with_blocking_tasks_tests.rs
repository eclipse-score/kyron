use crate::internals::helpers::execution_barrier::{ExecutionBarrier, ExecutionNotifier, MultiExecutionBarrier};
use crate::internals::helpers::runtime_helper::Runtime;
use crate::internals::test_case::TestCase;

use async_runtime::spawn;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
struct TestInput {
    blocking_tasks: Vec<String>,
    non_blocking_tasks: Vec<String>,
}

impl TestInput {
    pub fn new(inputs: &Option<String>) -> Self {
        let v: Value = serde_json::from_str(inputs.as_deref().unwrap()).unwrap();
        serde_json::from_value(v["test"].clone()).unwrap()
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

async fn blocking_task(name: String, counter: Arc<AtomicUsize>, counter_unblock_value: usize, notifier: ExecutionNotifier) {
    location_checkpoint(name.as_str(), "begin");
    counter.fetch_add(1, Ordering::Release);
    notifier.notify();

    while counter.load(Ordering::Acquire) != counter_unblock_value {} // Blocking loop
    location_checkpoint(name.as_str(), "end");
}

pub struct WorkerWithBlockingTasksTest;

impl TestCase for WorkerWithBlockingTasksTest {
    fn get_name(&self) -> &'static str {
        "worker_with_blocking_tasks"
    }

    ///
    /// Spawns all blocking_tasks first, which will be unblocked once all nonblocking_tasks are executed.
    ///
    fn run(&self, input: Option<String>) -> Result<(), String> {
        let logic = TestInput::new(&input);
        let mut rt = Runtime::new(&input).build();

        let barrier = ExecutionBarrier::new();
        let mut notifier = barrier.get_notifier();

        let _ = rt.enter_engine(async move {
            let mid_barrier = MultiExecutionBarrier::new(logic.blocking_tasks.len());
            let mut mid_notifiers = mid_barrier.get_notifiers();

            let counter = Arc::new(AtomicUsize::new(0));
            let all_tasks_count: usize = logic.blocking_tasks.len() + logic.non_blocking_tasks.len();
            for name in logic.blocking_tasks.as_slice() {
                notifier.add_handle(spawn(blocking_task(
                    name.to_string(),
                    counter.clone(),
                    all_tasks_count,
                    mid_notifiers.pop().unwrap(),
                )));
            }
            mid_barrier.wait_for_notification(Duration::from_secs(5)).unwrap();

            for name in logic.non_blocking_tasks.as_slice() {
                notifier.add_handle(spawn(non_blocking_task(name.to_string(), counter.clone())));
            }

            notifier.wait_and_notify().await;
        });

        barrier.wait_for_notification(Duration::from_secs(5))
    }
}
