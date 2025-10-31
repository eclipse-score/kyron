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

use ::core::{future::Future, marker::PhantomData, task::Waker};
use kyron_foundation::{not_recoverable_error, prelude::*};
use std::sync::Arc;

use crate::{
    futures::{FutureInternalReturn, FutureState},
    scheduler::waker::AtomicWakerStore,
};

///
/// Creates Single Producer Single Consumer channel with [`DEFAULT_CHANNEL_SIZE`] capacity
///
pub fn create_channel_default<T: Copy>() -> (Sender<T, DEFAULT_CHANNEL_SIZE>, Receiver<T, DEFAULT_CHANNEL_SIZE>) {
    let chan = Arc::new(Channel::new());

    (
        Sender {
            chan: chan.clone(),
            _not_sync: PhantomData,
        },
        Receiver {
            chan,
            _not_sync: PhantomData,
        },
    )
}

///
/// Creates Single Producer Single Consumer channel with `Size` capacity
///
pub fn create_channel<T: Copy, const SIZE: usize>() -> (Sender<T, SIZE>, Receiver<T, SIZE>) {
    let chan = Arc::new(Channel::<T, SIZE>::new());

    (
        Sender {
            chan: chan.clone(),
            _not_sync: PhantomData,
        },
        Receiver {
            chan,
            _not_sync: PhantomData,
        },
    )
}

pub struct Sender<T: Copy, const SIZE: usize> {
    chan: Arc<Channel<T, SIZE>>,
    _not_sync: PhantomData<T>,
}

unsafe impl<T: Send + Copy, const SIZE: usize> Send for Sender<T, SIZE> {}

impl<T: Copy, const SIZE: usize> Sender<T, SIZE> {
    ///
    /// Sends value to connected `Receiver`
    ///
    /// # Returns
    ///
    /// * `Ok(())` - when data was send
    /// * `Err(CommonErrors::NoSpaceLeft)` - when no more space available in channel.
    /// * `Err(CommonErrors::GenericError)` - when `Receiver` is disconnected (ie dropped)
    ///
    pub fn send(&self, value: &T) -> Result<(), CommonErrors> {
        self.chan.send(value)
    }
}

impl<T: Copy, const SIZE: usize> Drop for Sender<T, SIZE> {
    fn drop(&mut self) {
        self.chan.sender_dropping();
    }
}

pub struct Receiver<T: Copy, const SIZE: usize> {
    chan: Arc<Channel<T, SIZE>>,
    _not_sync: PhantomData<*const ()>,
}

impl<T: Copy, const SIZE: usize> Drop for Receiver<T, SIZE> {
    fn drop(&mut self) {
        self.chan.receiver_dropping();
    }
}

unsafe impl<T: Send + Copy, const SIZE: usize> Send for Receiver<T, SIZE> {}

impl<T: Copy, const SIZE: usize> Receiver<T, SIZE> {
    ///
    /// Receive data on a channel
    ///
    ///  # Returns
    ///
    /// * `Some(_)` - when new sample arrived
    /// * `None` - when `Sender` has disconnected and this channel is not usable anymore.
    ///
    pub async fn recv(&mut self) -> Option<T> {
        ReceiverFuture {
            parent: self,
            state: FutureState::default(),
            consumer: self.chan.queue.acquire_consumer().unwrap(),
        }
        .await
    }

    fn receive(&self, consumer: &mut spsc::queue::Consumer<'_, T, SIZE>, waker: Waker) -> Result<T, CommonErrors> {
        self.chan.receive(consumer, waker)
    }
}

pub const DEFAULT_CHANNEL_SIZE: usize = 8;
const BOTH_IN: u8 = 0;
const SENDER_GONE: u8 = 1;
const RECV_GONE: u8 = 2;

pub(super) struct Channel<T: Copy, const SIZE: usize> {
    queue: spsc::queue::Queue<T, SIZE>,
    waker_store: AtomicWakerStore,
    connected_state: FoundationAtomicU8,
}

