// *******************************************************************************
// Copyright (c) 2026 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// <https://www.apache.org/licenses/LICENSE-2.0>
//
// SPDX-License-Identifier: Apache-2.0
// *******************************************************************************

//!
//! IO driver based on MIO selector that is able to integrate with async world.
//!
//! The driver [`IoDriver`] is responsible for holding all objects required by MIO, connected registrations [`RegistrationInfo`]
//! and to provide interface for worker to poll IO events. The driver also provides further striped access to
//! certain functionalities:
//!
//!  - [`IoDriverHandle`] is a proxy that is able to register and deregister IO sources in a safe way.
//!  - [`IoDriverUnparker`] is a proxy that is able to unpark connected IO Driver. This object is used across worker via [`AsyncScheduler`]
//!    to wakeup worker that is currently waiting on MIO poll.
//!
//! The driver has to entry point contexts that should be looked into:
//!  - [`IoDriver`] `process_io` is called by worker to poll IO events and wake up all tasks that are registered for IO events that are ready.
//!  - [`IoDriverHandle`] via `add_io_source` and `remove_io_source` are called by upper layers to register and deregister IO sources in a safe way.
//!    This can be  called from any other worker thread once user uses some async IO API.
//!
//!
//! Explainer Sequence diagrams can be found in the [sequence diagram](../../doc/io_driver.md)
//!

// TODO: To be removed once used in IO APIs
#![allow(dead_code)]

use crate::macros::log::*;
use core::time::Duration;
use std::sync::{Arc, Mutex};

use core::sync::atomic::Ordering;

use crate::{
    io::{
        async_registration::{ReadinessState, RegistrationInfo},
        AsyncSelector,
    },
    mio::{
        poll::Poll,
        registry::Registry,
        types::{IoEvent, IoEventInterest, IoId, IoRegistryEntry, IoSelector},
    },
};
use kyron_foundation::{
    containers::*,
    prelude::{
        iceoryx2_bb_container::slotmap::{SlotMap, SlotMapKey},
        vector_extension::VectorExtension,
        CommonErrors, FoundationAtomicBool, ScoreLogDebug,
    },
};

/// Holds MIO object and all parts needed to manage it
pub struct IoDriver {
    inner: Mutex<IoDriverInner>,
    async_registration: Arc<Registrations>,
    registry: Registry<AsyncSelector>,
    waker: Arc<<AsyncSelector as IoSelector>::Waker>,
}

pub(crate) struct IoDriverInner {
    pool: Poll<AsyncSelector>,
    events: Vec<IoEvent>,
}

/// Proxy handle that let all async code interact with underlying MIO selector in a safe way.
#[derive(Clone)]
pub(crate) struct IoDriverHandle<T: IoSelector = AsyncSelector> {
    registry: Registry<T>,                  // Derived from IoDriver.inner.pool.registry()
    async_registration: Arc<Registrations>, // Shared with IoDriver to manage registrations
}

// Proxy that is able to unpark connected IO Driver
pub(crate) struct IoDriverUnparker {
    waker: Arc<<AsyncSelector as IoSelector>::Waker>,
}

struct RegistrationData {
    tracking: SlotMap<Arc<RegistrationInfo>>,
    waiting_release: Vec<SlotMapKey>, // Could be lock free Container from iceoryx to not need mutex on removal.
}

/// Internal structure that hold all registrations and ensure their lifetime until they are not used anymore.
pub(super) struct Registrations {
    pending_release_count: FoundationAtomicBool,
    data: Mutex<RegistrationData>,
}

impl Registrations {
    pub fn new(count: usize) -> Self {
        Registrations {
            pending_release_count: FoundationAtomicBool::new(false),
            data: Mutex::new(RegistrationData {
                tracking: SlotMap::new(count),
                waiting_release: Vec::new_in_global(count),
            }),
        }
    }

