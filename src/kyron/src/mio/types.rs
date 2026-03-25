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

use crate::mio::registry::Registry;
use core::{
    fmt::Debug,
    ops::{BitAnd, BitOr, Not},
    time::Duration,
};

use kyron_foundation::prelude::CommonErrors;
use std::os::fd::{AsRawFd, RawFd};

/// The return type for I/O operations.
pub type IoResult<T> = core::result::Result<T, std::io::Error>;

/// The return type for non IO API
pub type Result<T> = core::result::Result<T, CommonErrors>;

kyron_foundation::import_score_log!();
kyron_foundation::derive_score_debug_for_struct!(
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// A unique identifier for an I/O resource.
    pub struct IoId(u64);
);

impl IoId {
    /// Create an `Id`.
    pub fn new(id: u64) -> Self {
        IoId(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// This trait is used to wrap I/O sources and provide a way to perform I/O operations on them.
pub trait IoCall<S: AsRawFd> {
    /// Perform an I/O operation on the source, passing the source as an argument to the closure. It will make sure that when non blocking I/O will
    /// indicate blocking call, it will handle all selector logic to make it working again with selector.
    fn io_call<F, R>(&self, f: F) -> IoResult<R>
    where
        F: FnOnce(&S) -> IoResult<R>;

    fn new(source: S) -> Self;
    fn as_inner(&self) -> &S;
    fn as_inner_mut(&mut self) -> &mut S;
}

/// This trait helps to inspect register call on different abstraction levels and augment registration logic based on source or selector type.
pub trait IoRegistryEntry<T: IoSelector + ?Sized> {
    fn register(&mut self, registry: &Registry<T>, id: IoId, interest: IoEventInterest) -> Result<()>;
    fn reregister(&mut self, id: IoId, interest: IoEventInterest) -> Result<()>;
    fn deregister(&mut self) -> Result<()>;
}

/// Interface used to hide platform-dependent select logic for file descriptors.
pub trait IoSelector {
    type IoProxy<T: AsRawFd>: IoCall<T> + IoRegistryEntry<Self>;
    type Waker;

    fn register(&self, fd: RawFd, id: IoId, interest: IoEventInterest) -> Result<()>;
    fn reregister(&self, fd: RawFd, id: IoId, interest: IoEventInterest) -> Result<()>;
    fn deregister(&self, fd: RawFd) -> Result<()>;
    fn create_waker(&self, id: IoId) -> Result<Self::Waker>;
    fn capacity(&self) -> usize;
    fn select<C: IoSelectorEventContainer>(&self, events: &mut C, timeout: Option<Duration>) -> Result<()>;
}

pub trait IoSelectorEventContainer {
    fn push(&mut self, event: IoEvent) -> bool;
    fn clear(&mut self);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn capacity(&self) -> usize;
}

kyron_foundation::derive_score_debug_for_struct!(
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    /// Describes the interest in I/O operations on a file.
    ///
    ///Bit-wise operations can be used to combine interests.
    pub struct IoEventInterest(pub(crate) u8);
);

impl IoEventInterest {
    /// Describes interest in read operations on a file.
    pub const READABLE: IoEventInterest = IoEventInterest(0b0000_0001);

    /// Describes interest in write operations on a file.
    pub const WRITABLE: IoEventInterest = IoEventInterest(0b0000_0010);

    /// Check if the interest is in read operations.
    pub fn is_readable(&self) -> bool {
        (self.0 & Self::READABLE.0) != 0
    }

    /// Check if the interest is in write operations.
    pub fn is_writable(&self) -> bool {
        (self.0 & Self::WRITABLE.0) != 0
    }
}

impl BitOr for IoEventInterest {
    type Output = IoEventInterest;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitAnd for IoEventInterest {
    type Output = IoEventInterest;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl Not for IoEventInterest {
    type Output = IoEventInterest;

    fn not(self) -> Self::Output {
        Self(!self.0)
    }
}

/// An I/O event.
pub struct IoEvent {
    id: IoId,
    interest: IoEventInterest,
}

impl IoEvent {
    pub(crate) fn new(id: IoId, interest: IoEventInterest) -> Self {
        Self { id, interest }
    }

    /// The `Id` of the I/O resource.
    pub fn id(&self) -> IoId {
        self.id
    }

    /// The `EventInterest` for which the event was reported.
    pub fn interest(&self) -> IoEventInterest {
        self.interest
    }

    /// Check if the event was reported for a readable I/O resource.
    pub fn is_readable(&self) -> bool {
        self.interest.is_readable()
    }

    /// Check if the event was reported for a writable I/O resource.
    pub fn is_writable(&self) -> bool {
        self.interest.is_writable()
    }
}

/// Simple macro that implements `IoRegistryEntry` for a type that has an inner field of type `T::IoProxy` to remove boilerplate code.
#[macro_export]
macro_rules! impl_io_source_proxy {
    ($type:ident<$t:ident>, $inner:ident) => {
        impl<$t: IoSelector> $crate::mio::types::IoRegistryEntry<$t> for $type<$t> {
            fn register(
                &mut self,
                registry: &$crate::mio::registry::Registry<$t>,
                id: $crate::mio::types::IoId,
                interest: $crate::mio::types::IoEventInterest,
            ) -> $crate::mio::types::Result<()> {
                self.$inner.register(registry, id, interest)
            }

            fn reregister(
                &mut self,
                id: $crate::mio::types::IoId,
                interest: $crate::mio::types::IoEventInterest,
            ) -> $crate::mio::types::Result<()> {
                self.$inner.reregister(id, interest)
            }

            fn deregister(&mut self) -> $crate::mio::types::Result<()> {
                self.$inner.deregister()
            }
        }
    };
}
