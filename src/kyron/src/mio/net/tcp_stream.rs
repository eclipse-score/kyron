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

#![allow(dead_code, unused, clippy::module_name_repetitions, clippy::upper_case_acronyms)]

use core::net::SocketAddr;
use core::ops::Deref;
use std::io::Read;
use std::io::Write;
use std::os::fd::AsRawFd;

use crate::impl_io_source_proxy;
use crate::mio::types::IoCall;
use crate::mio::types::IoResult;
use crate::mio::types::IoSelector;

pub struct TcpStreamBridge<T: IoSelector> {
    inner: T::IoProxy<std::net::TcpStream>,
}

impl<T> core::fmt::Debug for TcpStreamBridge<T>
where
    T: IoSelector,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TcpStreamBridge<fd: {:?}>", self.inner.as_inner().as_raw_fd())
    }
}

kyron_foundation::impl_score_debug_for_type_t!(TcpStreamBridge<T>, IoSelector, |x: &TcpStreamBridge<T>| format!(
    "TcpStreamBridge<fd: {:?}>",
    x.inner.as_inner().as_raw_fd()
));

impl<T: IoSelector> TcpStreamBridge<T> {
    pub(crate) fn new(s: std::net::TcpStream) -> IoResult<Self> {
        s.set_nonblocking(true)?;

        Ok(TcpStreamBridge {
            inner: T::IoProxy::new(s),
        })
    }

    pub(crate) fn connect(addr: SocketAddr) -> IoResult<Self> {
        let s = std::net::TcpStream::connect(addr)?;
        Self::new(s)
    }

    pub fn read(&self, buf: &mut [u8]) -> IoResult<usize> {
        self.inner.io_call(|mut stream| stream.read(buf))
    }

    pub fn write(&self, buf: &[u8]) -> IoResult<usize> {
        self.inner.io_call(|mut stream| stream.write(buf))
    }

    pub fn shutdown(&self, how: std::net::Shutdown) -> IoResult<()> {
        self.inner.as_inner().shutdown(how)
    }

    pub fn local_addr(&self) -> IoResult<SocketAddr> {
        self.inner.as_inner().local_addr()
    }

    pub fn peer_addr(&self) -> IoResult<SocketAddr> {
        self.inner.as_inner().peer_addr()
    }

    pub fn ttl(&self) -> IoResult<u32> {
        self.inner.as_inner().ttl()
    }

    pub fn set_ttl(&self, ttl: u8) -> IoResult<()> {
        self.inner.as_inner().set_ttl(ttl as u32)
    }
}

impl<T: IoSelector> std::io::Read for TcpStreamBridge<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.io_call(|mut stream| stream.read(buf))
    }
}

impl<T: IoSelector> std::io::Write for TcpStreamBridge<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.io_call(|mut stream| stream.write(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(()) // no op for tcp socket
    }
}

impl_io_source_proxy!(TcpStreamBridge<T>, inner);
