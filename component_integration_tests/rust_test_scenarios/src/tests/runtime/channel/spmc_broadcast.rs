use crate::internals::execution_barrier::RuntimeJoiner;
use crate::internals::runtime_helper::Runtime;
use test_scenarios_rust::scenario::Scenario;

use async_runtime::channels::spmc_broadcast;
use async_runtime::spawn;
use foundation::prelude::CommonErrors;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::hint;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use tracing::info;

#[derive(Serialize, Deserialize, Debug)]
struct TestInput {
    data_to_send: Vec<u64>,
    receivers: Vec<String>,
    max_receiver_count: u16,
}

impl TestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input JSON string in TestInput::new");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse 'test' field from input JSON in TestInput::new")
    }
}

fn send_value<const SIZE: usize>(sender: &spmc_broadcast::Sender<u64, SIZE>, value: &u64) {
    let result = sender.send(value);
    match result {
        Err(e) => {
            info!(id = "send_task", error = format!("{e:?}"));
        }
        Ok(_) => info!(id = "send_task", data = value),
    }
}

fn send_data<const SIZE: usize>(sender: &spmc_broadcast::Sender<u64, SIZE>, data_to_send: &[u64]) {
    for data in data_to_send {
        send_value(sender, data);
    }
}

async fn send_task<const SIZE: usize>(sender: spmc_broadcast::Sender<u64, SIZE>, data_to_send: Vec<u64>) {
    send_data(&sender, &data_to_send);
}

async fn receive_data<const SIZE: usize>(name: &str, receiver: &mut spmc_broadcast::Receiver<u64, SIZE>, read_data_count: usize) {
    for _ndx in 0..read_data_count {
        let result = receiver.recv().await;
        match result {
            Some(val) => {
                info!(id = name, data = val);
            }
            None => {
                info!(id = name, error = "Provider dropped");
            }
        }
    }
}

async fn receive_task<const SIZE: usize>(name: String, mut receiver: spmc_broadcast::Receiver<u64, SIZE>, read_data_count: usize) {
    receive_data(&name, &mut receiver, read_data_count).await;
}

fn prepare_receivers<const SIZE: usize>(
    count: usize,
    base_receiver: spmc_broadcast::Receiver<u64, SIZE>,
) -> Vec<spmc_broadcast::Receiver<u64, SIZE>> {
    let mut receivers = Vec::with_capacity(count);
    for _ in 0..count - 1 {
        receivers.push(base_receiver.try_clone().expect("Failed to clone receiver"));
    }
    receivers.push(base_receiver);
    receivers
}

pub struct SPMCBroadcastSendReceive;

impl Scenario for SPMCBroadcastSendReceive {
    fn name(&self) -> &str {
        "send_receive"
    }

    ///
    /// Runs sending and receiving tasks.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spmc_broadcast::create_channel_default::<u64>(logic.max_receiver_count);
        let receivers = prepare_receivers(logic.receivers.len(), receiver);

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            joiner.add_handle(spawn(send_task(sender, logic.data_to_send.clone())));

            for (receiver, name) in receivers.into_iter().zip(logic.receivers) {
                joiner.add_handle(spawn(receive_task(name, receiver, logic.data_to_send.len())));
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct OverflowTestInput {
    population_method: String,
    receiver_count: usize,
    max_receiver_count: u16,
}

impl OverflowTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input JSON string in OverflowTestInput::new");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse 'test' field from input JSON in OverflowTestInput::new")
    }
}

pub struct SPMCBroadcastCreateReceiversOnly;

impl SPMCBroadcastCreateReceiversOnly {
    pub fn clone_receivers_handle_overflow<const SIZE: usize>(
        count: usize,
        base_receiver: spmc_broadcast::Receiver<u64, SIZE>,
    ) -> Vec<spmc_broadcast::Receiver<u64, SIZE>> {
        let mut receivers = Vec::with_capacity(count);
        let clone_count = count - 1; // There is one base receiver already

        for _ in 0..clone_count {
            let result = base_receiver.try_clone();
            match result {
                Some(cloned) => receivers.push(cloned),
                None => {
                    info!(id = "receivers_handle_overflow");
                }
            }
        }

        receivers.push(base_receiver);
        receivers
    }

