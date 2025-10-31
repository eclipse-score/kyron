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

// TODO: To be removed once used in IO APIs
#![allow(dead_code)]

use core::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    sync::atomic::Ordering,
    task::{Context, Poll, Waker},
};
use std::sync::{Arc, Mutex};

use kyron_foundation::{
    cell::UnsafeCell,
    containers::intrusive_linked_list,
    not_recoverable_error,
    prelude::{debug, error, trace, CommonErrors, FoundationAtomicU32},
};

use crate::{
    futures::{FutureInternalReturn, FutureState},
    io::{driver::IoDriverHandle, AsyncSelector},
    mio::types::{IoEvent, IoEventInterest, IoId, IoRegistryEntry, IoSelector},
    scheduler::context::ctx_get_drivers,
};

// Takes care about tracking all tasks to be waken, and is shared between core reactor that calls wakes + other interested entities, ie BridgedFd
pub(crate) struct AsyncRegistration<S: IoSelector = AsyncSelector> {
    // Handle that is able to register/deregister IO sources in the driver
    handle: IoDriverHandle<S>,

    // Shared between AsyncRegistration and IoDriver so both can access it (here when checking/requesting readiness, in IoDriver when waking up tasks)
    inner: Arc<RegistrationInfo>,
}

impl AsyncRegistration<AsyncSelector> {
    pub(crate) fn new<T: IoRegistryEntry<AsyncSelector> + core::fmt::Debug>(mio: &mut T) -> Result<Self, CommonErrors> {
        Self::new_with_interest(mio, IoEventInterest::READABLE | IoEventInterest::WRITABLE)
    }

    pub(crate) fn new_with_interest<T: IoRegistryEntry<AsyncSelector> + core::fmt::Debug>(
        mio: &mut T,
        interest: IoEventInterest,
    ) -> Result<Self, CommonErrors> {
        Self::new_internal(mio, interest, ctx_get_drivers().get_io_driver().handle())
    }
}

impl<S: IoSelector> AsyncRegistration<S> {
    pub(crate) fn drop_registration<T: IoRegistryEntry<S> + core::fmt::Debug>(&self, mio: &mut T) {
        // Deregister the source from the driver
        self.handle.remove_io_source(mio, &self.inner);
    }

    /// Async interface to await for readiness for the given interest. User will be waken once the interest is ready.
    pub(crate) async fn request_readiness(&self, interest: IoEventInterest) -> Result<ReadinessState, CommonErrors> {
        let readiness = ReadinessFuture::new(self, interest);
        Ok(readiness.await)
    }

    /// Clear readiness to indicate that user consumed the readiness for the given interest. This is required to be able again
    /// await on request_readiness/poll_readiness correctly.
    ///
    /// `readiness` shall match the one returned from request_readiness/poll_readiness
    pub(crate) fn clear_readiness(&self, readiness: ReadinessState, interest: IoEventInterest) -> bool {
        self.inner.clear_readiness(readiness, interest)
    }

    /// Same as `request_readiness` but in polling form. This is useful for implementing `AsyncRead` and `AsyncWrite` traits.
    pub(crate) fn poll_readiness(&self, cx: &mut Context<'_>, interest: IoEventInterest) -> Poll<ReadinessState> {
        self.inner.poll_register_interest(interest, cx).into()
    }

    fn new_internal<T: IoRegistryEntry<S> + core::fmt::Debug>(
        mio: &mut T,
        interest: IoEventInterest,
        handle: IoDriverHandle<S>,
    ) -> Result<Self, CommonErrors> {
        let info = handle.add_io_source(mio, interest)?;
        debug!("AsyncRegistration: Connecting {:?} with MIO object {:?}", info.tracing_id(), mio);

        Ok(AsyncRegistration { inner: info, handle })
    }
}

// Private part

// Internal struct that is shared between AsyncRegistration and IoDriver to dispatch notifications from it
pub(crate) struct RegistrationInfo {
    // Used to track entry in the tracking map in IO Driver
    tracking_key: usize,

    // Used to track wakers that are waiting for readiness changes
    wakers: Mutex<WakersCollection>,

    // Tracks the readiness state of the registration, either writable, readable or both. Probably more data needs to put here as markers
    readiness_state: FoundationAtomicU32,
}

#[derive(Default)]
struct WakersCollection {
    // For Async trait there can be only one of them at a time and upper layers needs to ensure it
    read: Option<Waker>,  // Slot used for AsyncRead, so polling API where there is no way to store anywhere waker
    write: Option<Waker>, // Slot used for AsyncWrite, so polling API where there is no way to store anywhere waker

    // This is used for async API directly over ReadinessFuture which can hold a waker inside itself so we can put it into intrusive list.
    // This is the way to implement multi read/writer use case, ie from UDPSocket
    waiters: intrusive_linked_list::List<WakerElems>, // This is used for async API, ie ReadinessFuture, where we can store wakers in intrusive list
}

// This will be kept in LinkedList and in Future handling awaits
struct WakerElems {
    waker: Option<Waker>,
    list: intrusive_linked_list::Link,
    interest: IoEventInterest,
}

unsafe impl intrusive_linked_list::Item for WakerElems {
    fn link_field_offset() -> usize {
        core::mem::offset_of!(WakerElems, list)
    }
}

impl RegistrationInfo {
    pub(crate) fn new(key: usize) -> Self {
        RegistrationInfo {
            readiness_state: FoundationAtomicU32::new(0),
            tracking_key: key,
            wakers: Mutex::new(WakersCollection::default()),
        }
    }

    pub(crate) fn tracking_key(&self) -> Option<usize> {
        let res = self.tracking_key;
        if res == usize::MAX {
            None
        } else {
            Some(res)
        }
    }

    pub(crate) fn identifier(self: &Arc<Self>) -> IoId {
        let ptr = Arc::as_ptr(self);
        let ptr_addr = ptr as u64;
        IoId::new(ptr_addr)
    }

    // RegistrationInfo is used within Arc so this will be really unique identifier for this registration
    pub(crate) fn tracing_id(&self) -> TracingId<Self> {
        TracingId(self as *const _)
    }

    /// Wake API is exposed towards IO driver so it can wake up tasks that are waiting for IO events on this registration
    pub(super) fn wake(&self, state: ReadinessState) {
        // Mark state so before wakeups it's correct, tasks will check it
        let (_, readiness) = self.mark_state(state);

        self.wake_all(readiness);
    }

