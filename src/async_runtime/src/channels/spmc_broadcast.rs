//
// Copyright (c) 2025 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// <https://www.apache.org/licenses/LICENSE-2.0>
//
// SPDX-License-Identifier: Apache-2.0
//

use std::{future::Future, marker::PhantomData, sync::Arc, task::Waker};

use foundation::{not_recoverable_error, prelude::*};

use crate::futures::{FutureInternalReturn, FutureState};

const CHANNEL_SIZE: usize = 8;

///
/// Creates Single Producer Multiple Consumer channel. Please keep in mind this is broadcast channel, so all `Receiver`s will receive the same value.
///
pub fn create_channel<T: Copy, const SIZE: usize>(max_num_of_receivers: usize) -> (Sender<T, SIZE>, Receiver<T, SIZE>) {
    assert!(max_num_of_receivers > 0, "Channel size must be greater than 0");
    let chan = Arc::new(Channel::new(max_num_of_receivers));

    (
        Sender {
            chan: chan.clone(),
            _not_sync: PhantomData,
        },
        chan.subscribe().expect("Failed to create initial receiver"),
    )
}

///
/// Creates Single Producer Multiple Consumer channel with [`CHANNEL_SIZE`] capacity. Please keep in mind this is broadcast channel, so all `Receiver`s will receive the same value.
///
pub fn create_channel_default<T: Copy>(max_num_of_receivers: usize) -> (Sender<T, CHANNEL_SIZE>, Receiver<T, CHANNEL_SIZE>) {
    let chan = Arc::new(Channel::new(max_num_of_receivers));

    (
        Sender {
            chan: chan.clone(),
            _not_sync: PhantomData,
        },
        chan.subscribe().expect("Failed to create initial receiver"),
    )
}

pub struct Sender<T: Copy, const SIZE: usize> {
    chan: Arc<Channel<T, SIZE>>,
    _not_sync: PhantomData<*const ()>,
}

unsafe impl<T: Send + Copy, const SIZE: usize> Send for Sender<T, SIZE> {}
unsafe impl<T: Send + Copy, const SIZE: usize> Sync for Sender<T, SIZE> {}

impl<T: Copy, const SIZE: usize> Sender<T, SIZE> {
    ///
    /// Sends a value to all connected `Receiver`s.
    ///
    /// # Returns
    ///
    /// * `Ok()` - When successfully send to all receivers. This may not mean they will read it since ie. `Receiver` can drop in meanwhile
    /// * `Err(CommonErrors::NoSpaceLeft)` - When no more space is available in the channel was accounted at least on single receiver.
    /// * `Err(CommonErrors::GenericError)` - When all `Receiver`s are disconnected (i.e., dropped) was accounted at least on single receiver.
    ///
    pub fn send(&self, value: &T) -> Result<(), CommonErrors> {
        self.chan.send(value)
    }

    ///
    /// Subscribes a new `Receiver` to the channel.
    ///
    /// # Returns
    ///
    /// * `Some(Receiver<T>)` - A new `Receiver` instance if the subscription is successful (max_num_of_receivers was not exhausted yet).
    /// * `None` - If the maximum number of receivers has been reached.
    ///
    pub fn subscribe(&self) -> Option<Receiver<T, SIZE>> {
        self.chan.subscribe()
    }

    ///
    /// Returns the number of receivers that was fetched.
    ///
    pub fn num_of_subscribers(&self) -> usize {
        self.chan.len() as usize
    }
}

impl<T: Copy, const SIZE: usize> Drop for Sender<T, SIZE> {
    fn drop(&mut self) {
        self.chan.sender_dropping();
    }
}

pub struct Receiver<T: Copy, const SIZE: usize> {
    chan: Arc<Channel<T, SIZE>>,
    index: usize,
    _not_sync: PhantomData<*const ()>,
}

unsafe impl<T: Send + Copy, const SIZE: usize> Send for Receiver<T, SIZE> {}