    pub fn subscribe_receivers_handle_overflow<const SIZE: usize>(
        count: usize,
        base_receiver: spmc_broadcast::Receiver<u64, SIZE>,
        sender: &spmc_broadcast::Sender<u64, SIZE>,
    ) -> Vec<spmc_broadcast::Receiver<u64, SIZE>> {
        let mut receivers = Vec::with_capacity(count);
        let subscribe_count = count - 1; // There is one base receiver already

        for _ in 0..subscribe_count {
            let result = sender.subscribe();
            match result {
                Some(subscribed) => receivers.push(subscribed),
                None => {
                    info!(id = "receivers_handle_overflow");
                }
            }
        }

        receivers.push(base_receiver);
        receivers
    }
}

impl Scenario for SPMCBroadcastCreateReceiversOnly {
    fn name(&self) -> &str {
        "create_receivers_only"
    }

    ///
    /// Reports error when trying to create more receivers than allowed.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = OverflowTestInput::new(input);

        let (sender, receiver) = spmc_broadcast::create_channel_default::<u64>(logic.max_receiver_count);

        let _receivers = match logic.population_method.as_str() {
            "clone" => Self::clone_receivers_handle_overflow(logic.receiver_count, receiver),
            "subscribe" => Self::subscribe_receivers_handle_overflow(logic.receiver_count, receiver, &sender),
            _ => panic!("Unknown receiver population type: {}", logic.population_method),
        };

        Ok(())
    }
}

pub struct SPMCBroadcastNumOfSubscribers;

impl SPMCBroadcastNumOfSubscribers {
    pub fn log_subscriber_count<const SIZE: usize>(sender: &spmc_broadcast::Sender<u64, SIZE>, op_type: &str) {
        let subscriber_count = sender.num_of_subscribers();
        info!(id = "subscriber_count", op_type = op_type, value = subscriber_count);
    }

    pub fn clone_receivers_log_subscriber_count<const SIZE: usize>(
        count: usize,
        base_receiver: spmc_broadcast::Receiver<u64, SIZE>,
        sender: &spmc_broadcast::Sender<u64, SIZE>,
    ) -> Vec<spmc_broadcast::Receiver<u64, SIZE>> {
        let mut receivers = Vec::with_capacity(count);
        let clone_count = count - 1; // There is one base receiver already

        for _ in 0..clone_count {
            let result = base_receiver.try_clone();
            match result {
                Some(cloned) => receivers.push(cloned),
                None => {
                    info!(id = "receivers_handle_overflow");
                }
            }
            Self::log_subscriber_count(sender, "add_receiver");
        }

        receivers.push(base_receiver);
        receivers
    }

    pub fn subscribe_receivers_log_subscriber_count<const SIZE: usize>(
        count: usize,
        base_receiver: spmc_broadcast::Receiver<u64, SIZE>,
        sender: &spmc_broadcast::Sender<u64, SIZE>,
    ) -> Vec<spmc_broadcast::Receiver<u64, SIZE>> {
        let mut receivers = Vec::with_capacity(count);
        let subscribe_count = count - 1; // There is one base receiver already

        for _ in 0..subscribe_count {
            let result = sender.subscribe();
            match result {
                Some(subscribed) => receivers.push(subscribed),
                None => {
                    info!(id = "receivers_handle_overflow");
                }
            }
            Self::log_subscriber_count(sender, "add_receiver");
        }

        receivers.push(base_receiver);
        receivers
    }
}

impl Scenario for SPMCBroadcastNumOfSubscribers {
    fn name(&self) -> &str {
        "num_of_subscribers"
    }

    ///
    /// Checks number of subscribers in different phases.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = OverflowTestInput::new(input);

