use super::basic::basic_scenario_group::BasicScenarioGroup;
use super::orchestration::orchestration_scenario_group::OrchestrationScenarioGroup;
use super::runtime::runtime_scenario_group::RuntimeScenarioGroup;
use crate::internals::scenario::{ScenarioGroup, ScenarioGroupImpl};

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
