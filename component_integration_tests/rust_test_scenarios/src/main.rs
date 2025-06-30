mod internals;
mod tests;

use internals::helpers::monotonic_clock::MonotonicClockTime;
use internals::test_context::TestContext;
use tests::RootScenarioGroup;

use clap::{Parser, ValueEnum};
use std::io;

use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(ValueEnum, Debug, Clone, PartialEq, Eq)]
enum InputType {
    Stdin,
    Arg,
}

#[derive(Parser, Debug)]
pub struct Args {
    #[arg(short, long)]
    name: String,

    #[arg(short, long)]
    input: Option<String>,

    #[arg(long, value_enum, default_value_t = InputType::Stdin)]
    input_type: InputType,
}

fn read_test_stdin_input() -> String {
    println!("Please enter the TestInput in JSON format:");
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer).unwrap();
    // TODO: Handle empty stdin as option none
    return buffer;
}

fn init_tracing_subscriber() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::TRACE)
        .with_thread_ids(true)
        .with_timer(MonotonicClockTime::new())
        .json()
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Setting default subscriber failed!");
}

fn main() {
    let arguments = Args::parse();

    let mut input = arguments.input.clone();
    if arguments.input_type == InputType::Stdin {
        input = Some(read_test_stdin_input());
    }

    init_tracing_subscriber();

    let mut context = TestContext::new(Box::new(RootScenarioGroup::new()));
    context.run_scenario(arguments.name.as_str(), input);
}
