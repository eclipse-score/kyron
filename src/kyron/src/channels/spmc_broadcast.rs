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
use std::sync::Arc;

use kyron_foundation::{
    not_recoverable_error,
    prelude::{vector_extension::VectorExtension, *},
};

use crate::futures::{FutureInternalReturn, FutureState};

pub const DEFAULT_CHANNEL_SIZE: usize = 8;

use iceoryx2_bb_elementary::bump_allocator::*;
use iceoryx2_bb_lock_free::mpmc::unique_index_set::*;

///
/// Creates Single Producer Multiple Consumer channel. Please keep in mind this is broadcast channel, so all `Receiver`s will receive the same value.
///
pub fn create_channel<T: Copy, const SIZE: usize>(max_num_of_receivers: u16) -> (Sender<T, SIZE>, Receiver<T, SIZE>) {
    assert!(max_num_of_receivers > 0, "Channel size must be greater than 0");
    let mut chan = Arc::new(Channel::new(max_num_of_receivers as usize));
    // Initialize the channel to set up the UniqueIndexSet after it is placed in an Arc.
    // Since UniqueIndexSet uses self-referential pointers, it must be initialized after being placed in Arc.
    Arc::get_mut(&mut chan).unwrap().init();
    (
        Sender {
            chan: chan.clone(),
            _not_sync: PhantomData,
        },
        chan.subscribe().expect("Failed to create initial receiver"),
    )
}