    // Create info from identifier assuming that lifetime of RegistrationInfo in AsyncRegistration is kept by the caller.
    // # SAFETY
    //  Not keeping lifetime of RegistrationInfo in AsyncRegistration will lead to undefined behavior.
    pub(super) unsafe fn from_identifier(id: IoId) -> *const RegistrationInfo {
        id.as_u64() as *const RegistrationInfo
    }

    // Private part

    // Create union from current state and `state` and returns (previous state, new state)
    fn mark_state(&self, state: ReadinessState) -> (ReadinessState, ReadinessState) {
        let mut new_state = state;

        let old = self.set_readiness(
            |prev| {
                new_state = prev.union(state);
                Some(new_state)
            },
            TickOp::Set,
        );

        (old, new_state)
    }

    fn wake_all(&self, readiness: ReadinessState) {
        const MAX_WAKEUPS: usize = 6; // Limit the number of wakers to wake up out of lock as we cannot allocate here

        //TODO: FixedSizeVec once miri issues are fixed there
        let mut offloader = [const { Option::<Waker>::None }; MAX_WAKEUPS];
        let mut iter = offloader.iter_mut();

        let mut wakers = self.wakers.lock().unwrap();
        trace!("{:?}: Waking up wakers for readiness: {:?}", self.tracing_id(), readiness);

        if readiness.is_readable() {
            if let Some(waker) = wakers.read.take() {
                *iter.next().unwrap() = Some(waker);
            }
        }

        if readiness.is_writable() {
            if let Some(waker) = wakers.write.take() {
                *iter.next().unwrap() = Some(waker);
            }
        }

        wakers.waiters.remove_if(|elem| {
            if readiness.intersection(elem.interest.into()).is_some() {
                let waker_opt = elem.waker.as_ref().unwrap();

                if let Some(slot) = iter.next() {
                    *slot = Some(waker_opt.clone());
                } else {
                    waker_opt.wake_by_ref();
                }

                true
            } else {
                false
            }
        });

        drop(wakers);

        // Notify out of lock
        for e in offloader.into_iter().take_while(|e| e.is_some()) {
            e.unwrap().wake();
        }
    }

    // Try clear readiness for interest, returns true if it was cleared or false if it was not set (potentially someone else cleared it)
    // `readiness` shall match one coming from request_readiness to correctly clear it.
    fn clear_readiness(&self, readiness: ReadinessState, interest: IoEventInterest) -> bool {
        let mut was_cleared = false;
        self.set_readiness(
            |mut state| {
                let mask = interest.into();

                // If no intersection, nothing to clear
                state.intersection(mask)?;

                state.clear(mask);

                was_cleared = true;
                Some(state)
            },
            TickOp::Clear(readiness.tick()),
        );

        was_cleared
    }

    // Returns `Some(ReadinessState)` if the current state matches the interest (can also contain more states), otherwise `None`.
    fn is_ready(&self, interest: IoEventInterest) -> Option<ReadinessState> {
        let state = self.readiness_state.load(Ordering::SeqCst);
        let readiness = ReadinessState::from(state);

        readiness.intersection(interest.into())
    }

    fn poll_register_interest(&self, interest: IoEventInterest, cx: &mut Context<'_>) -> FutureInternalReturn<ReadinessState> {
        let mut ready = self.is_ready(interest);

        // If we have something ready, we can return it immediately
        if let Some(readiness) = ready {
            return FutureInternalReturn::ready(readiness);
        }

        // No ready, we need to take a lock
        let mut wakers = self.wakers.lock().unwrap();

        // In this PR we only care about single wakers, lets choose one
        let waker = if interest.is_readable() { &mut wakers.read } else { &mut wakers.write };

        // Lets recheck if we are not ready while holding a lock
        ready = self.is_ready(interest);

        // If we have something ready, we can return it immediately
        if let Some(readiness) = ready {
            return FutureInternalReturn::ready(readiness);
        }

        waker.get_or_insert_with(|| cx.waker().clone()).clone_from(cx.waker());

        FutureInternalReturn::polled()
    }

    // Internal proxy handling all details of setting readiness state
    fn set_readiness<F>(&self, mut f: F, tick_op: TickOp) -> ReadinessState
    where
        F: FnMut(ReadinessState) -> Option<ReadinessState>,
    {
        match self.readiness_state.fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
            let mut typed_state = ReadinessState::from(state);

            match tick_op {
                TickOp::Clear(tick) if tick != typed_state.tick() => return None, // We are not allowed to clear this tick as we would clear readiness for older state than it's now here.

                TickOp::Clear(_) => {
                    // Nothing to do, user will apply its closure
                }
                TickOp::Set => {
                    typed_state.increment_tick(); // Just increment the tick as new states comes
                }
            }

            // TICKs are private, user does not touch it
            f(typed_state).map(Into::<u32>::into)
        }) {
            Ok(v) => Into::<ReadinessState>::into(v),
            Err(v) => Into::<ReadinessState>::into(v),
        }
    }
}

enum TickOp {
    Clear(u16), // Request to clear readiness for a given tick
    Set,        // Will advance the tick
}

const READINESS_STATE_READABLE: u32 = 0b0000_0001;
const READINESS_STATE_WRITABLE: u32 = 0b0000_0010;

/// This represents particular AsyncIO readiness state and also a TICK that tells us from which IO Driver run this state comes from.
/// This is used to later be able to check whether user wants to clear the state that he  consumed with readiness or maybe there is already a new state that he shall not clear
/// and consume again (ie. new event on socket is already there)
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ReadinessState(u32); // u16 for TICK and u16 for state

const READINESS_STATE_MASK: u32 = 0x0000FFFF; // Mask for the state bits
const READINESS_TICK_MASK: u32 = 0xFFFF0000; // Mask for the tick bits
const READINESS_TICK_SHIFT: u32 = READINESS_TICK_MASK.count_ones();

impl ReadinessState {
    #[inline]
    pub fn tick(&self) -> u16 {
        self.extract_tick()
    }

    #[inline]
    pub fn is_readable(&self) -> bool {
        self.0 & READINESS_STATE_READABLE != 0
    }

    #[inline]
    pub fn is_writable(&self) -> bool {
        self.0 & READINESS_STATE_WRITABLE != 0
    }

    #[inline]
    pub fn clear_readable(&mut self) {
        self.0 &= !READINESS_STATE_READABLE;
    }

    #[inline]
    pub fn clear_writable(&mut self) {
        self.0 &= !READINESS_STATE_WRITABLE;
    }