impl<T: Copy, const SIZE: usize> Channel<T, SIZE> {
    pub(super) fn new() -> Self {
        Self {
            queue: spsc::queue::Queue::new(),
            waker_store: AtomicWakerStore::default(),
            connected_state: FoundationAtomicU8::new(BOTH_IN),
        }
    }

    pub(super) fn sender_dropping(&self) {
        let prev = self.connected_state.swap(SENDER_GONE, ::core::sync::atomic::Ordering::SeqCst);

        if prev == BOTH_IN {
            // if receiver is still there, notify him
            if let Some(waker) = self.waker_store.take() {
                waker.wake();
            }
        }
    }

    pub(super) fn receiver_dropping(&self) {
        self.connected_state.store(RECV_GONE, ::core::sync::atomic::Ordering::SeqCst);
        let _ = self.waker_store.take();
    }

    pub(super) fn get_queue(&self) -> &spsc::queue::Queue<T, SIZE> {
        &self.queue
    }

    ///
    /// Enables receiver again in case it was dropped and we are reusing the channel
    ///
    pub(super) fn enable_receiver(&self) {
        // Load current state
        let state = self.connected_state.load(::core::sync::atomic::Ordering::Acquire);

        // Check if receiver was gone
        if state & RECV_GONE != 0 {
            // Clear any stale data before enabling receiver again
            let mut consumer = self.queue.acquire_consumer().unwrap();
            while consumer.pop().is_some() {}

            // Clear the bit to enable again
            self.connected_state.fetch_and(!RECV_GONE, ::core::sync::atomic::Ordering::AcqRel);
        }
    }

    ///
    /// Safety: Upper layer needs to assure that there is no other `send` caller at the same time, otherwise this will panic
    ///
    pub(crate) fn send(&self, value: &T) -> Result<(), CommonErrors> {
        // if receiver is gone here,
        if self.connected_state.load(::core::sync::atomic::Ordering::Acquire) == RECV_GONE {
            Err(CommonErrors::GenericError)
        } else {
            let res = self.queue.acquire_producer().unwrap().push(value);
            if !res {
                return Err(CommonErrors::NoSpaceLeft);
            }

            // Safety: store makes sure this is not reordered before we push
            if let Some(waker) = self.waker_store.take() {
                waker.wake();
            }

            Ok(())
        }
    }

    ///
    /// Safety: Upper layer needs to assure that there is no other `receive` caller at the same time, otherwise this will panic
    ///
    pub(super) fn receive(&self, consumer: &mut spsc::queue::Consumer<'_, T, SIZE>, waker: Waker) -> Result<T, CommonErrors> {
        loop {
            let empty = self.queue.is_empty();

            if empty {
                // Register current waker
                let old = self.waker_store.swap(Some(waker.clone()));
                let mut res = Err(CommonErrors::NoData);

                if old.is_some() {
                    // We get woken by "Someone" (because Sender did not took a waker), there still may be an item in queue already but we just return like there is none.
                    // If there was pushed data in between, the user will use current waker to notify us and we will pick work next `poll`
                    break res;
                }

                let state = self.connected_state.load(::core::sync::atomic::Ordering::Acquire);

                // we must recheck queue since if waker was empty, producer could already push something before
                if self.queue.is_empty() {
                    if SENDER_GONE == state {
                        //Sender dropped and no items, we are done
                        res = Err(CommonErrors::AlreadyDone);
                        let _ = self.waker_store.take(); // Clear waker.
                    }
                } else {
                    // Let us process items still
                    continue;
                }

                // There is no old waker, maybe we got notified already and the receiver is drop now

                break res;
            } else {
                // There is data already, take a piece, clear a waker if it was set and continue as we don't need to register waker really since Future is completed now
                let _ = self.waker_store.take();
                break Ok(consumer.pop().unwrap());
            }
        }
    }
}

