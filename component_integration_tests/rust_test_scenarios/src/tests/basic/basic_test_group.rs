use super::basic_tests::OnlyShutdownSequenceTest;
use crate::internals::test_case::{TestGroup, TestGroupImpl};

pub struct BasicTestGroup {
    test_group: TestGroupImpl,
}

impl BasicTestGroup {
    pub fn new() -> Self {
        BasicTestGroup {
            test_group: TestGroupImpl::new("basic"),
        }
    }
}

impl TestGroup for BasicTestGroup {
    fn get_test_group_impl(&mut self) -> &mut TestGroupImpl {
        &mut self.test_group
    }

    fn init(&mut self) -> () {
        self.test_group.add_test_case(Box::new(OnlyShutdownSequenceTest));
    }
}