    pub fn from_mio_event(event: &IoEvent) -> Self {
        let mut state = 0;
        if event.is_readable() {
            state |= READINESS_STATE_READABLE;
        }
        if event.is_writable() {
            state |= READINESS_STATE_WRITABLE;
        }

        ReadinessState::from_components(state as u16, 0)
    }

    // Removes the states specified by the mask from the current state.
    pub fn clear(&mut self, mask: Self) {
        let new_state = self.extract_state() & !mask.extract_state(); //TODO CHECK IF CORRECT
        *self = Self::from_components(new_state, self.extract_tick())
    }

    pub fn union(&self, other: Self) -> Self {
        let new_state = self.extract_state() | other.extract_state();
        Self::from_components(new_state, self.extract_tick())
    }

    pub fn intersection(&self, other: Self) -> Option<Self> {
        let val = self.extract_state() & other.extract_state();
        if val == 0 {
            None
        } else {
            Some(Self::from_components(val, self.extract_tick()))
        }
    }

    #[inline]
    fn increment_tick(&mut self) {
        *self = Self::from_components(self.extract_state(), self.tick().wrapping_add(1));
    }

    #[inline]
    fn from_components(state: u16, tick: u16) -> Self {
        ReadinessState(((tick as u32) << READINESS_TICK_SHIFT) | (state as u32))
    }

    #[inline]
    fn extract_state(&self) -> u16 {
        (self.0 & READINESS_STATE_MASK) as u16
    }

    #[inline]
    fn extract_tick(&self) -> u16 {
        ((self.0 & READINESS_TICK_MASK) >> READINESS_TICK_SHIFT) as u16
    }
}

impl core::fmt::Debug for ReadinessState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ReadinessState(s: 0b{:b}, t: {})", self.extract_state(), self.extract_tick())
    }
}

impl From<ReadinessState> for u32 {
    fn from(value: ReadinessState) -> Self {
        value.0
    }
}

impl From<u32> for ReadinessState {
    fn from(value: u32) -> Self {
        ReadinessState(value)
    }
}

impl From<IoEventInterest> for ReadinessState {
    fn from(interest: IoEventInterest) -> Self {
        let mut state: u32 = 0;
        if interest.is_readable() {
            state |= READINESS_STATE_READABLE;
        }
        if interest.is_writable() {
            state |= READINESS_STATE_WRITABLE;
        }
        ReadinessState::from_components(state as u16, 0)
    }
}

pub(crate) struct TracingId<T>(*const T);

impl core::fmt::Debug for TracingId<RegistrationInfo> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RegistrationInfo({:p})", self.0)
    }
}

struct ReadinessFuture<'a, S: IoSelector = AsyncSelector> {
    registration: &'a AsyncRegistration<S>,
    state: RefCell<FutureState>,

    // This entry is kept in intrusive list. Lifetime is assured by ReadinessFuture and it's corelation with 'a AsyncRegistration
    list_item: UnsafeCell<WakerElems>,
    interest: IoEventInterest, // Copy for local usage without lock
}

impl<'a, S: IoSelector> ReadinessFuture<'a, S> {
    fn new(registration: &'a AsyncRegistration<S>, interest: IoEventInterest) -> Self {
        ReadinessFuture {
            registration,
            list_item: UnsafeCell::new(WakerElems {
                waker: None,
                list: intrusive_linked_list::Link::default(),
                interest,
            }),
            state: RefCell::new(FutureState::New),
            interest,
        }
    }
}

impl<S: IoSelector> Future for ReadinessFuture<'_, S> {
    type Output = ReadinessState;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let state = match *self.state.borrow() {
            FutureState::New => {
                // It's first call so we are not in list yet, we can easilly take a reference to the list item
                self.list_item.with_mut(|item| {
                    // This makes sure that there's only one reference to item:
                    let item = unsafe { &mut *item };

                    match self.registration.inner.is_ready(item.interest) {
                        Some(readiness) => FutureInternalReturn::ready(readiness),
                        _ => {
                            // Now we need to register as it's not ready

                            let mut wakers = self.registration.inner.wakers.lock().unwrap();

                            // Recheck under lock
                            if let Some(readiness) = self.registration.inner.is_ready(item.interest) {
                                FutureInternalReturn::ready(readiness)
                            } else {
                                item.waker = Some(cx.waker().clone());

                                unsafe {
                                    wakers.waiters.push_back(item);
                                }

                                FutureInternalReturn::polled()
                            }
                        }
                    }
                })
            }
            FutureState::Polled => match self.registration.inner.is_ready(self.interest) {
                Some(readiness) => {
                    let mut wakers = self.registration.inner.wakers.lock().unwrap();

                    self.list_item.with(|item| {
                        wakers.waiters.remove(unsafe { &*item });
                    });

                    FutureInternalReturn::ready(readiness)
                }
                _ => FutureInternalReturn::polled(),
            },
            FutureState::Finished => {
                not_recoverable_error!(with self.registration.inner.tracing_id(), "Shall not poll finished future for ReadinessFuture")
            }
        };

        self.state.borrow_mut().assign_and_propagate(state)
    }
}

