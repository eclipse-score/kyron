pub trait TestCase {
    fn get_name(&self) -> &'static str;
    fn run(&self, input: Option<String>) -> Result<(), String>;
}

pub trait TestGroup {
    fn get_test_group_impl(&mut self) -> &mut TestGroupImpl;
    fn init(&mut self);

    fn get_name(&mut self) -> &str {
        self.get_test_group_impl().get_name()
    }

    fn find_test(&mut self, name: &str) -> Option<&dyn TestCase> {
        self.init();
        self.get_test_group_impl().find_test(name)
    }
}

pub struct TestGroupImpl {
    name: String,
    test_cases: Vec<Box<dyn TestCase>>,
    test_groups: Vec<Box<dyn TestGroup>>,
}

impl TestGroupImpl {
    pub fn new(name: &str) -> Self {
        TestGroupImpl {
            name: name.to_string(),
            test_cases: vec![],
            test_groups: vec![],
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn add_test_case(&mut self, test_case: Box<dyn TestCase>) {
        self.test_cases.push(test_case);
    }

    pub fn add_test_group(&mut self, test_group: Box<dyn TestGroup>) {
        self.test_groups.push(test_group);
    }

    pub fn find_test(&mut self, name: &str) -> Option<&dyn TestCase> {
        let split: Vec<&str> = name.split('.').collect();
        if split.len() == 1 {
            for test_case in &self.test_cases {
                if test_case.get_name() == name {
                    return Some(test_case.as_ref());
                }
            }
            None
        } else {
            for test_group in &mut self.test_groups {
                if test_group.get_name() == split[0] {
                    return test_group.find_test(split[1..].join(".").as_str());
                }
            }
            None
        }
    }
}