impl<T: Copy, const SIZE: usize> Receiver<T, SIZE> {
    ///
    /// Receives a value from the channel.
    ///
    /// This function waits asynchronously until a value is available or the `Sender` is dropped.
    ///
    /// # Returns
    ///
    /// * `Some(T)` - A value received from the channel.
    /// * `None` - If the `Sender` is dropped and no more values are available.
    ///
    pub async fn recv(&mut self) -> Option<T> {
        ReceiverFuture {
            parent: self,
            state: FutureState::default(),
            consumer: self.chan.get_consumer(self.index),
        }
        .await
    }

    ///
    /// Clones receiver if there is still a slot for new one
    ///
    pub fn try_clone(&self) -> Option<Self> {
        self.chan.subscribe()
    }

    ///
    /// Attempts to receive a value from the channel immediately.
    ///
    /// This function does not block and returns an error if no value is available.
    ///
    ///
    /// # Returns
    ///
    /// * `Ok(T)` - A value received from the channel.
    /// * `Err(CommonErrors::NoData)` - If no value is available in the channel.
    /// * `Err(CommonErrors::GenericError)` - If the `Sender` is dropped and no more values are available.
    ///
    fn receive(&self, consumer: &mut spsc::queue::Consumer<'_, T, SIZE>, waker: Waker) -> Result<T, CommonErrors> {
        self.chan.receive(self.index, consumer, waker)
    }
}

impl<T: Copy, const SIZE: usize> Drop for Receiver<T, SIZE> {
    fn drop(&mut self) {
        self.chan.receiver_dropping(self.index);
    }
}

struct Channel<T: Copy, const SIZE: usize = CHANNEL_SIZE> {
    channels: Vec<super::spsc::Channel<T, SIZE>>,
    next_free_receiver: FoundationAtomicU8,
}

unsafe impl<T: Copy, const SIZE: usize> Sync for Channel<T, SIZE> {}

impl<T: Copy, const SIZE: usize> Channel<T, SIZE> {
    pub fn new(size: usize) -> Self {
        let mut v = Vec::new(size);
        for _ in 0..size {
            v.push(super::spsc::Channel::<T, SIZE>::new());
        }

        Self {
            channels: v,
            next_free_receiver: FoundationAtomicU8::new(0),
        }
    }

    fn len(&self) -> u8 {
        self.next_free_receiver.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn receiver_dropping(&self, index: usize) {
        self.channels[index].receiver_dropping();
    }

    fn sender_dropping(&self) {
        for c in self.channels.as_slice() {
            c.sender_dropping();
        }
    }

    fn send(&self, value: &T) -> Result<(), CommonErrors> {
        let len = self.next_free_receiver.load(std::sync::atomic::Ordering::Relaxed) as usize;
        let mut ret = Ok(());
        let mut i = 0;

        for c in 0..len {
            let r = self.channels[c].send(value);

            match r {
                Ok(_) => {}
                Err(CommonErrors::GenericError) => {
                    i += 1;

                    if i == len {
                        ret = Err(CommonErrors::GenericError)
                    }
                }
                Err(_) => ret = r,
            }
        }

        ret
    }

    fn receive(&self, index: usize, consumer: &mut spsc::queue::Consumer<'_, T, SIZE>, waker: Waker) -> Result<T, CommonErrors> {
        self.channels[index].receive(consumer, waker)
    }

    fn get_consumer(&self, index: usize) -> spsc::queue::Consumer<'_, T, SIZE> {
        self.channels[index].get_queue().acquire_consumer().unwrap()
    }

    fn subscribe(self: &Arc<Self>) -> Option<Receiver<T, SIZE>> {
        let mut curr = self.next_free_receiver.load(std::sync::atomic::Ordering::Relaxed);

        loop {
            if curr >= self.channels.len() as u8 {
                return None; // No more receivers can be created
            }

            match self
                .next_free_receiver
                .compare_exchange(curr, curr + 1, std::sync::atomic::Ordering::AcqRel, std::sync::atomic::Ordering::Acquire)
            {
                Ok(_) => break,
                Err(v) => curr = v,
            }
        }

        Some(Receiver {
            chan: self.clone(),
            index: curr as usize,
            _not_sync: PhantomData,
        })
    }
}