struct ReceiverFuture<'a, T: Copy, const SIZE: usize> {
    parent: &'a Receiver<T, SIZE>,
    consumer: spsc::queue::Consumer<'a, T, SIZE>, // Since future has an explicit lifetime, we can store consumer
    state: FutureState,
}

unsafe impl<const SIZE: usize, T: Copy> Send for ReceiverFuture<'_, T, SIZE> {}

impl<T: Copy, const SIZE: usize> Future for ReceiverFuture<'_, T, SIZE> {
    type Output = Option<T>;

    fn poll(mut self: ::core::pin::Pin<&mut Self>, cx: &mut ::core::task::Context<'_>) -> ::core::task::Poll<Self::Output> {
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
    use kyron_testing::prelude::*;

    #[test]
    fn test_channel_ordered_send_receive_works() {
        let (s, mut r) = create_channel_default::<u32>();

        let input = vec![1, 2, 10, 3];
        for e in &input {
            assert!(s.send(e).is_ok());
        }

        let cnt = input.len();
        let mut poller = TestingFuturePoller::new(async move {
            let mut v = vec![];

            for _ in 0..cnt {
                v.push(r.recv().await.unwrap());
            }

            v
        });

        let (waker, _) = get_dummy_task_waker();
        let res = poller.poll_with_waker(&waker);
        assert_poll_ready(res, input);
    }

    #[test]
    fn test_channel_receiver_gone() {
        {
            let (s, _) = create_channel_default::<u32>();

            assert!(s.send(&1).is_err());
        }

        {
            let (s, mut r) = create_channel_default::<u32>();

            assert!(s.send(&1).is_ok());
            {
                let (waker, _) = get_dummy_task_waker();

                let mut poller = TestingFuturePoller::new(async move { r.recv().await.unwrap() });

                let res = poller.poll_with_waker(&waker);
                assert_poll_ready(res, 1);
            }

            // recv was dropped from above scope
            assert!(s.send(&1).is_err());
        }
    }

    #[test]
    fn test_channel_sender_gone() {
        // sender gone but there is data
        {
            let (s, mut r) = create_channel_default::<u32>();

            assert!(s.send(&1).is_ok());
            drop(s);

            {
                let (waker, _) = get_dummy_task_waker();

                let mut poller = TestingFuturePoller::new(async move {
                    let mut v = vec![];
                    for _ in 0..2 {
                        v.push(r.recv().await);
                    }

                    v
                });

                let res = poller.poll_with_waker(&waker);
                assert_poll_ready(res, vec![Some(1), None]); // data before sender is dropped and then None imidietelly
            }
        }
    }

    #[test]
    fn test_channel_receiver_different_wakes() {
        // Data available at first poll
        {
            let (s, mut r) = create_channel_default::<u32>();
            assert!(s.send(&1).is_ok());

            {
                let mut poller = TestingFuturePoller::new(async move {
                    loop {
                        if let Some(v) = r.recv().await {
                            return v;
                        }
                    }
                });

                let (waker, tracker) = get_dummy_task_waker();

                {
                    // Poll with data shall get value and not store waker
                    let res = poller.poll_with_waker(&waker);
                    assert_poll_ready(res, 1);

                    drop(waker);
                }

                assert_eq!(1, Arc::strong_count(&tracker));
            }
        }

        // No data at first poll, then data
        {
            let (s, mut r) = create_channel_default::<u32>();

            {
                let mut poller = TestingFuturePoller::new(async move {
                    loop {
                        if let Some(v) = r.recv().await {
                            return v;
                        }
                    }
                });

                let (mut waker, tracker) = get_dummy_task_waker();

                {
                    // Poll with data shall not have data and store waker
                    let res = poller.poll_with_waker(&waker);
                    assert!(res.is_pending());
                    drop(waker);
                }

                assert_eq!(2, Arc::strong_count(&tracker));

                // Now set value
                assert!(s.send(&1).is_ok());

                waker = get_waker_from_task(&tracker);

                {
                    // Poll with data shall get value and not store waker
                    let res = poller.poll_with_waker(&waker);
                    assert_poll_ready(res, 1);
                    drop(waker);
                }

                assert_eq!(1, Arc::strong_count(&tracker));
            }
        }
    }

    #[test]
    fn test_channel_send_when_full() {
        let (s, mut r) = create_channel::<u32, 2>();

        // Fill the channel to capacity
        assert!(s.send(&1).is_ok());
        assert!(s.send(&2).is_ok());

        // Attempt to send another value, which should fail
        assert!(s.send(&3).is_err());

        // Consume one value to make space
        let (waker, _) = get_dummy_task_waker();
        let mut poller = TestingFuturePoller::new(async move {
            let ret = r.recv().await.unwrap();
            AlwaysPending {}.await;

            ret
        });
        let _ = poller.poll_with_waker(&waker); // Will return pending

        // Now sending should succeed, as future is keep due to always pending, so 'r' is not dropped
        assert!(s.send(&3).is_ok());
    }

    #[test]
    fn test_channel_reuse_after_receiver_dropped() {
        let (s, mut r) = create_channel_default::<u32>();

        assert!(s.send(&1).is_ok());
        {
            let (waker, _) = get_dummy_task_waker();

            let mut poller = TestingFuturePoller::new(async move { r.recv().await.unwrap() });

            let res = poller.poll_with_waker(&waker);
            assert_poll_ready(res, 1);
        }

        // recv was dropped from above scope
        assert!(s.send(&2).is_err());

        // Create new receiver on same channel
        let mut r2 = Receiver {
            chan: s.chan.clone(),
            _not_sync: PhantomData,
        };
        // Enable receiver again since it was dropped before
        r2.chan.enable_receiver();

        assert!(s.send(&2).is_ok());

        {
            let (waker, _) = get_dummy_task_waker();

            let mut poller = TestingFuturePoller::new(async move { r2.recv().await.unwrap() });

            let res = poller.poll_with_waker(&waker);
            assert_poll_ready(res, 2);
        }
    }
}

