use crate::internals::helpers::execution_barrier::ExecutionBarrier;
use crate::internals::helpers::runtime_helper::Runtime;
use crate::internals::test_case::TestCase;
use orchestration::{prelude::*, program::ProgramBuilder};
use tracing::info;

pub struct OnlyShutdownSequenceTest;

/// Checks (almost) empty program with only shutdown
impl TestCase for OnlyShutdownSequenceTest {
    fn get_name(&self) -> &'static str {
        "only_shutdown"
    }

    fn run(&self, input: Option<String>) -> Result<(), String> {
        // let logic: TestInput = TestInput::new(&input);
        // TODO: Read TestInput and make 2 shutdowns
        let mut rt = Runtime::new(&input).build();

        let barrier = ExecutionBarrier::new();
        let notifier = barrier.get_notifier();
        let _ = rt.enter_engine(async move {
            info!("Program entered engine");
            let mut program = ProgramBuilder::new(file!())
                .with_body(Sequence::new_with_id(NamedId::new_static("Sequence")))
                .with_shutdown_notification(Sync::new("Shutdown"))
                .build();

            program.run_n(2).await;
            notifier.notify();
            info!("Program execution finished");
        });

        barrier.wait_for_notification(std::time::Duration::from_secs(5))
    }
}
