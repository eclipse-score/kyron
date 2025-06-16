use super::worker_basic_tests::BasicWorkerTest;
use super::worker_with_blocking_tasks_tests::WorkerWithBlockingTasksTest;
use crate::internals::test_case::{TestGroup, TestGroupImpl};

pub struct RuntimeTestGroup {
    test_group: TestGroupImpl,
}

impl RuntimeTestGroup {
    pub fn new() -> Self {
        RuntimeTestGroup {
            test_group: TestGroupImpl::new("runtime"),
        }
    }
}

impl TestGroup for RuntimeTestGroup {
    fn get_test_group_impl(&mut self) -> &mut TestGroupImpl {
        &mut self.test_group
    }

    fn init(&mut self) -> () {
        self.test_group.add_test_case(Box::new(BasicWorkerTest));
        self.test_group.add_test_case(Box::new(WorkerWithBlockingTasksTest));
    }
}