#[cfg(test)]
#[cfg(loom)]
mod tests {

    use super::*;
    use crate::testing::*;

    use kyron_testing::prelude::*;

    use loom::model::Builder;

    #[test]
    fn test_channel_mt_sender_receiver() {
        let mut builder = Builder::new();
        builder.preemption_bound = Some(3);

        builder.check(|| {
            let (s, mut r) = create_channel_default::<u32>();

            let input = vec![1, 2, 10, 3];

            let handle = loom::thread::spawn(move || {
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
                        break res;
                    }

                    while !sched.wait_for_wake() {
                        loom::hint::spin_loop();
                    }
                }
            });

            for e in &input {
                assert!(s.send(e).is_ok());
            }

            drop(s);

            let res = handle.join().unwrap();

            assert_poll_ready(res, input);
        });
    }

    #[test]
    fn test_channel_mt_sender_receiver_single_message() {
        let builder = Builder::new();

        builder.check(|| {
            let (s, mut r) = create_channel_default::<u32>();

            let input = vec![1];

            let handle = loom::thread::spawn(move || {
                let mut poller = TestingFuturePoller::new(async move {
                    let mut v = vec![];

                    // Receive only once, to check races around empty state without need to drop sender which may hide bugs
                    let r = r.recv().await;
                    v.push(r.unwrap());

                    v
                });

                let sched = create_mock_scheduler_sync();

                loop {
                    let waker = get_dummy_sync_task_waker(sched.clone());

                    let res = poller.poll_with_waker(&waker);
                    if res.is_ready() {
                        break res;
                    }

                    while !sched.wait_for_wake() {
                        loom::hint::spin_loop();
                    }
                }
            });

            for e in &input {
                assert!(s.send(e).is_ok());
            }

            // No drop before joining to not wake-up receiver from drop
            let res = handle.join().unwrap();
            assert_poll_ready(res, input);
        });
    }
}
