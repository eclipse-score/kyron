pub trait Scenario {
    fn get_name(&self) -> &'static str;
    fn run(&self, input: Option<String>) -> Result<(), String>;
}

pub trait ScenarioGroup {
    fn get_group_impl(&mut self) -> &mut ScenarioGroupImpl;
    fn init(&mut self);

    fn get_name(&mut self) -> &str {
        self.get_group_impl().get_name()
    }

    fn find_scenario(&mut self, name: &str) -> Option<&dyn Scenario> {
        self.init();
        self.get_group_impl().find_scenario(name)
    }
}

pub struct ScenarioGroupImpl {
    name: String,
    scenarios: Vec<Box<dyn Scenario>>,
    groups: Vec<Box<dyn ScenarioGroup>>,
}

impl ScenarioGroupImpl {
    pub fn new(name: &str) -> Self {
        ScenarioGroupImpl {
            name: name.to_string(),
            scenarios: vec![],
            groups: vec![],
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn add_scenario(&mut self, scenario: Box<dyn Scenario>) {
        self.scenarios.push(scenario);
    }

    pub fn add_group(&mut self, group: Box<dyn ScenarioGroup>) {
        self.groups.push(group);
    }

    pub fn find_scenario(&mut self, name: &str) -> Option<&dyn Scenario> {
        let split: Vec<&str> = name.split('.').collect();
        if split.len() == 1 {
            for scenario in &self.scenarios {
                if scenario.get_name() == name {
                    return Some(scenario.as_ref());
                }
            }
            None
        } else {
            for group in &mut self.groups {
                if group.get_name() == split[0] {
                    return group.find_scenario(split[1..].join(".").as_str());
                }
            }
            None
        }
    }
}
