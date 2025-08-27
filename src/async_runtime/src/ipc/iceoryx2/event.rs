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

use core::time::Duration;

use foundation::not_recoverable_error;
use foundation::prelude::*;

use crate::io::bridgedfd::BridgedFd;
use crate::io::AsyncSelector;
use crate::ipc::iceoryx2::EventBuilderAsyncExt;
use crate::mio::rawfd::RawFdBridge;
use crate::mio::types::{IoEventInterest, IoResult};
use iceoryx2::port::listener::ListenerCreateError;
use iceoryx2::port::port_identifiers::UniqueListenerId;
use iceoryx2::prelude::{EventId, FileDescriptorBased};
use iceoryx2::service;
use iceoryx2::service::port_factory::listener::PortFactoryListener;
use iceoryx2_cal::event::ListenerWaitError;

impl<Service: service::Service> EventBuilderAsyncExt for PortFactoryListener<'_, Service>
where
    <Service::Event as iceoryx2_cal::event::Event>::Listener: FileDescriptorBased,
{
    type Service = Service;
    type ListenerType = Listener<Service>;

    fn create_async(self) -> Result<Self::ListenerType, ListenerCreateError> {
        Listener::from(self.create()?).map_err(|e| {
            error!("Error creating async listener: {:?}", e);
            ListenerCreateError::ResourceCreationFailed
        })
    }
}

pub struct Listener<Service>
where
    Service: service::Service,
{
    listener: iceoryx2::port::listener::Listener<Service>,
    io: BridgedFd<RawFdBridge<AsyncSelector>>,
}

impl<Service> Listener<Service>
where
    Service: service::Service,
    <Service::Event as iceoryx2_cal::event::Event>::Listener: FileDescriptorBased,
{
    pub(crate) fn from(listener: iceoryx2::port::listener::Listener<Service>) -> Result<Self, CommonErrors> {
        // Safety:
        // - This FD is owned by iceoryx2 listener and we don't close it on drop of RawFdBridge
        // - The FD is kept along with listener so lifetime is take care of
        // - Each Listener has its own FD so no sharing is done in iceoryx2 layer
        let fd = unsafe { listener.file_descriptor().native_handle() };

        Ok(Self {
            listener,
            io: BridgedFd::new_with_interest(RawFdBridge::from(fd)?, IoEventInterest::READABLE)?,
        })
    }

    /// Returns the [`UniqueListenerId`] of the [`Listener`]
    pub fn id(&self) -> UniqueListenerId {
        self.listener.id()
    }

    /// Returns the deadline of the corresponding [`Service`](crate::service::Service).
    pub fn deadline(&self) -> Option<Duration> {
        self.listener.deadline()
    }

    /// Async wait for a new [`EventId`]. On error it returns [`ListenerWaitError`] is returned which describes
    /// the error in detail.
    pub async fn wait_one(&self) -> Result<EventId, ListenerWaitError> {
        self.io
            .async_call(IoEventInterest::READABLE, |raw_fd| raw_fd.io_call(|_| self.wait_one_internal()))
            .await
            .map_err(|_| ListenerWaitError::InternalFailure)
            .and_then(|r| match r {
                Ok(event) => Ok(event),
                Err(e) => Err(e),
            })
    }

    /// Async wait for new [`EventId`]s. Collects all [`EventId`]s that were received and
    /// calls the provided callback is with the [`EventId`] as input argument. This will `await` until callback is called at least once
    pub async fn wait_all<F: FnMut(EventId)>(&self, callback: &mut F) -> Result<(), ListenerWaitError> {
        // TODO: Iceoryx API does move callback into wait_all, so we cannot really use it, below is emulated behavior
        // with worse performance at it has to enter critical section multiple times

        self.io
            .async_call(IoEventInterest::READABLE, |raw_fd| {
                raw_fd.io_call(|_| {
                    let mut called_at_least_once = false;
                    loop {
                        match self.wait_one_internal() {
                            Ok(Ok(event)) => {
                                callback(event);
                                called_at_least_once = true;
                            }
                            Ok(Err(e)) => {
                                warn!("Error waiting for iceoryx2 event: {}", e);
                                return Ok(Err(e));
                            }
                            // This means all samples are fetched out, we can exit
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock && called_at_least_once => {
                                return Ok(Ok(()));
                            }
                            Err(e) => {
                                return Err(e);
                            }
                        }
                    }
                })
            })
            .await
            .map_err(|_| ListenerWaitError::InternalFailure)
            .and_then(|r| match r {
                Ok(event) => Ok(event),
                Err(e) => Err(e),
            })
    }

    fn wait_one_internal(&self) -> IoResult<Result<EventId, ListenerWaitError>> {
        loop {
            match self.listener.try_wait_one() {
                Ok(Some(event)) => return Ok(Ok(event)),
                Ok(_) => {
                    // This is None, so there was and error, probably EAGAIN or EWOULDBLOCK
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::WouldBlock {
                        return Err(std::io::ErrorKind::WouldBlock.into()); // Propagate WouldBlock so async layer can reregister
                    } else {
                        not_recoverable_error!(with err, "Other errors shall not be captured under Ok(None) returned from try_wait_one !");
                    }
                }
                Err(ListenerWaitError::InterruptSignal) => {
                    continue;
                }
                Err(e) => {
                    error!("Error waiting for iceoryx2 event: {}", e);
                    return Ok(Err(e));
                }
            }
        }
    }
}