    fn create_registration_info(&self) -> Result<Arc<RegistrationInfo>, CommonErrors> {
        let mut item = Arc::<RegistrationInfo>::new_uninit();

        let mut data = self.data.lock().unwrap();
        let key = data.tracking.next_free_key().ok_or(CommonErrors::NoSpaceLeft)?;

        Arc::get_mut(&mut item)
            .unwrap()
            .write(RegistrationInfo::new(key.value()));
        let item = unsafe { item.assume_init() };

        let key_ret = data.tracking.insert(item.clone()).ok_or(CommonErrors::NoSpaceLeft)?;
        debug_assert!(
            key == key_ret,
            "SlotMap returned key should be the same as provided in next_free_key"
        );

        Ok(item)
    }

    fn schedule_registration_for_disposal(&self, key: SlotMapKey) {
        let mut data = self.data.lock().unwrap();
        data.waiting_release
            .push(key)
            .expect("Failed to schedule registration for disposal"); // Vec cannot fail here as it has enough capacity
        self.pending_release_count.store(true, Ordering::Release);
    }

    fn cleanup_disposed_registrations(&self) {
        if !self.pending_release_count.load(Ordering::Acquire) {
            return;
        }

        let mut data = self.data.lock().unwrap();

        while let Some(key) = data.waiting_release.pop() {
            data.tracking.remove(key);
        }

        // Ordering does not matter, since the only synchronized access is done in this function and this can be called only once at a time
        self.pending_release_count.store(false, Ordering::Relaxed);
    }

    // # Caveats
    // Expensive call that takes a lock, should only be used for error condition cleanups
    fn release_registration_info(&self, key: SlotMapKey) {
        let mut data = self.data.lock().unwrap();
        data.tracking.remove(key);

        //TODO: shall we check pending for double sure ?
    }
}

impl<T: IoSelector> IoDriverHandle<T> {
    pub(super) fn new(registry: Registry<T>, async_registration: Arc<Registrations>) -> Self {
        IoDriverHandle {
            registry,
            async_registration,
        }
    }

    /// Adds given IO source into the driver so it will be polled for events.
    pub(crate) fn add_io_source<Source>(
        &self,
        source: &mut Source,
        interest: IoEventInterest,
    ) -> Result<Arc<RegistrationInfo>, CommonErrors>
    where
        Source: IoRegistryEntry<T> + core::fmt::Debug + ScoreLogDebug,
    {
        self.async_registration
            .create_registration_info()
            .and_then(|info| match self.registry.register(source, info.identifier(), interest) {
                Ok(_) => {
                    info!(
                        "Successfully registered IO source ({:?}) with interest: {:?} and assigned info ident {:?}",
                        source,
                        interest,
                        info.identifier()
                    );

                    Ok(info)
                }
                Err(e) => {
                    //We need to remove this Arc from the tracking so it's not left dangling
                    let key = info.tracking_key();

                    if key.is_none() {
                        error!("Trying to release registration that was not registered ({:?})", source);
                        return Err(e);
                    }

                    self.async_registration.release_registration_info(SlotMapKey::new(key.unwrap()));
                    println!(
                        "Failed to register IO source ({:?}) with interest: {:?} and assigned info ident {:?}, error: {:?}",
                        source,
                        interest,
                        info.identifier(),
                        e
                    );
                    Err(e)
                }
            })
    }

    /// Removes given IO source from the driver so it will not be polled for events anymore. Once this is called
    /// ths source is put on pending removal list and will be fully removed before next poll call. This way
    /// we ensure that if this source was considered in the poll call it will be still valid until we finish processing
    pub(crate) fn remove_io_source<Source>(&self, source: &mut Source, registration: &Arc<RegistrationInfo>)
    where
        Source: IoRegistryEntry<T> + core::fmt::Debug + ScoreLogDebug,
    {
        match self.registry.deregister(source) {
            Ok(_) => info!("Successfully deregistered IO source ({:?})", source),
            Err(e) => warn!("Deregister of source({:?}) failed with {:?}", source, e),
        }

        let key = registration.tracking_key();

        if key.is_none() {
            error!("Trying to remove registration that was not registered ({:?})", source);
            return;
        }

        // Right now we are sure that there is somewhere a next call to driver.poll, this source will not be considered.
        // Still it could be this source was considered int the poll happened during or little before deregister call so
        // we need to make sure that we keep it until next poll happens and remove before to release our reference count
        self.async_registration
            .schedule_registration_for_disposal(SlotMapKey::new(key.unwrap()));
    }
}

