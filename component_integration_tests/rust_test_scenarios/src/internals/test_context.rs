use super::test_case::TestGroup;

pub struct TestContext {
    root_group: Box<dyn TestGroup>,
}

impl TestContext {
    pub fn new(root_group: Box<dyn TestGroup>) -> Self {
        TestContext { root_group: root_group }
    }

    pub fn run_test(&mut self, name: &str, input: Option<String>) -> () {
        let test = self.root_group.find_test(name);
        match test {
            Some(test) => test.run(input).expect("Run failed"),
            None => panic!("Test {} not found", name),
        };
    }
}