        let (sender, receiver) = spmc_broadcast::create_channel_default::<u64>(logic.max_receiver_count);
        Self::log_subscriber_count(&sender, "initial");

        let receivers = match logic.population_method.as_str() {
            "clone" => Self::clone_receivers_log_subscriber_count(logic.receiver_count, receiver, &sender),
            "subscribe" => Self::subscribe_receivers_log_subscriber_count(logic.receiver_count, receiver, &sender),
            _ => panic!("Unknown receiver population type: {}", logic.population_method),
        };

        for receiver in receivers {
            drop(receiver);
            Self::log_subscriber_count(&sender, "drop_receiver");
        }

        Ok(())
    }
}

pub struct SPMCBroadcastDropAddReceiver;

impl Scenario for SPMCBroadcastDropAddReceiver {
    fn name(&self) -> &str {
        "drop_add_receiver"
    }

    ///
    /// Runs sending and receiving tasks, where one of the receivers is dropped and a new one is created.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        if logic.receivers.len() < 2 {
            panic!("At least two receivers are required, otherwise channel will be closed");
        }

        let (sender, receiver) = spmc_broadcast::create_channel_default::<u64>(logic.max_receiver_count);
        let mut receivers = prepare_receivers(logic.receivers.len(), receiver);

        receivers.pop();
        receivers.push(receivers[0].try_clone().expect("Failed to clone receiver"));

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            joiner.add_handle(spawn(send_task(sender, logic.data_to_send.clone())));

            for (receiver, name) in receivers.into_iter().zip(logic.receivers) {
                joiner.add_handle(spawn(receive_task(name, receiver, logic.data_to_send.len())));
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}

pub struct SPMCBroadcastSendReceiveOneLagging;

impl SPMCBroadcastSendReceiveOneLagging {
    async fn send_task_with_ctrl<const SIZE: usize>(
        sender: spmc_broadcast::Sender<u64, SIZE>,
        data_to_send: Vec<u64>,
        data_is_sent_counter: Arc<AtomicUsize>,
        double_send_flag: Arc<AtomicBool>,
        exit_task_flag: Arc<AtomicBool>,
    ) {
        send_data(&sender, &data_to_send);
        data_is_sent_counter.fetch_add(1, Ordering::Release);

        while !double_send_flag.load(Ordering::Acquire) {
            hint::spin_loop();
        }
        send_data(&sender, &data_to_send);
        data_is_sent_counter.fetch_add(1, Ordering::Release);

        while !exit_task_flag.load(Ordering::Acquire) {
            hint::spin_loop();
        }
    }

    async fn receive_task_with_ctrl<const SIZE: usize>(
        name: String,
        mut receiver: spmc_broadcast::Receiver<u64, SIZE>,
        expected_data_count: usize,
        data_is_received_counter: Arc<AtomicUsize>,
        exit_task_flag: Arc<AtomicBool>,
    ) {
        receive_data(&name, &mut receiver, expected_data_count).await;
        data_is_received_counter.fetch_add(1, Ordering::Release);

        receive_data(&name, &mut receiver, expected_data_count).await;

        while !exit_task_flag.load(Ordering::Acquire) {
            hint::spin_loop();
        }
    }
}

impl Scenario for SPMCBroadcastSendReceiveOneLagging {
    fn name(&self) -> &str {
        "send_receive_one_lagging"
    }

    ///
    /// Runs sending and receiving tasks where one receiver does not read.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        const QUEUE_SIZE: usize = 4; // Must be compile constant
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spmc_broadcast::create_channel::<u64, { QUEUE_SIZE }>(logic.max_receiver_count);
        let mut receivers = prepare_receivers(logic.receivers.len(), receiver);

        // Receiver that will not read any data must stay valid
        let _receivers_tail = receivers.pop().expect("Failed to pop receiver");