impl Default for IoDriver {
    fn default() -> Self {
        // TODO: This shall be routed to kyron::Runtime builder, once we have configuration exposed there
        IoDriver::new(AsyncSelector::new(1024))
    }
}

type AccessGuard<'a> = std::sync::MutexGuard<'a, IoDriverInner>;

/// Internal waker IoId used by IoDriverUnparker to wake up the IO driver from poll.
const WAKER_IO_ID: u64 = 0xAABBCCDD;

impl IoDriverUnparker {
    pub(crate) fn unpark(&self) {
        self.waker.wake();
    }
}

impl IoDriver {
    pub(crate) fn new(selector: AsyncSelector) -> Self {
        let poll = Poll::new(selector);
        let waker = poll
            .create_waker(IoId::new(WAKER_IO_ID))
            .expect("Failed to create waker for IO driver, this should never happen");

        let max_fd_supported = poll.get_safe_poll_events_capacity();

        IoDriver {
            async_registration: Arc::new(Registrations::new(max_fd_supported)),
            registry: poll.registry().clone(),
            inner: Mutex::new(IoDriverInner {
                pool: poll,
                events: Vec::new_in_global(max_fd_supported),
            }),
            waker: Arc::new(waker),
        }
    }

    /// Provides unparker that is able to unpark (wake up from poll) this IO driver from other threads.
    pub(crate) fn get_unparker(&self) -> IoDriverUnparker {
        IoDriverUnparker {
            waker: self.waker.clone(),
        }
    }

    /// Provides handle that is able to register and deregister IO sources in this driver.
    pub fn handle(&self) -> IoDriverHandle<AsyncSelector> {
        IoDriverHandle::new(self.registry.clone(), self.async_registration.clone())
    }

    pub(crate) fn try_get_access(&self) -> Option<AccessGuard<'_>> {
        self.inner.try_lock().ok()
    }

    /// Processes IO events by polling MIO selector and waking up all tasks that are registered for IO events that are ready.
    pub(crate) fn process_io(&self, inner: &mut AccessGuard, timeout: Option<Duration>) -> Result<(), CommonErrors> {
        // Once IO source is deregistered by user, we still keep it in pending removal list until we finish processing
        // events so we are sure that no invalid reference is used. Once we are coming here, we cleanup pending list
        // since we are sure it's not used anymore in poll processing
        self.async_registration.cleanup_disposed_registrations();

        let binding: &mut IoDriverInner = inner;

        match binding.pool.poll(&mut binding.events, timeout) {
            Err(CommonErrors::Timeout) => {
                return Err(CommonErrors::Timeout);
            },
            Ok(_) => {},
            _ => {
                panic!("Generic error not handled!!!");
            },
        }

        for event in binding.events.iter() {
            let id = event.id();

            if id == IoId::new(WAKER_IO_ID) {
                // This is our internal waker, just ignore it
                continue;
            }

            debug!("Processing mio identifier: {:?}", id);
            // SAFETY
            // Those two operations assumes that the lifetime of RegistrationInfo is held
            // by other part of code(Registrations in IoHandle), until this is fully removed
            // from the mio part and this code have done polling so we are sure we never end up here with invalid reference.
            // We assure this by keeping reference in Registrations until next poll call happens after deregister is called
            unsafe {
                let info = RegistrationInfo::from_identifier(id);
                let info_ref = &*info;

                let readiness = ReadinessState::from_mio_event(event);
                info_ref.wake(readiness);
            }
        }

        Ok(())
    }
}
