mod num_workers;
mod thread_affinity;
mod thread_priority;
mod worker_basic;
mod worker_with_blocking_tasks;

use num_workers::NumWorkers;
use test_scenarios_rust::scenario::{ScenarioGroup, ScenarioGroupImpl};
use thread_affinity::ThreadAffinity;
use thread_priority::ThreadPriority;
use worker_basic::BasicWorker;
use worker_with_blocking_tasks::WorkerWithBlockingTasks;

pub fn worker_scenario_group() -> Box<dyn ScenarioGroup> {
    Box::new(ScenarioGroupImpl::new(
        "worker",
        vec![
            Box::new(BasicWorker),
            Box::new(WorkerWithBlockingTasks),
            Box::new(NumWorkers),
            Box::new(ThreadAffinity),
            Box::new(ThreadPriority),
        ],
        vec![],
    ))
}
