use crate::internals::scenario::{ScenarioGroup, ScenarioGroupImpl};
use basic::BasicScenarioGroup;
use orchestration::OrchestrationScenarioGroup;
use runtime::RuntimeScenarioGroup;

pub mod basic;
pub mod orchestration;
pub mod runtime;

pub struct RootScenarioGroup {
    group: ScenarioGroupImpl,
}

impl RootScenarioGroup {
    pub fn new() -> Self {
        RootScenarioGroup {
            group: ScenarioGroupImpl::new("root"),
        }
    }
}

impl ScenarioGroup for RootScenarioGroup {
    fn get_group_impl(&mut self) -> &mut ScenarioGroupImpl {
        &mut self.group
    }

    fn init(&mut self) -> () {
        self.group.add_group(Box::new(BasicScenarioGroup::new()));
        self.group.add_group(Box::new(RuntimeScenarioGroup::new()));
        self.group.add_group(Box::new(OrchestrationScenarioGroup::new()));
    }
}
