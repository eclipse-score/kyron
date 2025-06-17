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

        let _ = rt.block_on(async move {
            info!("Program entered engine");
            let mut program = ProgramBuilder::new(file!())
                .with_body(Sequence::new_with_id(NamedId::new_static("Sequence")))
                .with_shutdown_notification(Sync::new("Shutdown"))
                .build();

            program.run_n(2).await;
            info!("Program execution finished");
            Ok(0)
        });

        Ok(())
    }
}
