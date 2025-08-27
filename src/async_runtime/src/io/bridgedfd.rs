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
use core::task::{ready, Context, Poll};

use foundation::prelude::{error, CommonErrors};

use crate::{
    io::{
        async_registration::{AsyncRegistration, ReadinessState},
        read_buf::ReadBuf,
        AsyncSelector,
    },
    mio::types::{IoEventInterest, IoRegistryEntry},
};
use core::ops::{Deref, DerefMut};
use std::io::{Error, ErrorKind};

/// Bridge between MIO layer and async world that let other upper layers use it in concrete implementations.
/// This is a basic building block for async IO operations in implementations like networking part.
/// User shall only need to use this type to get async IO operations on top of MIO objects. All other work is done by
/// the internals of this type.
pub struct BridgedFd<T: IoRegistryEntry<AsyncSelector> + core::fmt::Debug> {
    pub(crate) mio_object: T,
    registration: AsyncRegistration,
}

impl<T: IoRegistryEntry<AsyncSelector> + core::fmt::Debug> BridgedFd<T> {
    /// Creates MIO <-> async bridge for the given MIO object with READABLE and WRITABLE interests.
    pub fn new(mut mio_object: T) -> Result<Self, CommonErrors> {
        let registration = AsyncRegistration::new(&mut mio_object)?;
        Ok(BridgedFd { mio_object, registration })
    }

    /// Creates MIO <-> async bridge for the given MIO object with provided interest.
    pub fn new_with_interest(mut mio_object: T, interest: IoEventInterest) -> Result<Self, CommonErrors> {
        let registration = AsyncRegistration::new_with_interest(&mut mio_object, interest)?;

        Ok(BridgedFd { mio_object, registration })
    }

    /// Async interface to check readiness for the given interest. Keep in mind that this have to be registered before in `new` or `new_with_interest`.
    pub async fn ready(&self, interest: IoEventInterest) -> std::io::Result<ReadinessState> {
        self.registration.request_readiness(interest).await.map_err(|e| e.into())
    }

    /// This bring ability to conduct synchronous, non blocking IO calls from upper layers with async behavior.
    ///
    /// # Notes
    /// User is responsible to forward `mio_object` errors into this call so we can detect `WouldBlock` scenarios and provide async behavior
    ///
    pub async fn async_call<F, R>(&self, interest: IoEventInterest, mut f: F) -> std::io::Result<R>
    where
        F: FnMut(&T) -> std::io::Result<R>,
    {
        loop {
            let r = self.registration.request_readiness(interest).await?;

            match f(&self.mio_object) {
                Ok(v) => {
                    return Ok(v);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    self.registration.clear_readiness(r, interest);
                    continue;
                }
                Err(e) => {
                    error!("Error reading from mio object via async: {}", e);
                    return Err(e);
                }
            }
        }
    }

    /// Synchronous read operation that will read data into the provided `buf` which will register waker so the current task is waken when IO is ready.
    /// This is future compatible API to be used from Futures. This is also main API that powers AsyncRead trait implementation and it's derivatives
    pub fn poll_read(&mut self, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<Result<(), Error>>
    where
        T: std::io::Read,
    {
        loop {
            // API makes sure requested readiness is returned accordingly
            let readiness = ready!(self.registration.poll_readiness(cx, IoEventInterest::READABLE));

            let space = buf.initialized_unfilled_mut();

            match self.mio_object.read(space) {
                Ok(n) => {
                    buf.advance_filled(n);

                    // TODO: Add here code that can detect if we use other selector than poll so we can do clear readiness logic here

                    if n == 0 {
                        // EOF or no more data to read, anyway we are done in this round
                        // TODO: Handle EOF on all paths missing still
                    }

                    // Even if we read less bytes than the buffer has space, we don clear readiness as some underlying selectors may need to
                    // be explicitly `read` with WWouldBlock error to rearm notifications. This is pesimization for some of them like `epoll`.
                    // This can be fixed if we coordinate some compile time flag between this place and AsyncSelector binding
                    return Poll::Ready(Ok(()));
                }

                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    // Clear readiness and let it run again and check readiness, if not ready, it will return Poll::Pending
                    self.registration.clear_readiness(readiness, IoEventInterest::READABLE);
                }
                Err(e) => {
                    error!("Error reading from mio object: {}", e);
                    return Poll::Ready(Err(e));
                }
            }
        }
    }

    /// Synchronous write operation that will write data from the provided `buf` which will register waker so the current task is waken when IO is ready.
    /// This is future compatible API to be used from Futures. This is also main API that powers AsyncWrite trait implementation and it's derivatives
    ///
    pub fn poll_write(&mut self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, Error>>
    where
        T: std::io::Write,
    {
        loop {
            // API makes sure requested readiness is returned accordingly
            let readiness = ready!(self.registration.poll_readiness(cx, IoEventInterest::WRITABLE));

            match self.mio_object.write(buf) {
                Ok(n) => {
                    // Same as in in read, we can only arm new events received by selector by writing till WouldBlock error.
                    // TODO: Add here code that can detect if we use other selector than poll so we can do clear readiness logic here
                    return Poll::Ready(Ok(n));
                }

                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    // Clear readiness and let it run again and check readiness, if not ready, it will return Poll::Pending
                    self.registration.clear_readiness(readiness, IoEventInterest::WRITABLE);
                }
                Err(e) => {
                    error!("Error writing using mio object: {}", e);
                    return Poll::Ready(Err(e));
                }
            }
        }
    }
}

impl<T: IoRegistryEntry<AsyncSelector> + core::fmt::Debug> Deref for BridgedFd<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.mio_object
    }
}

impl<T: IoRegistryEntry<AsyncSelector> + core::fmt::Debug> DerefMut for BridgedFd<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mio_object
    }
}

impl<T: IoRegistryEntry<AsyncSelector> + core::fmt::Debug> Drop for BridgedFd<T> {
    fn drop(&mut self) {
        // Deregister the source from the driver
        self.registration.drop_registration(&mut self.mio_object);
    }
}