        let send_receive_counter = Arc::new(AtomicUsize::new(0));
        let exit_task_flag = Arc::new(AtomicBool::new(false));
        let double_send_flag = Arc::new(AtomicBool::new(false));

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            joiner.add_handle(spawn(Self::send_task_with_ctrl(
                sender,
                logic.data_to_send.clone(),
                send_receive_counter.clone(),
                double_send_flag.clone(),
                exit_task_flag.clone(),
            )));

            let active_receiver_count = receivers.len();
            for (receiver, name) in receivers.into_iter().zip(logic.receivers) {
                joiner.add_handle(spawn(Self::receive_task_with_ctrl(
                    name,
                    receiver,
                    logic.data_to_send.len(),
                    send_receive_counter.clone(),
                    exit_task_flag.clone(),
                )));
            }

            // =Control logic for tasks=
            let sender_count = 1;

            // Wait until all active receivers and sender receive/send first batch of data
            let first_batch_checkpoint = active_receiver_count + sender_count;
            while send_receive_counter.load(Ordering::Acquire) != first_batch_checkpoint {
                hint::spin_loop();
            }

            // Signal sender to send second batch of data
            double_send_flag.store(true, Ordering::Release);

            // Wait until sender sends second batch of data
            let second_batch_checkpoint = first_batch_checkpoint + sender_count;
            while send_receive_counter.load(Ordering::Acquire) != second_batch_checkpoint {
                hint::spin_loop();
            }

            // Exit all tasks
            exit_task_flag.store(true, Ordering::Release);

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}

pub struct SPMCBroadcastVariableReceivers;

impl SPMCBroadcastVariableReceivers {
    pub fn prepare_send_trigger_values(active_receiver_count: usize) -> Vec<usize> {
        let mut send_on_read_val = Vec::with_capacity(active_receiver_count);

        send_on_read_val.push(0);
        for i in (1..=active_receiver_count).rev() {
            let last_value = send_on_read_val.last().expect("No value found");
            send_on_read_val.push(last_value + i); // Send after all active receivers read previous batch
        }
        send_on_read_val
    }

    async fn send_task_looped<const SIZE: usize>(
        sender: spmc_broadcast::Sender<u64, SIZE>,
        data_to_send: Vec<u64>,
        read_counter: Arc<AtomicUsize>,
        send_on_read_val: Vec<usize>,
    ) {
        for send_trigger in send_on_read_val {
            while read_counter.load(Ordering::Acquire) != send_trigger {
                hint::spin_loop();
            }
            send_data(&sender, &data_to_send);
        }
    }

    async fn receive_task_looped<const SIZE: usize>(
        name: String,
        mut receiver: spmc_broadcast::Receiver<u64, SIZE>,
        expected_data_count: usize,
        read_counter: Arc<AtomicUsize>,
        loop_count: usize,
    ) {
        for _ in 0..loop_count - 1 {
            receive_data(&name, &mut receiver, expected_data_count).await;
            read_counter.fetch_add(1, Ordering::Release);
        }

        receive_data(&name, &mut receiver, expected_data_count).await;
        drop(receiver); // Make sure that receiver is destroyed before increasing read counter
        read_counter.fetch_add(1, Ordering::Release);
    }
}

impl Scenario for SPMCBroadcastVariableReceivers {
    fn name(&self) -> &str {
        "variable_receivers"
    }

    ///
    /// Runs sending and receiving tasks where receivers are removed during the test.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        const QUEUE_SIZE: usize = 3; // Must be compile constant
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spmc_broadcast::create_channel::<u64, { QUEUE_SIZE }>(logic.max_receiver_count);
        let receivers = prepare_receivers(logic.receivers.len(), receiver);

        let read_counter = Arc::new(AtomicUsize::new(0));

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            let send_on_read_val = Self::prepare_send_trigger_values(receivers.len());
            joiner.add_handle(spawn(Self::send_task_looped(
                sender,
                logic.data_to_send.clone(),
                read_counter.clone(),
                send_on_read_val,
            )));