struct ReceiverFuture<'a, T: Copy, const SIZE: usize> {
    parent: &'a Receiver<T, SIZE>,
    consumer: spsc::queue::Consumer<'a, T, SIZE>, // Since future has an explicit lifetime, we can store consumer
    state: FutureState,
}

unsafe impl<T: Copy, const SIZE: usize> Send for ReceiverFuture<'_, T, SIZE> {}

impl<T: Copy, const SIZE: usize> Future for ReceiverFuture<'_, T, SIZE> {
    type Output = Option<T>;

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let res = match self.state {
            FutureState::New | FutureState::Polled => self.parent.receive(&mut self.consumer, cx.waker().clone()).map_or_else(
                |e| {
                    if e == CommonErrors::NoData {
                        FutureInternalReturn::polled()
                    } else {
                        FutureInternalReturn::ready(None)
                    }
                },
                |v| FutureInternalReturn::ready(Some(v)),
            ),
            FutureState::Finished => not_recoverable_error!("Cannot reuse future!"),
        };

        self.state.assign_and_propagate(res)
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use super::*;
    use crate::testing::*;
    use testing::prelude::*;

    #[test]
    fn test_channel_ordered_send_receive_works() {
        let (s, mut r1) = create_channel::<u32, 16>(2);
        let mut r2 = s.subscribe().unwrap();

        let input = vec![1, 2, 10, 3];
        for e in &input {
            assert!(s.send(e).is_ok());
        }

        let cnt = input.len();
        let mut poller1 = TestingFuturePoller::new(async move {
            let mut v = vec![];
            for _ in 0..cnt {
                v.push(r1.recv().await.unwrap());
            }
            v
        });

        let mut poller2 = TestingFuturePoller::new(async move {
            let mut v = vec![];
            for _ in 0..cnt {
                v.push(r2.recv().await.unwrap());
            }
            v
        });

        let (waker, _) = get_dummy_task_waker();
        let res1 = poller1.poll_with_waker(&waker);
        let res2 = poller2.poll_with_waker(&waker);
        assert_poll_ready(res1, input.clone());
        assert_poll_ready(res2, input);
    }

    #[test]
    fn test_channel_max_receivers() {
        let (s, _) = create_channel::<u32, 16>(3);

        let r1 = s.subscribe();
        let r2 = s.subscribe();
        let r3 = s.subscribe(); // Exceeds the maximum number of receivers

        assert!(r1.is_some());
        assert!(r2.is_some());
        assert!(r3.is_none()); // No more receivers can be created
    }

    #[test]
    fn test_channel_receiver_gone() {
        let (s, r1) = create_channel::<u32, 16>(2);
        let mut r2 = s.subscribe().unwrap();

        assert!(s.send(&1).is_ok());
        drop(r1); // Drop one receiver

        {
            let (waker, _) = get_dummy_task_waker();

            let mut poller2 = TestingFuturePoller::new(async move { r2.recv().await.unwrap() });

            let res2 = poller2.poll_with_waker(&waker);
            assert_poll_ready(res2, 1);
        }

        assert!(s.send(&1).is_err());
    }

    #[test]
    fn test_channel_sender_gone() {
        let (s, mut r1) = create_channel::<u32, 16>(2);
        let mut r2 = s.subscribe().unwrap();

        assert!(s.send(&1).is_ok());
        drop(s); // Drop the sender

        {
            let (waker, _) = get_dummy_task_waker();

            let mut poller1 = TestingFuturePoller::new(async move {
                let mut v = vec![];
                for _ in 0..2 {
                    v.push(r1.recv().await);
                }
                v
            });

            let mut poller2 = TestingFuturePoller::new(async move {
                let mut v = vec![];
                for _ in 0..2 {
                    v.push(r2.recv().await);
                }
                v
            });

            let res1 = poller1.poll_with_waker(&waker);
            let res2 = poller2.poll_with_waker(&waker);
            assert_poll_ready(res1, vec![Some(1), None]);
            assert_poll_ready(res2, vec![Some(1), None]);
        }
    }

    #[test]
    fn test_channel_multiple_receivers() {
        let (s, mut r1) = create_channel::<u32, 16>(3);
        let mut r2 = s.subscribe().unwrap();
        let mut r3 = s.subscribe().unwrap();

        assert!(s.send(&1).is_ok());
        assert!(s.send(&2).is_ok());

        let (waker, _) = get_dummy_task_waker();

        let mut poller1 = TestingFuturePoller::new(async move { vec![r1.recv().await.unwrap(), r1.recv().await.unwrap()] });

        let mut poller2 = TestingFuturePoller::new(async move { vec![r2.recv().await.unwrap(), r2.recv().await.unwrap()] });

        let mut poller3 = TestingFuturePoller::new(async move { vec![r3.recv().await.unwrap(), r3.recv().await.unwrap()] });

        let res1 = poller1.poll_with_waker(&waker);
        let res2 = poller2.poll_with_waker(&waker);
        let res3 = poller3.poll_with_waker(&waker);

        assert_poll_ready(res1, vec![1, 2]);
        assert_poll_ready(res2, vec![1, 2]);
        assert_poll_ready(res3, vec![1, 2]);
    }

    #[test]
    #[should_panic]
    fn test_channel_invalid_subscription() {
        let (_, _) = create_channel::<u32, 16>(0); // Invalid size
    }

    #[test]
    fn test_sender_send_to_dropped_receiver() {
        let (s, r1) = create_channel::<u32, 16>(2);
        let mut r2 = s.subscribe().unwrap();

        drop(r1); // Drop one receiver
        assert!(s.send(&1).is_ok()); // Should succeed for the remaining receiver

        let (waker, _) = get_dummy_task_waker();
        let mut poller = TestingFuturePoller::new(async move { r2.recv().await.unwrap() });
        let res = poller.poll_with_waker(&waker);
        assert_poll_ready(res, 1);
    }

    #[test]
    fn test_sender_send_with_no_receivers() {
        let (s, r1) = create_channel::<u32, 16>(1);
        drop(r1); // Drop the only receiver

        assert!(s.send(&1).is_err()); // Should fail as there are no active receivers
    }

    #[test]
    fn test_sender_send_with_partial_receiver_failures() {
        let (s, r1) = create_channel::<u32, 16>(2);
        let mut r2 = s.subscribe().unwrap();

        drop(r1); // Drop one receiver
        assert!(s.send(&1).is_ok()); // Should succeed for the remaining receiver

        let (waker, _) = get_dummy_task_waker();
        let mut poller = TestingFuturePoller::new(async move { r2.recv().await.unwrap() });
        let res = poller.poll_with_waker(&waker);
        assert_poll_ready(res, 1);
    }

    #[test]
    fn test_sender_send_with_maximum_receivers() {
        let (s, mut r1) = create_channel::<u32, 16>(3);
        let mut r2 = s.subscribe().unwrap();
        let mut r3 = s.subscribe().unwrap();

        assert!(s.send(&1).is_ok());
        assert!(s.send(&2).is_ok());

        let (waker, _) = get_dummy_task_waker();

        let mut poller1 = TestingFuturePoller::new(async move { vec![r1.recv().await.unwrap(), r1.recv().await.unwrap()] });
        let mut poller2 = TestingFuturePoller::new(async move { vec![r2.recv().await.unwrap(), r2.recv().await.unwrap()] });
        let mut poller3 = TestingFuturePoller::new(async move { vec![r3.recv().await.unwrap(), r3.recv().await.unwrap()] });

        let res1 = poller1.poll_with_waker(&waker);
        let res2 = poller2.poll_with_waker(&waker);
        let res3 = poller3.poll_with_waker(&waker);

        assert_poll_ready(res1, vec![1, 2]);
        assert_poll_ready(res2, vec![1, 2]);
        assert_poll_ready(res3, vec![1, 2]);
    }

    #[test]
    fn test_sender_send_after_receiver_drops_midway() {
        let (s, r1) = create_channel::<u32, 16>(2);
        let mut r2 = s.subscribe().unwrap();

        assert!(s.send(&1).is_ok());
        drop(r1); // Drop one receiver after the first send

        assert!(s.send(&2).is_ok()); // Should succeed for the remaining receiver

        let (waker, _) = get_dummy_task_waker();
        let mut poller = TestingFuturePoller::new(async move { vec![r2.recv().await.unwrap(), r2.recv().await.unwrap()] });
        let res = poller.poll_with_waker(&waker);
        assert_poll_ready(res, vec![1, 2]);
    }
}