impl<S: IoSelector> Drop for ReadinessFuture<'_, S> {
    fn drop(&mut self) {
        self.list_item.with(|item| {
            self.registration.inner.wakers.lock().unwrap().waiters.remove(unsafe { &*item });
        });
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use core::time::Duration;
    use std::os::fd::AsRawFd;

    use kyron_testing::{build_with_location, prelude::MockFnBuilder};

    use crate::{
        io::driver::Registrations,
        mio::{
            registry::Registry,
            types::{IoCall, IoSelector},
        },
        testing::MockWaker,
    };

    use super::*;

    // RegistrationInfo

    #[test]
    fn registration_info_() {
        let _info = Arc::new(RegistrationInfo::new(12));
    }

    #[test]
    fn registration_info_tracing_id() {
        let info = RegistrationInfo::new(12);
        let id = info.tracing_id();
        assert_eq!(format!("{:?}", id), format!("RegistrationInfo({:p})", &info));
    }

    #[test]
    fn registration_info_tracking_key() {
        {
            let info = Arc::new(RegistrationInfo::new(12));
            assert_eq!(info.tracking_key(), Some(12));
        }
    }

    #[test]
    fn registration_info_clear_readiness() {
        // Nothing to clear (as no common intersection), should return false
        {
            let info = RegistrationInfo::new(12);
            assert!(!info.clear_readiness(
                ReadinessState::from_components(READINESS_STATE_READABLE as u16, 0),
                IoEventInterest::READABLE
            ));
        }

        // Set means shall be cleared since tick is from same round
        {
            let info = RegistrationInfo::new(12);
            info.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 1));

            assert!(info.clear_readiness(
                ReadinessState::from_components(READINESS_STATE_READABLE as u16, 1),
                IoEventInterest::READABLE
            ));
        }

        // Set but tick is from old round, shall not clear
        {
            let info = RegistrationInfo::new(12);
            info.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 1));

            assert!(!info.clear_readiness(
                ReadinessState::from_components(READINESS_STATE_READABLE as u16, 0),
                IoEventInterest::READABLE
            ));
        }
    }

    #[test]
    fn registration_info_is_ready() {
        {
            let info = RegistrationInfo::new(12);
            assert!(info.is_ready(IoEventInterest::READABLE).is_none());
        }

        {
            let info = RegistrationInfo::new(12);
            info.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 1));

            let res = info.is_ready(IoEventInterest::READABLE);

            assert!(res.is_some());
            assert!(res.unwrap().is_readable());
        }

        {
            let info = RegistrationInfo::new(12);
            info.wake(ReadinessState::from_components(READINESS_STATE_WRITABLE as u16, 1));
            let res = info.is_ready(IoEventInterest::WRITABLE);

            assert!(res.is_some());
            assert!(res.unwrap().is_writable());
        }
    }

    #[test]
    fn registration_info_poll_wakes() {
        let non_call_waker = MockWaker::new(build_with_location!(MockFnBuilder::new().times(0))).into_arc();

        {
            let info = Arc::new(RegistrationInfo::new(12));

            let mock_fn = build_with_location!(MockFnBuilder::new().times(1));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            info.wakers.lock().unwrap().write = Some(Waker::from(waker_mock.clone()));
            info.wakers.lock().unwrap().read = Some(Waker::from(non_call_waker.clone()));

            info.wake(ReadinessState::from_components(READINESS_STATE_WRITABLE as u16, 0));
        }

        {
            let info = Arc::new(RegistrationInfo::new(12));

            let mock_fn = build_with_location!(MockFnBuilder::new().times(1));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            info.wakers.lock().unwrap().write = Some(Waker::from(non_call_waker.clone()));
            info.wakers.lock().unwrap().read = Some(Waker::from(waker_mock.clone()));

            info.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 0));
        }

        {
            let info = Arc::new(RegistrationInfo::new(12));

            let mock_fn = build_with_location!(MockFnBuilder::new().times(1));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            let read_waker = MockWaker::new(build_with_location!(MockFnBuilder::new().times(1))).into_arc();

            info.wakers.lock().unwrap().write = Some(Waker::from(waker_mock.clone()));
            info.wakers.lock().unwrap().read = Some(Waker::from(read_waker.clone()));

            info.wake(ReadinessState::from_components(
                (READINESS_STATE_READABLE | READINESS_STATE_WRITABLE) as u16,
                0,
            ));
        }

        {
            let info = Arc::new(RegistrationInfo::new(12));

            let mock_fn = build_with_location!(MockFnBuilder::new().times(0));
            let waker_mock = MockWaker::new(mock_fn).into_arc();
            let read_waker = MockWaker::new(build_with_location!(MockFnBuilder::new().times(1))).into_arc();

            info.wakers.lock().unwrap().write = Some(Waker::from(waker_mock.clone()));
            info.wakers.lock().unwrap().read = Some(Waker::from(read_waker.clone()));

            info.wake(ReadinessState::from_components((READINESS_STATE_READABLE) as u16, 0));
        }
    }

    #[test]
    fn registration_info_async_wakes() {
        let elem1_readable = WakerElems {
            waker: Some(Waker::from(
                MockWaker::new(build_with_location!(MockFnBuilder::new().times(1))).into_arc(),
            )),
            list: intrusive_linked_list::Link::default(),
            interest: IoEventInterest::READABLE,
        };

        let elem2_readable = WakerElems {
            waker: Some(Waker::from(
                MockWaker::new(build_with_location!(MockFnBuilder::new().times(1))).into_arc(),
            )),
            list: intrusive_linked_list::Link::default(),
            interest: IoEventInterest::READABLE,
        };

        let elem3_writable = WakerElems {
            waker: Some(Waker::from(
                MockWaker::new(build_with_location!(MockFnBuilder::new().times(1))).into_arc(),
            )),
            list: intrusive_linked_list::Link::default(),
            interest: IoEventInterest::WRITABLE,
        };

        let elem4_writable = WakerElems {
            waker: Some(Waker::from(
                MockWaker::new(build_with_location!(MockFnBuilder::new().times(1))).into_arc(),
            )),
            list: intrusive_linked_list::Link::default(),
            interest: IoEventInterest::WRITABLE,
        };

        let elem5_both = WakerElems {
            waker: Some(Waker::from(
                MockWaker::new(build_with_location!(MockFnBuilder::new().times(1))).into_arc(),
            )),
            list: intrusive_linked_list::Link::default(),
            interest: IoEventInterest::READABLE | IoEventInterest::WRITABLE,
        };

        let info = Arc::new(RegistrationInfo::new(12));

        unsafe {
            info.wakers.lock().unwrap().waiters.push_back(&elem1_readable);
            info.wakers.lock().unwrap().waiters.push_back(&elem2_readable);
            info.wakers.lock().unwrap().waiters.push_back(&elem3_writable);
            info.wakers.lock().unwrap().waiters.push_back(&elem4_writable);
            info.wakers.lock().unwrap().waiters.push_back(&elem5_both);
        }

        info.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 0));

        drop(elem1_readable);
        drop(elem2_readable);
        drop(elem5_both);

        info.wake(ReadinessState::from_components(READINESS_STATE_WRITABLE as u16, 0));
    }

    //Async registration
    pub struct ProxyMock<T: AsRawFd> {
        data: T,
        _phantom: core::marker::PhantomData<T>,
    }

    impl<T: AsRawFd> IoRegistryEntry<AsyncSelectorMock> for ProxyMock<T> {
        fn register(&mut self, _registry: &Registry<AsyncSelectorMock>, _id: IoId, _interest: IoEventInterest) -> crate::mio::types::Result<()> {
            Ok(())
        }

        fn reregister(&mut self, __id: IoId, _interest: IoEventInterest) -> crate::mio::types::Result<()> {
            Ok(())
        }

        fn deregister(&mut self) -> crate::mio::types::Result<()> {
            Ok(())
        }
    }

    impl<T: AsRawFd> IoCall<T> for ProxyMock<T> {
        fn io_call<C, R>(&self, f: C) -> std::io::Result<R>
        where
            C: FnOnce(&T) -> std::io::Result<R>,
        {
            f(&self.data)
        }

        fn new(source: T) -> Self {
            Self {
                data: source,
                _phantom: core::marker::PhantomData,
            }
        }

        fn as_inner(&self) -> &T {
            &self.data
        }

        fn as_inner_mut(&mut self) -> &mut T {
            &mut self.data
        }
    }

    pub struct AsyncSelectorMock {}

    impl IoSelector for AsyncSelectorMock {
        type IoProxy<T: AsRawFd> = ProxyMock<T>;

        type Waker = ();

        fn select<Container: crate::mio::types::IoSelectorEventContainer>(
            &self,
            _events: &mut Container,
            _timeout: Option<Duration>,
        ) -> crate::mio::types::Result<()> {
            Ok(())
        }

        fn register(&self, _fd: std::os::unix::prelude::RawFd, _id: IoId, _interest: IoEventInterest) -> crate::mio::types::Result<()> {
            Ok(())
        }

        fn reregister(&self, _fd: std::os::unix::prelude::RawFd, _id: IoId, _interest: IoEventInterest) -> crate::mio::types::Result<()> {
            Ok(())
        }

        fn deregister(&self, _fd: std::os::unix::prelude::RawFd) -> crate::mio::types::Result<()> {
            Ok(())
        }

        fn create_waker(&self, _id: IoId) -> crate::mio::types::Result<Self::Waker> {
            Ok(())
        }

        fn capacity(&self) -> usize {
            1024
        } // For simplicity, we use the type itself as the proxy
    }

    #[derive(Clone, Debug)]
    pub struct IoMock {}
    impl IoRegistryEntry<AsyncSelectorMock> for IoMock {
        fn register(&mut self, _registry: &Registry<AsyncSelectorMock>, _id: IoId, _interest: IoEventInterest) -> crate::mio::types::Result<()> {
            Ok(())
        }

        fn reregister(&mut self, _id: IoId, _interest: IoEventInterest) -> crate::mio::types::Result<()> {
            Ok(())
        }

        fn deregister(&mut self) -> crate::mio::types::Result<()> {
            Ok(())
        }
    }

    fn create_registration(interest: IoEventInterest) -> Arc<AsyncRegistration<AsyncSelectorMock>> {
        let handle = IoDriverHandle::<AsyncSelectorMock>::new(Registry::new(AsyncSelectorMock {}), Arc::new(Registrations::new(1024)));
        let mut mio = IoMock {};
        Arc::new(AsyncRegistration::new_internal(&mut mio, interest, handle).unwrap())
    }

    fn run_request_readiness(
        reg: &Arc<AsyncRegistration<AsyncSelectorMock>>,
        interest: IoEventInterest,
    ) -> Arc<Mutex<Result<ReadinessState, CommonErrors>>> {
        let result = Arc::new(Mutex::new(Err(CommonErrors::GenericError)));
        let result_clone = result.clone();
        let reg = reg.clone();
        crate::testing::mock::spawn(async move {
            let ret = reg.request_readiness(interest).await;
            *result_clone.lock().unwrap() = ret;
        });

        result
    }

    #[test]
    fn async_registration_request_readiness_writable_works() {
        crate::testing::mock::runtime::init();

        // Writable readiness
        {
            let reg = create_registration(IoEventInterest::WRITABLE);
            let result = run_request_readiness(&reg, IoEventInterest::WRITABLE);

            crate::testing::mock::runtime::step();
            assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // We still have task not done

            reg.inner.wake(ReadinessState::from_components(READINESS_STATE_WRITABLE as u16, 0)); // Make task ready
            crate::testing::mock::runtime::step();

            let ret_val = *result.lock().unwrap();
            assert!(ret_val.is_ok(), "Expected Ok result, got: {:?}", ret_val);
            assert!(ret_val.unwrap().is_writable(), "Expected writable state, got: {:?}", ret_val.unwrap());
        }

        // Readable readiness
        {
            let reg = create_registration(IoEventInterest::READABLE);
            let result = run_request_readiness(&reg, IoEventInterest::READABLE);

            crate::testing::mock::runtime::step();
            assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // We still have task not done

            reg.inner.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 0)); // Make task ready
            crate::testing::mock::runtime::step();

            let ret_val = *result.lock().unwrap();
            assert!(ret_val.is_ok(), "Expected Ok result, got: {:?}", ret_val);
            assert!(ret_val.unwrap().is_readable(), "Expected readable state, got: {:?}", ret_val.unwrap());
        }

        // Multi readiness but readable
        {
            let reg = create_registration(IoEventInterest::READABLE);
            let result = run_request_readiness(&reg, IoEventInterest::READABLE);

            crate::testing::mock::runtime::step();
            assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // We still have task not done

            reg.inner.wake(ReadinessState::from_components(
                (READINESS_STATE_READABLE | READINESS_STATE_WRITABLE) as u16,
                0,
            )); // Make task ready
            crate::testing::mock::runtime::step();

            let ret_val = *result.lock().unwrap();
            assert!(ret_val.is_ok(), "Expected Ok result, got: {:?}", ret_val);
            assert!(ret_val.unwrap().is_readable(), "Expected readable state, got: {:?}", ret_val.unwrap());
        }

        // Multi readiness but writable
        {
            let reg = create_registration(IoEventInterest::WRITABLE);
            let result = run_request_readiness(&reg, IoEventInterest::WRITABLE);

            crate::testing::mock::runtime::step();
            assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // We still have task not done

            reg.inner.wake(ReadinessState::from_components(
                (READINESS_STATE_READABLE | READINESS_STATE_WRITABLE) as u16,
                0,
            )); // Make task ready
            crate::testing::mock::runtime::step();

            let ret_val = *result.lock().unwrap();
            assert!(ret_val.is_ok(), "Expected Ok result, got: {:?}", ret_val);
            assert!(ret_val.unwrap().is_writable(), "Expected writable state, got: {:?}", ret_val.unwrap());
        }
    }

    #[test]
    fn async_registration_request_readiness_not_consumed_does_not_await() {
        crate::testing::mock::runtime::init();

        let reg = create_registration(IoEventInterest::WRITABLE);

        // 1. Readiness is achieved
        {
            let result = run_request_readiness(&reg, IoEventInterest::WRITABLE);

            crate::testing::mock::runtime::step();
            assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // We still have task not done

            reg.inner.wake(ReadinessState::from_components(READINESS_STATE_WRITABLE as u16, 0)); // Make task ready
            crate::testing::mock::runtime::step();

            let ret_val = *result.lock().unwrap();
            assert!(ret_val.is_ok(), "Expected Ok result, got: {:?}", ret_val);
            assert!(ret_val.unwrap().is_writable(), "Expected writable state, got: {:?}", ret_val.unwrap());
        }

        // 2. Try again, but this time we are not waiting for anything
        {
            let result = run_request_readiness(&reg, IoEventInterest::WRITABLE);

            crate::testing::mock::runtime::step();
            assert_eq!(0, crate::testing::mock::runtime::remaining_tasks()); // Immediate return, we are not waiting for anything

            let ret_val = *result.lock().unwrap();
            assert!(ret_val.is_ok(), "Expected Ok result, got: {:?}", ret_val);
            assert!(ret_val.unwrap().is_writable(), "Expected writable state, got: {:?}", ret_val.unwrap());
        }
    }

    #[test]
    fn async_registration_request_readiness_consumed_does_await() {
        crate::testing::mock::runtime::init();

        let reg = create_registration(IoEventInterest::WRITABLE);

        // 1. Readiness is achieved

        let result = run_request_readiness(&reg, IoEventInterest::WRITABLE);

        crate::testing::mock::runtime::step();
        assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // We still have task not done

        reg.inner.wake(ReadinessState::from_components(READINESS_STATE_WRITABLE as u16, 0)); // Make task ready
        crate::testing::mock::runtime::step();

        let ret_val = *result.lock().unwrap();
        assert!(ret_val.is_ok(), "Expected Ok result, got: {:?}", ret_val);
        assert!(ret_val.unwrap().is_writable(), "Expected writable state, got: {:?}", ret_val.unwrap());

        // 2. Consume readiness
        reg.clear_readiness(ret_val.unwrap(), IoEventInterest::WRITABLE);

        // 3. Try again, but this time we are not waiting for anything
        let _ = run_request_readiness(&reg, IoEventInterest::WRITABLE);

        crate::testing::mock::runtime::step();
        crate::testing::mock::runtime::step();

        assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // Task stuck there since its not done
    }

    #[test]
    fn async_registration_request_readiness_block_on_not_requested_readiness() {
        crate::testing::mock::runtime::init();

        let reg = create_registration(IoEventInterest::WRITABLE);

        // 1. Readiness is requested

        let result = run_request_readiness(&reg, IoEventInterest::WRITABLE);

        crate::testing::mock::runtime::step();
        assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // We still have task not done

        // 2. Other readiness is set, but not requested
        reg.inner.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 0)); // Make task ready

        // 3. So no progress on Future even after multiple calls
        crate::testing::mock::runtime::step();
        crate::testing::mock::runtime::step();
        crate::testing::mock::runtime::step();
        assert_eq!(1, crate::testing::mock::runtime::remaining_tasks()); // Task stuck there since its not done

        let ret_val = *result.lock().unwrap();
        assert!(ret_val.is_err());
    }

    #[test]
    fn async_registration_poll_readiness() {
        // Not ready
        {
            let reg = create_registration(IoEventInterest::WRITABLE);

            let mock_fn = build_with_location!(MockFnBuilder::new().times(0));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            let waker = Waker::from(waker_mock.clone());
            let mut ctx = Context::from_waker(&waker);

            let ret = reg.poll_readiness(&mut ctx, IoEventInterest::READABLE);

            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);
        }

        // Waked
        {
            let reg = create_registration(IoEventInterest::WRITABLE);

            let mock_fn = build_with_location!(MockFnBuilder::new().times(1));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            let waker = Waker::from(waker_mock.clone());
            let mut ctx = Context::from_waker(&waker);

            let mut ret = reg.poll_readiness(&mut ctx, IoEventInterest::READABLE);
            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);

            reg.inner.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 0)); // Make task ready

            ret = reg.poll_readiness(&mut ctx, IoEventInterest::READABLE);

            assert!(ret.is_ready(), "Expected ready state");
        }

        // Multi poll
        {
            let reg = create_registration(IoEventInterest::WRITABLE);

            let mock_fn = build_with_location!(MockFnBuilder::new().times(1));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            let waker = Waker::from(waker_mock.clone());
            let mut ctx = Context::from_waker(&waker);

            let mut ret = reg.poll_readiness(&mut ctx, IoEventInterest::READABLE);
            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);

            ret = reg.poll_readiness(&mut ctx, IoEventInterest::READABLE);
            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);

            ret = reg.poll_readiness(&mut ctx, IoEventInterest::READABLE);
            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);

            reg.inner.wake(ReadinessState::from_components(READINESS_STATE_READABLE as u16, 0)); // Make task ready

            ret = reg.poll_readiness(&mut ctx, IoEventInterest::READABLE);

            assert!(ret.is_ready(), "Expected ready state");
        }

        // Not ready
        {
            let reg = create_registration(IoEventInterest::WRITABLE);

            let mock_fn = build_with_location!(MockFnBuilder::new().times(0));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            let waker = Waker::from(waker_mock.clone());
            let mut ctx = Context::from_waker(&waker);

            let ret = reg.poll_readiness(&mut ctx, IoEventInterest::WRITABLE);

            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);
        }

        // Waked
        {
            let reg = create_registration(IoEventInterest::WRITABLE);

            let mock_fn = build_with_location!(MockFnBuilder::new().times(1));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            let waker = Waker::from(waker_mock.clone());
            let mut ctx = Context::from_waker(&waker);

            let mut ret = reg.poll_readiness(&mut ctx, IoEventInterest::WRITABLE);
            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);

            reg.inner.wake(ReadinessState::from_components(READINESS_STATE_WRITABLE as u16, 0)); // Make task ready

            ret = reg.poll_readiness(&mut ctx, IoEventInterest::WRITABLE);

            assert!(ret.is_ready(), "Expected ready state");
        }

        // Multi poll
        {
            let reg = create_registration(IoEventInterest::WRITABLE);

            let mock_fn = build_with_location!(MockFnBuilder::new().times(1));
            let waker_mock = MockWaker::new(mock_fn).into_arc();

            let waker = Waker::from(waker_mock.clone());
            let mut ctx = Context::from_waker(&waker);

            let mut ret = reg.poll_readiness(&mut ctx, IoEventInterest::WRITABLE);
            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);

            ret = reg.poll_readiness(&mut ctx, IoEventInterest::WRITABLE);
            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);

            ret = reg.poll_readiness(&mut ctx, IoEventInterest::WRITABLE);
            assert!(ret.is_pending(), "Expected pending state, got: {:?}", ret);

            reg.inner.wake(ReadinessState::from_components(READINESS_STATE_WRITABLE as u16, 0)); // Make task ready

            ret = reg.poll_readiness(&mut ctx, IoEventInterest::WRITABLE);

            assert!(ret.is_ready(), "Expected ready state");
        }
    }

    // ReadinessState tests

    // Helper function to create ReadinessState with specific bit patterns for testing
    // This assumes that ReadinessState can really be anything to test extensively ensuring behavior does not change when we add new states ie.
    fn create_state_with_bits(full_value: u32) -> ReadinessState {
        ReadinessState(full_value)
    }

    #[test]
    fn readiness_state_tick_extraction() {
        // Test tick extraction from upper 16 bits
        let state = create_state_with_bits(0x1234_5678);
        assert_eq!(state.tick(), 0x1234);

        // Test with zero tick
        let state = create_state_with_bits(0x0000_FFFF);
        assert_eq!(state.tick(), 0x0000);

        // Test with max tick
        let state = create_state_with_bits(0xFFFF_0000);
        assert_eq!(state.tick(), 0xFFFF);
    }

    #[test]
    fn readiness_state_is_readable() {
        // Test when readable bit is set (assuming bit 0 is readable)
        let state = create_state_with_bits(0x0000_0001);
        assert!(state.is_readable());

        // Test when readable bit is not set
        let state = create_state_with_bits(0x0000_0002);
        assert!(!state.is_readable());

        // Test with other bits set but not readable
        let state = create_state_with_bits(0xFFFF_FFFE);
        assert!(!state.is_readable());

        // Test with all bits set
        let state = create_state_with_bits(0xFFFF_FFFF);
        assert!(state.is_readable());
    }

    #[test]
    fn readiness_state_is_writable() {
        // Test when writable bit is set (assuming bit 1 is writable)
        let state = create_state_with_bits(0x0000_0002);
        assert!(state.is_writable());

        // Test when writable bit is not set
        let state = create_state_with_bits(0x0000_0001);
        assert!(!state.is_writable());

        // Test with other bits set but not writable
        let state = create_state_with_bits(0xFFFF_FFFD);
        assert!(!state.is_writable());

        // Test with all bits set
        let state = create_state_with_bits(0xFFFF_FFFF);
        assert!(state.is_writable());
    }

    #[test]
    fn readiness_state_clear_readable() {
        // Test clearing readable when it's set
        let mut state = create_state_with_bits(0x1234_0003); // Both readable and writable set
        state.clear_readable();
        assert!(!state.is_readable());
        assert_eq!(state.tick(), 0x1234); // Tick should be preserved

        // Test clearing readable when it's already clear
        let mut state = create_state_with_bits(0x5678_0002); // Only writable set
        state.clear_readable();
        assert!(!state.is_readable());
        assert_eq!(state.tick(), 0x5678);

        // Test clearing readable preserves other state bits
        let mut state = create_state_with_bits(0xABCD_00FF); // Many state bits set
        let original_writable = state.is_writable();
        state.clear_readable();
        assert!(!state.is_readable());
        assert_eq!(state.is_writable(), original_writable);
        assert_eq!(state.tick(), 0xABCD);
    }

    #[test]
    fn readiness_state_clear_writable() {
        // Test clearing writable when it's set
        let mut state = create_state_with_bits(0x9876_0003); // Both readable and writable set
        state.clear_writable();
        assert!(!state.is_writable());
        assert_eq!(state.tick(), 0x9876); // Tick should be preserved

        // Test clearing writable when it's already clear
        let mut state = create_state_with_bits(0x4321_0001); // Only readable set
        state.clear_writable();
        assert!(!state.is_writable());
        assert_eq!(state.tick(), 0x4321);

        // Test clearing writable preserves other state bits
        let mut state = create_state_with_bits(0xDEAD_00FF); // Many state bits set
        let original_readable = state.is_readable();
        state.clear_writable();
        assert!(!state.is_writable());
        assert_eq!(state.is_readable(), original_readable);
        assert_eq!(state.tick(), 0xDEAD);
    }

    #[test]
    fn readiness_state_clear_with_mask() {
        // Test clearing specific bits using mask
        let mut state = create_state_with_bits(0x1111_00FF);
        let mask = create_state_with_bits(0x0000_000F); // Clear lower 4 bits
        state.clear(mask);

        // Verify tick is preserved
        assert_eq!(state.tick(), 0x1111);

        // Test clearing with empty mask (should not change anything)
        let mut state = create_state_with_bits(0x2222_00FF);
        let original_state = state;
        let mask = create_state_with_bits(0x0000_0000);
        state.clear(mask);
        assert_eq!(state, original_state);

        // Test clearing with full state mask
        let mut state = create_state_with_bits(0x3333_FFFF);
        let mask = create_state_with_bits(0x0000_FFFF);
        state.clear(mask);
        assert_eq!(state.tick(), 0x3333);
        // All state bits should be cleared

        // Test that mask's tick doesn't affect the result
        let mut state = create_state_with_bits(0x4444_0003);
        let mask = create_state_with_bits(0x9999_0001); // Different tick in mask
        state.clear(mask);
        assert_eq!(state.tick(), 0x4444); // Original tick preserved
    }

    #[test]
    fn readiness_state_union() {
        // Test union of two states
        let state1 = create_state_with_bits(0x1111_0001); // Readable
        let state2 = create_state_with_bits(0x2222_0002); // Writable
        let result = state1.union(state2);

        // Result should have both flags and higher tick
        assert!(result.is_readable());
        assert!(result.is_writable());

        // Test union with same tick
        let state1 = create_state_with_bits(0x1234_0001);
        let state2 = create_state_with_bits(0x1234_0002);
        let result = state1.union(state2);
        assert_eq!(result.tick(), 0x1234);

        // Test union is commutative
        let state1 = create_state_with_bits(0x1111_0001);
        let state2 = create_state_with_bits(0x2222_0002);
        assert_eq!(state1.union(state2).extract_state(), state2.union(state1).extract_state());
    }

    #[test]
    fn readiness_state_intersection() {
        // Test intersection with common bits
        let state1 = create_state_with_bits(0x1111_0003); // Both readable and writable
        let state2 = create_state_with_bits(0x2222_0001); // Only readable
        let result = state1.intersection(state2);
        assert!(result.is_some());
        if let Some(intersected) = result {
            assert!(intersected.is_readable());
            assert!(!intersected.is_writable());
        }

        // Test intersection with no common bits
        let state1 = create_state_with_bits(0x3333_0001); // Only readable
        let state2 = create_state_with_bits(0x4444_0002); // Only writable
        let result = state1.intersection(state2);
        assert!(result.is_none());

        // Test intersection with identical states
        let state = create_state_with_bits(0x5555_0003);
        let result = state.intersection(state);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), state);

        // Test intersection with empty state
        let state1 = create_state_with_bits(0x6666_0003);
        let state2 = create_state_with_bits(0x7777_0000);
        let result = state1.intersection(state2);
        assert!(result.is_none());

        // Test intersection is commutative
        let state1 = create_state_with_bits(0x1111_0003);
        let state2 = create_state_with_bits(0x2222_0001);
        assert_eq!(
            state1.intersection(state2).unwrap().extract_state(),
            state2.intersection(state1).unwrap().extract_state()
        );
    }

    #[test]
    fn readiness_state_increment_tick() {
        // Test normal increment
        let mut state = create_state_with_bits(0x1234_0003);
        state.increment_tick();
        assert_eq!(state.tick(), 0x1235);
        // State bits should be preserved
        assert!(state.is_readable());
        assert!(state.is_writable());

        // Test increment at boundary (u16 max)
        let mut state = create_state_with_bits(0xFFFF_0001);
        state.increment_tick();
        assert_eq!(state.tick(), 0x0000); // Should wrap around
        assert!(state.is_readable()); // State should be preserved

        // Test multiple increments
        let mut state = create_state_with_bits(0x0000_0002);
        for i in 1..=100 {
            state.increment_tick();
            assert_eq!(state.tick(), i);
            assert!(state.is_writable()); // State should be preserved
        }
    }

    #[test]
    fn readiness_state_from_components() {
        // Test combining state and tick
        let state = ReadinessState::from_components(0x1234, 0x5678);
        assert_eq!(state.extract_state(), 0x1234);
        assert_eq!(state.extract_tick(), 0x5678);

        // Test with zero values
        let state = ReadinessState::from_components(0x0000, 0x0000);
        assert_eq!(state.extract_state(), 0x0000);
        assert_eq!(state.extract_tick(), 0x0000);

        // Test with max values
        let state = ReadinessState::from_components(0xFFFF, 0xFFFF);
        assert_eq!(state.extract_state(), 0xFFFF);
        assert_eq!(state.extract_tick(), 0xFFFF);
    }

    #[test]
    fn readiness_state_extract_state() {
        // Test extracting state from lower 16 bits
        let state = create_state_with_bits(0x1234_5678);
        assert_eq!(state.extract_state(), 0x5678);

        // Test with zero state
        let state = create_state_with_bits(0xFFFF_0000);
        assert_eq!(state.extract_state(), 0x0000);

        // Test with max state
        let state = create_state_with_bits(0x0000_FFFF);
        assert_eq!(state.extract_state(), 0xFFFF);
    }

    #[test]
    fn readiness_state_extract_tick() {
        // Test extracting tick from upper 16 bits
        let state = create_state_with_bits(0x1234_5678);
        assert_eq!(state.extract_tick(), 0x1234);

        // Test with zero tick
        let state = create_state_with_bits(0x0000_FFFF);
        assert_eq!(state.extract_tick(), 0x0000);

        // Test with max tick
        let state = create_state_with_bits(0xFFFF_0000);
        assert_eq!(state.extract_tick(), 0xFFFF);
    }

    #[test]
    fn readiness_state_clone_and_copy() {
        let state1 = create_state_with_bits(0x1234_5678);
        let state2 = state1; // Copy
        #[allow(clippy::clone_on_copy)]
        let state3 = state1.clone(); // Clone

        assert_eq!(state1, state2);
        assert_eq!(state1, state3);
        assert_eq!(state2, state3);
    }

    #[test]
    fn readiness_state_partial_eq() {
        let state1 = create_state_with_bits(0x1234_5678);
        let state2 = create_state_with_bits(0x1234_5678);
        let state3 = create_state_with_bits(0x1234_5679);

        assert_eq!(state1, state2);
        assert_ne!(state1, state3);
        assert_ne!(state2, state3);
    }

    #[test]
    fn readiness_state_edge_cases_bit_manipulation() {
        // Test alternating bit patterns
        let state = create_state_with_bits(0xAAAA_5555);
        assert_eq!(state.extract_tick(), 0xAAAA);
        assert_eq!(state.extract_state(), 0x5555);

        let state = create_state_with_bits(0x5555_AAAA);
        assert_eq!(state.extract_tick(), 0x5555);
        assert_eq!(state.extract_state(), 0xAAAA);

        // Test single bit patterns
        for bit in 0..16 {
            let state_bit = 1u16 << bit;
            let tick_bit = 1u16 << bit;

            let state = ReadinessState::from_components(state_bit, tick_bit);
            assert_eq!(state.extract_state(), state_bit);
            assert_eq!(state.extract_tick(), tick_bit);
        }
    }

    #[test]
    fn readiness_state_mask_constants() {
        // Verify the mask constants are correct
        assert_eq!(READINESS_STATE_MASK, 0x0000FFFF);
        assert_eq!(READINESS_TICK_MASK, 0xFFFF0000);

        // Test that masks don't overlap
        assert_eq!(READINESS_STATE_MASK & READINESS_TICK_MASK, 0);

        // Test that masks cover all bits
        assert_eq!(READINESS_STATE_MASK | READINESS_TICK_MASK, 0xFFFFFFFF);
    }

    #[test]
    fn readiness_state_complex_scenarios() {
        // Scenario: Multiple operations on the same state
        let mut state = create_state_with_bits(0x1000_0003); // Both flags set

        // Clear readable, then increment tick
        state.clear_readable();
        state.increment_tick();
        assert!(!state.is_readable());
        assert!(state.is_writable());
        assert_eq!(state.tick(), 0x1001);

        // Union with another state
        let other = create_state_with_bits(0x2000_0001); // Different tick, readable
        let result = state.union(other);
        assert!(result.is_readable());
        assert!(result.is_writable());
        assert_eq!(result.tick(), state.tick()); // Tick is internal details, always propagated from left side

        // Intersection should find common bits (none in this case)
        let intersection = state.intersection(other);
        assert!(intersection.is_none());
    }
}
