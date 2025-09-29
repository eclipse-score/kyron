use crate::internals::execution_barrier::RuntimeJoiner;
use crate::internals::runtime_helper::Runtime;
use test_scenarios_rust::scenario::Scenario;

use async_runtime::channels::spsc;
use async_runtime::spawn;

use foundation::prelude::CommonErrors;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hint;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
struct TestInput {
    data_to_send: Vec<u64>,
}

impl TestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

async fn send_task<const SIZE: usize>(sender: spsc::Sender<u64, SIZE>, data_to_send: Vec<u64>) {
    for data in data_to_send.as_slice() {
        let result = sender.send(data);
        match result {
            Err(e) => {
                info!(id = "send_task", error = format!("{e:?}"));
            }
            Ok(_) => info!(id = "send_task", data = data),
        }
    }
}

async fn receive_task<const SIZE: usize>(mut receiver: spsc::Receiver<u64, SIZE>, expected_data_count: usize) {
    for _ndx in 0..expected_data_count {
        let result = receiver.recv().await;
        match result {
            Some(val) => {
                info!(id = "receive_task", data = val);
            }
            None => {
                info!(id = "receive_task", error = "Provider dropped");
            }
        }
    }
}

pub struct SPSCSendReceive;

impl Scenario for SPSCSendReceive {
    fn name(&self) -> &str {
        "send_receive"
    }

    ///
    /// Runs sending and receiving tasks.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spsc::create_channel_default::<u64>();

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            joiner.add_handle(spawn(send_task(sender, logic.data_to_send.clone())));
            joiner.add_handle(spawn(receive_task(receiver, logic.data_to_send.len())));
            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}

pub struct SPSCSendOnly;

impl Scenario for SPSCSendOnly {
    fn name(&self) -> &str {
        "send_only"
    }

    ///
    /// Runs only sending task, receiver remains valid.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        const QUEUE_SIZE: usize = 5; // Must be compile constant
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, _receiver) = spsc::create_channel::<u64, QUEUE_SIZE>();

        let _ = rt.block_on(async move {
            spawn(send_task(sender, logic.data_to_send.clone())).await.expect("Failed to spawn task");
            Ok(0)
        });

        Ok(())
    }
}

pub struct SPSCDropReceiver;

impl Scenario for SPSCDropReceiver {
    fn name(&self) -> &str {
        "drop_receiver"
    }

    ///
    /// Drops the receiver before running sending task.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spsc::create_channel_default::<u64>();
        drop(receiver);

        let _ = rt.block_on(async move {
            spawn(send_task(sender, logic.data_to_send.clone())).await.expect("Failed to spawn task");
            Ok(0)
        });

        Ok(())
    }
}

pub struct SPSCDropSender;

impl Scenario for SPSCDropSender {
    fn name(&self) -> &str {
        "drop_sender"
    }

    ///
    /// Drops the sender before running receiving task.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spsc::create_channel_default::<u64>();
        drop(sender);

        let _ = rt.block_on(async move {
            spawn(receive_task(receiver, logic.data_to_send.len()))
                .await
                .expect("Failed to spawn task");
            Ok(0)
        });

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct DcSenderTestInput {
    data_to_send: Vec<u64>,
    overread_count: usize,
}

impl DcSenderTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}
pub struct SPSCDropSenderInTheMiddle;

impl Scenario for SPSCDropSenderInTheMiddle {
    fn name(&self) -> &str {
        "drop_sender_in_the_middle"
    }

    ///
    /// Drops the sender when running receiving task.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = DcSenderTestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spsc::create_channel_default::<u64>();

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            joiner.add_handle(spawn(send_task(sender, logic.data_to_send.clone())));
            joiner.add_handle(spawn(receive_task(receiver, logic.data_to_send.len() + logic.overread_count)));
            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct DcReceiverTestInput {
    first_step_data: Vec<u64>,
    second_step_data: Vec<u64>,
}

impl DcReceiverTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}