#[cfg(test)]
#[cfg(loom)]
mod tests {

    use super::*;
    use crate::testing::*;
    use testing::prelude::*;

    use loom::model::Builder;

    #[test]
    fn test_broadcast_channel_mt_sender_receiver() {
        let mut builder = Builder::new();
        builder.preemption_bound = Some(2);

        builder.check(|| {
            let (s, mut r) = create_channel::<u32, 16>(2);

            let input = vec![1, 2, 10, 3];

            let handle1 = loom::thread::spawn(move || {
                let mut poller = TestingFuturePoller::new(async move {
                    let mut v = vec![];

                    loop {
                        let r = r.recv().await;

                        if r.is_none() {
                            break;
                        }
                        v.push(r.unwrap());
                    }

                    v
                });

                let sched = create_mock_scheduler_sync();

                loop {
                    let waker = get_dummy_sync_task_waker(sched.clone());

                    let res = poller.poll_with_waker(&waker);
                    if res.is_ready() {
                        return res;
                    }

                    sched.wait_for_wake();
                    loom::hint::spin_loop();
                }
            });

            let mut r2 = s.subscribe().unwrap();
            let handle2 = loom::thread::spawn(move || {
                let mut poller = TestingFuturePoller::new(async move {
                    let mut v = vec![];

                    loop {
                        let r = r2.recv().await;

                        if r.is_none() {
                            break;
                        }
                        v.push(r.unwrap());
                    }

                    v
                });

                // Its copy paste but in loom number of cases, so time grow as hell if we sue loop here.
                // So I did few steps and then either we are done or not, loom will provide all options that we handle in assert
                let sched = create_mock_scheduler_sync();
                let waker = get_dummy_sync_task_waker(sched.clone());

                let res = poller.poll_with_waker(&waker);
                if res.is_ready() {
                    return res;
                }

                sched.wait_for_wake();

                let sched = create_mock_scheduler_sync();
                let waker = get_dummy_sync_task_waker(sched.clone());

                let res = poller.poll_with_waker(&waker);
                if res.is_ready() {
                    return res;
                }

                sched.wait_for_wake();

                let sched = create_mock_scheduler_sync();
                let waker = get_dummy_sync_task_waker(sched.clone());

                let res = poller.poll_with_waker(&waker);
                if res.is_ready() {
                    return res;
                }

                sched.wait_for_wake();

                let sched = create_mock_scheduler_sync();
                let waker = get_dummy_sync_task_waker(sched.clone());

                let res = poller.poll_with_waker(&waker);
                if res.is_ready() {
                    return res;
                }

                sched.wait_for_wake();

                let sched = create_mock_scheduler_sync();
                let waker = get_dummy_sync_task_waker(sched.clone());

                let res = poller.poll_with_waker(&waker);
                if res.is_ready() {
                    return res;
                }

                sched.wait_for_wake();

                let sched = create_mock_scheduler_sync();
                let waker = get_dummy_sync_task_waker(sched.clone());

                let res = poller.poll_with_waker(&waker);
                if res.is_ready() {
                    return res;
                }

                sched.wait_for_wake();

                let sched = create_mock_scheduler_sync();
                let waker = get_dummy_sync_task_waker(sched.clone());

                let res = poller.poll_with_waker(&waker);
                if res.is_ready() {
                    return res;
                }

                sched.wait_for_wake();

                let sched = create_mock_scheduler_sync();
                let waker = get_dummy_sync_task_waker(sched.clone());

                let res = poller.poll_with_waker(&waker);
                if res.is_ready() {
                    return res;
                }

                sched.wait_for_wake();
                res
            });

            for e in &input {
                assert!(s.send(e).is_ok());
            }

            drop(s);

            let res1 = handle1.join().unwrap();
            let res2 = handle2.join().unwrap();

            assert_poll_ready(res1, input.clone());
            match res2 {
                std::task::Poll::Pending => {}
                _ => assert_poll_ready(res2, input.clone()),
            }
        });
    }
}
