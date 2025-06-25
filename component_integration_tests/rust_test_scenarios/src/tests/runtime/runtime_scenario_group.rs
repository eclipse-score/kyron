use super::worker_basic::BasicWorker;
use super::worker_with_blocking_tasks::WorkerWithBlockingTasks;
use crate::internals::scenario::{ScenarioGroup, ScenarioGroupImpl};

pub struct RuntimeScenarioGroup {
    group: ScenarioGroupImpl,
}

impl RuntimeScenarioGroup {
    pub fn new() -> Self {
        RuntimeScenarioGroup {
            group: ScenarioGroupImpl::new("runtime"),
        }
    }
}

impl ScenarioGroup for RuntimeScenarioGroup {
    fn get_group_impl(&mut self) -> &mut ScenarioGroupImpl {
        &mut self.group
    }

    fn init(&mut self) -> () {
        self.group.add_scenario(Box::new(BasicWorker));
        self.group.add_scenario(Box::new(WorkerWithBlockingTasks));
    }
}