            let it = receivers.into_iter().zip(logic.receivers);
            for (ndx, (receiver, name)) in it.enumerate() {
                joiner.add_handle(spawn(Self::receive_task_looped(
                    name,
                    receiver,
                    logic.data_to_send.len(),
                    read_counter.clone(),
                    ndx + 1,
                )));
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}

pub struct SPMCBroadcastDropSender;

impl Scenario for SPMCBroadcastDropSender {
    fn name(&self) -> &str {
        "drop_sender"
    }

    ///
    /// Runs sending and receiving tasks where sender is removed during the test.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        let logic = TestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spmc_broadcast::create_channel_default::<u64>(logic.max_receiver_count);
        let receivers = prepare_receivers(logic.receivers.len(), receiver);

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            joiner.add_handle(spawn(send_task(sender, logic.data_to_send.clone())));

            for (receiver, name) in receivers.into_iter().zip(logic.receivers) {
                let receive_data_count = logic.data_to_send.len() + 1; // Expect one more value (None) after sender is dropped
                joiner.add_handle(spawn(receive_task(name, receiver, receive_data_count)));
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct HeavyTestInput {
    send_count: u64,
    receivers: Vec<String>,
    max_receiver_count: u16,
}

impl HeavyTestInput {
    pub fn new(input: &str) -> Self {
        let v: Value = serde_json::from_str(input).expect("Failed to parse input JSON string in HeavyTestInput::new");
        serde_json::from_value(v["test"].clone()).expect("Failed to parse 'test' field from input JSON in HeavyTestInput::new")
    }
}

pub struct SPMCBroadcastHeavyLoad;

impl SPMCBroadcastHeavyLoad {
    fn send_value<const SIZE: usize>(sender: &spmc_broadcast::Sender<u64, SIZE>, value: u64) {
        loop {
            let result = sender.send(&value);
            match result {
                Err(e) => {
                    if e == CommonErrors::GenericError {
                        info!(id = "send_task", iter = value, error = format!("{e:?}"));
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

    async fn heavy_send_task<const SIZE: usize>(sender: spmc_broadcast::Sender<u64, SIZE>, data_count: u64) {
        for value in 0..data_count {
            Self::send_value(&sender, value);
        }
    }

    async fn heavy_receive_task<const SIZE: usize>(name: String, mut receiver: spmc_broadcast::Receiver<u64, SIZE>, expected_data_count: u64) {
        for expected_val in 0..expected_data_count {
            let result = receiver.recv().await;
            match result {
                Some(val) => {
                    if val != expected_val {
                        info!(id = name, iter = expected_val, error = format!("Expected {expected_val}, got {val}"));
                    }
                }
                None => {
                    info!(id = name, iter = expected_val, error = "Provider dropped");
                }
            }
        }
    }
}

impl Scenario for SPMCBroadcastHeavyLoad {
    fn name(&self) -> &str {
        "heavy_load"
    }

    ///
    /// Runs sending and receiving tasks. Validates that all data is sent and received by all consumers.
    /// Reports any errors. One final error is expected for the over-read.
    ///
    fn run(&self, input: &str) -> Result<(), String> {
        const QUEUE_SIZE: usize = 128; // Must be compile constant
        let logic = HeavyTestInput::new(input);
        let mut rt = Runtime::from_json(input)?.build();

        let (sender, receiver) = spmc_broadcast::create_channel::<u64, { QUEUE_SIZE }>(logic.max_receiver_count);
        let receivers = prepare_receivers(logic.receivers.len(), receiver);

        let mut joiner = RuntimeJoiner::new();
        let _ = rt.block_on(async move {
            joiner.add_handle(spawn(Self::heavy_send_task(sender, logic.send_count)));

            for (receiver, name) in receivers.into_iter().zip(logic.receivers) {
                let receive_data_count = logic.send_count + 1;
                joiner.add_handle(spawn(Self::heavy_receive_task(name, receiver, receive_data_count)));
            }

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}