///
/// Creates Single Producer Multiple Consumer channel with [`DEFAULT_CHANNEL_SIZE`] capacity. Please keep in mind this is broadcast channel, so all `Receiver`s will receive the same value.
///
pub fn create_channel_default<T: Copy>(max_num_of_receivers: u16) -> (Sender<T, DEFAULT_CHANNEL_SIZE>, Receiver<T, DEFAULT_CHANNEL_SIZE>) {
    let mut chan = Arc::new(Channel::new(max_num_of_receivers as usize));
    // Initialize the channel to set up the UniqueIndexSet after it is placed in an Arc.
    // Since UniqueIndexSet uses self-referential pointers, it must be initialized after being placed in Arc.
    Arc::get_mut(&mut chan).unwrap().init();
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
    /// Returns the number of active receivers.
    ///
    pub fn num_of_subscribers(&self) -> usize {
        self.chan.len()
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

struct Channel<T: Copy, const SIZE: usize = DEFAULT_CHANNEL_SIZE> {
    channels: Vec<super::spsc::Channel<T, SIZE>>,
    free_slots: UniqueIndexSet,
    // Memory required for UniqueIndexSet to hold the indices
    memory: Box<[u8]>,
    // Since max receivers is limited to u16 in create channel function, we can use u16 here
    subscriber_index_max: FoundationAtomicU16,
}

unsafe impl<T: Copy, const SIZE: usize> Sync for Channel<T, SIZE> {}

impl<T: Copy, const SIZE: usize> Channel<T, SIZE> {
    pub fn new(size: usize) -> Self {
        let mut v = Vec::new_in_global(size);

        // Temporary adapter to handle 0 case since Vec::new_in_global(0) is not allowed. This shall be fixed later.
        if size != 0 {
            let _ = v.resize_with(size, || super::spsc::Channel::<T, SIZE>::new());
        }

        Self {
            channels: v,
            free_slots: unsafe { UniqueIndexSet::new_uninit(size) },
            memory: {
                let mut slice = Box::<[u8]>::new_uninit_slice(UniqueIndexSet::const_memory_size(size));
                // SAFETY: We are initializing all bytes to zero before assuming init.
                unsafe {
                    ::core::ptr::write_bytes(slice.as_mut_ptr(), 0, slice.len());
                }
                unsafe { slice.assume_init() }
            },
            subscriber_index_max: FoundationAtomicU16::new(0),
        }
    }

    fn init(&mut self) {
        let allocator = BumpAllocator::new(self.memory.as_mut_ptr());
        unsafe {
            self.free_slots
                .init(&allocator)
                .expect("Failed to allocate enough memory for UniqueIndexSet");
        }
    }

    fn len(&self) -> usize {
        self.free_slots.borrowed_indices()
    }

    fn receiver_dropping(&self, index: usize) {
        self.channels[index].receiver_dropping();
        unsafe { self.free_slots.release_raw_index(index as u32, ReleaseMode::Default) };
    }

    fn sender_dropping(&self) {
        for c in self.channels.as_slice() {
            c.sender_dropping();
        }
    }

    fn send(&self, value: &T) -> Result<(), CommonErrors> {
        let len = self.subscriber_index_max.load(FoundationOrdering::Acquire) as usize + 1;
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
        let free_slot = unsafe { self.free_slots.acquire_raw_index() };
        if free_slot.is_err() {
            return None; // No more slots
        }
        let curr = free_slot.unwrap() as u16;
        self.subscriber_index_max.fetch_max(curr, FoundationOrdering::AcqRel);
        // If receiver was dropped and reusing the channel, then we need to enable receiver again
        // If new receiver, then it is already enabled, no harm in calling enable_receiver again
        self.channels[curr as usize].enable_receiver();
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
        // The default receiver is already dropped above, so we can create max_num_of_receivers
        let r1 = s.subscribe();
        let r2 = s.subscribe();
        let r3 = s.subscribe();
        let r4 = s.subscribe(); // Exceeds the maximum number of receivers

        assert!(r1.is_some());
        assert!(r2.is_some());
        assert!(r3.is_some());
        assert!(r4.is_none()); // No more receivers can be created
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

    #[test]
    fn test_reuse_of_unsubscribed_channel() {
        let (s, r1) = create_channel::<u32, 16>(2);
        let mut r2 = s.subscribe().unwrap();
        assert!(s.num_of_subscribers() == 2);
        drop(r1); // Drop one receiver
        assert!(s.num_of_subscribers() == 1);

        assert!(s.send(&1).is_ok()); // Should succeed for the remaining receiver

        let mut r3 = s.subscribe().unwrap(); // Should reuse the slot of the dropped receiver
        assert!(r3.try_clone().is_none()); // No more slots available

        assert!(s.send(&2).is_ok()); // Should succeed for both receivers

        let (waker, _) = get_dummy_task_waker();
        let mut poller2 = TestingFuturePoller::new(async move { vec![r2.recv().await.unwrap(), r2.recv().await.unwrap()] });
        let mut poller3 = TestingFuturePoller::new(async move { vec![r3.recv().await.unwrap()] });

        let res2 = poller2.poll_with_waker(&waker);
        let res3 = poller3.poll_with_waker(&waker);
        assert_poll_ready(res2, vec![1, 2]);
        assert_poll_ready(res3, vec![2]);
    }

    #[test]
    fn test_reuse_of_unsubscribed_channel_discard_old_samples() {
        let (s, r1) = create_channel::<u32, 16>(2);
        let mut r2 = s.subscribe().unwrap();
        assert!(s.num_of_subscribers() == 2);
        // Send 5 samples
        for i in 1..6 {
            assert!(s.send(&i).is_ok());
        }
        drop(r1); // Drop one receiver
        assert!(s.num_of_subscribers() == 1);
        // Send another 5 samples
        for i in 6..11 {
            assert!(s.send(&i).is_ok());
        }
        // subscribe new receiver, should reuse the slot of the dropped receiver
        let mut r3 = s.subscribe().unwrap();

        // Send one sample
        assert!(s.send(&100).is_ok());

        let (waker, _) = get_dummy_task_waker();
        let mut poller2 = TestingFuturePoller::new(async move {
            let mut v = vec![];
            // r2 should receive all 10 samples (1..10) + last one (100)
            for _ in 0..11 {
                v.push(r2.recv().await.unwrap());
            }
            v
        });
        let mut poller3 = TestingFuturePoller::new(async move { vec![r3.recv().await.unwrap()] });

        let res2 = poller2.poll_with_waker(&waker);
        let res3 = poller3.poll_with_waker(&waker);
        assert_poll_ready(res2, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 100]);
        assert_poll_ready(res3, vec![100]); // r3 should only receive the last sample (100)
    }

    #[test]
    fn test_data_not_sent_to_unsubscribed_receiver() {
        let (s, r1) = create_channel::<u32, 8>(2);
        assert!(s.num_of_subscribers() == 1);

        // Fill the channel
        for i in 0..8 {
            assert!(s.send(&i).is_ok());
        }

        // Read one sample from r1
        let (waker, _) = get_dummy_task_waker();
        let res1 = r1.receive(&mut r1.chan.get_consumer(r1.index), waker);
        assert!(res1.is_ok());
        assert_eq!(res1.unwrap(), 0);

        // Call send again, should succeed as r1 has read one sample and no data is sent to unsubscribed receiver
        assert!(s.send(&8).is_ok());
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
                ::core::task::Poll::Pending => {}
                _ => assert_poll_ready(res2, input.clone()),
            }
        });
    }

    #[test]
    fn test_broadcast_channel_multiple_receivers_drop_and_subscribe_again() {
        let mut builder = Builder::new();
        builder.preemption_bound = Some(3);

        builder.check(|| {
            let (s, r1) = create_channel::<u32, 16>(8);
            let r2 = s.subscribe().unwrap();
            let mut r3 = s.subscribe().unwrap();

            // Drop 2 receivers from different threads
            let h1 = loom::thread::spawn(move || drop(r1));
            drop(r2);
            h1.join().unwrap();
            // only one receiver should be left
            assert_eq!(s.num_of_subscribers(), 1);

            let s = Arc::new(s);
            let s1 = s.clone();
            // Subscribe from 2 different threads
            let handle1 = loom::thread::spawn(move || s1.subscribe().unwrap());
            let mut r1 = s.subscribe().unwrap();
            let mut r2 = handle1.join().unwrap();

            // Now we should have 3 receivers
            assert_eq!(s.num_of_subscribers(), 3);

            // Send a value
            assert!(s.send(&42).is_ok());

            // All receivers should get the value
            let (waker, _) = get_dummy_task_waker();
            let w2 = waker.clone();
            let w3 = waker.clone();
            let mut poller1 = TestingFuturePoller::new(async move { r1.recv().await.unwrap() });
            let mut poller2 = TestingFuturePoller::new(async move { r2.recv().await.unwrap() });
            let mut poller3 = TestingFuturePoller::new(async move { r3.recv().await.unwrap() });

            let res1 = poller1.poll_with_waker(&waker);
            let h2 = loom::thread::spawn(move || poller2.poll_with_waker(&w2));
            let h3 = loom::thread::spawn(move || poller3.poll_with_waker(&w3));
            let res2 = h2.join().unwrap();
            let res3 = h3.join().unwrap();

            assert_poll_ready(res1, 42);
            assert_poll_ready(res2, 42);
            assert_poll_ready(res3, 42);
        });
    }
}
