use crate::internals::helpers::runtime_helper::Runtime;
use crate::internals::scenario::Scenario;
//use orchestration::{prelude::*, program::ProgramBuilder};
use tracing::info;

pub struct OnlyShutdownSequence;

/// Checks (almost) empty program with only shutdown
impl Scenario for OnlyShutdownSequence {
    fn get_name(&self) -> &'static str {
        "only_shutdown"
    }

    fn run(&self, input: Option<String>) -> Result<(), String> {
        // let logic: TestInput = TestInput::new(&input);
        // TODO: Read TestInput and make 2 shutdowns
        let mut rt = Runtime::new(&input).build();

        let _ = rt.block_on(async move {
            info!("Program entered engine");
            // TODO: Create a program with only shutdown sequence once it is supported.
            info!("Program execution finished");
            Ok(0)
        });

        Ok(())
    }
}
