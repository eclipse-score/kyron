use super::basic::basic_test_group::BasicTestGroup;
use super::orchestration::orchestration_test_group::OrchestrationTestGroup;
use super::runtime::runtime_test_group::RuntimeTestGroup;
use crate::internals::test_case::{TestGroup, TestGroupImpl};

pub struct RootTestGroup {
    test_group: TestGroupImpl,
}

impl RootTestGroup {
    pub fn new() -> Self {
        RootTestGroup {
            test_group: TestGroupImpl::new("root"),
        }
    }
}

impl TestGroup for RootTestGroup {
    fn get_test_group_impl(&mut self) -> &mut TestGroupImpl {
        &mut self.test_group
    }

    fn init(&mut self) -> () {
        self.test_group.add_test_group(Box::new(BasicTestGroup::new()));
        self.test_group.add_test_group(Box::new(RuntimeTestGroup::new()));
        self.test_group.add_test_group(Box::new(OrchestrationTestGroup::new()));
    }
}