pub struct SPSCDropReceiverInTheMiddle;

impl SPSCDropReceiverInTheMiddle {
    fn send_data<const SIZE: usize>(sender: &spsc::Sender<u64, SIZE>, data: u64) {
        let result = sender.send(&data);
        match result {
            Err(e) => {
                info!(id = "send_task", error = format!("{e:?}"));
            }
            Ok(_) => info!(id = "send_task", data = data),
        }
    }

    pub async fn double_send_task<const SIZE: usize>(
        sender: spsc::Sender<u64, SIZE>,
        first_step_data: Vec<u64>,
        second_step_data: Vec<u64>,
        sync: Arc<AtomicBool>,
    ) {
        for data in first_step_data.as_slice() {
            Self::send_data(&sender, *data);
        }

        while !sync.load(Ordering::Acquire) {
            hint::spin_loop();
        }

        for data in second_step_data.as_slice() {
            Self::send_data(&sender, *data);
        }
    }
}

impl Scenario for SPSCDropReceiverInTheMiddle {
    fn name(&self) -> &str {
        "drop_receiver_in_the_middle"
    }

    ///
    /// Drops the receiver when running sending task.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = DcReceiverTestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spsc::create_channel_default::<u64>();

        let sync = Arc::new(AtomicBool::new(false));
        let _ = rt.block_on(async move {
            let handle = spawn(Self::double_send_task(
                sender,
                logic.first_step_data.clone(),
                logic.second_step_data.clone(),
                sync.clone(),
            ));

            spawn(receive_task(receiver, logic.first_step_data.len()))
                .await
                .expect("Failed to spawn receive task");
            sync.store(true, Ordering::Release);

            handle.await.expect("Failed to spawn send task");
            Ok(0)
        });

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct HeavyTestInput {
    send_count: u64,
}

impl HeavyTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input string");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse \"test\" field")
    }
}
pub struct SPSCHeavyLoad;

impl SPSCHeavyLoad {
    fn send_data<const SIZE: usize>(sender: &spsc::Sender<u64, SIZE>, data: u64) {
        loop {
            let result = sender.send(&data);
            match result {
                Err(e) => {
                    if e == CommonErrors::GenericError {
                        info!(id = "send_task", iter = data, error = format!("{e:?}"));
                        break;
                    } else if e == CommonErrors::NoSpaceLeft {
                        continue;
                    } else {
                        panic!("Unexpected error: {e:?}");
                    }
                }
                Ok(_) => break,
            }
        }
    }

    pub async fn heavy_send_task<const SIZE: usize>(sender: spsc::Sender<u64, SIZE>, data_count: u64) {
        for data in 1..=data_count {
            Self::send_data(&sender, data);
        }
    }

    pub async fn heavy_receive_task<const SIZE: usize>(mut receiver: spsc::Receiver<u64, SIZE>, expected_data_count: u64) {
        for expected_val in 1..=expected_data_count {
            let result = receiver.recv().await;
            match result {
                Some(val) => {
                    if val != expected_val {
                        info!(
                            id = "receive_task",
                            iter = expected_val,
                            error = format!("Expected {expected_val}, got {val}")
                        );
                    }
                }
                None => {
                    info!(id = "receive_task", iter = expected_val, error = "Provider dropped");
                }
            }
        }
    }
}

impl Scenario for SPSCHeavyLoad {
    fn name(&self) -> &str {
        "heavy_load"
    }

    ///
    /// Runs sending and receiving tasks. Validates that all data is sent and received correctly.
    /// Reports any errors. One final error is expected for the over-read.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        const QUEUE_SIZE: usize = 128;
        let logic = HeavyTestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spsc::create_channel::<u64, QUEUE_SIZE>();

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            joiner.add_handle(spawn(SPSCHeavyLoad::heavy_send_task(sender, logic.send_count)));
            joiner.add_handle(spawn(SPSCHeavyLoad::heavy_receive_task(receiver, logic.send_count + 1)));
            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}
