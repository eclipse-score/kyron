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

use core::fmt::Debug;
use std::os::fd::{AsRawFd, RawFd};

use crate::{
    impl_io_source_proxy,
    mio::types::{IoCall, IoResult, IoSelector, Result},
};

///
/// This is adapter to use RawFd as IoSource in mio based selectors.
/// Keep in mind that when dropped it will close the underlying fd as it takes ownership of it.
///
pub struct RawFdBridge<T: IoSelector> {
    inner: T::IoProxy<RawFd>,
}

impl<T> Debug for RawFdBridge<T>
where
    T: IoSelector,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RawFdBridge<fd: {:?}>", self.inner.as_inner().as_raw_fd())
    }
}

impl<T: IoSelector> RawFdBridge<T> {
    ///
    /// Takes ownership of given RawFd to use it as IoSource in mio based selector.
    ///
    /// # Safety
    ///  - The caller must ensure that the given RawFd is valid and open.
    ///  - The caller must ensure that the given RawFd is not used simultaneously in other places. This will lead to
    ///    behavior that the author is probably not aware of.
    ///
    pub fn from(raw: RawFd) -> Result<Self> {
        Ok(RawFdBridge { inner: T::IoProxy::new(raw) })
    }

    /// Consumes self and returns the underlying RawFd.
    pub fn into_raw(self) -> RawFd {
        self.inner.as_inner().as_raw_fd()
    }

    // This expose a way to do any IO calls by the user code.
    //
    // # Considerations
    // - The IoResult generated in `f` shall be propagated as return value to make sure underlying selector will work correctly.
    //
    pub fn io_call<F, R>(&self, f: F) -> IoResult<R>
    where
        F: FnOnce(RawFd) -> IoResult<R>,
    {
        self.inner.io_call(|raw_fd| f(*raw_fd))
    }
}

impl_io_source_proxy!(RawFdBridge<T>, inner);
