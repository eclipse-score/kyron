use crate::internals::runtime_helper::Runtime;
use test_scenarios_rust::scenario::Scenario;
use tracing::info;

pub struct OnlyShutdownSequence;

/// Checks (almost) empty program with only shutdown
impl Scenario for OnlyShutdownSequence {
    fn name(&self) -> &str {
        "only_shutdown"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let mut rt = Runtime::from_json(input)?.build();

        let _ = rt.block_on(async move {
            info!("Program entered engine");
            // TODO: Create a program with only shutdown sequence once it is supported.
            info!("Program execution finished");
            Ok(0)
        });

        Ok(())
    }
}
