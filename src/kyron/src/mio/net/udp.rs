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
use std::{
    io::Error,
    net::{ToSocketAddrs, UdpSocket},
    os::fd::AsRawFd,
};

use crate::{
    impl_io_source_proxy,
    mio::types::{IoCall, IoResult, IoSelector},
};

pub struct UdpSocketBridge<T: IoSelector> {
    inner: T::IoProxy<UdpSocket>,
}

impl<T> core::fmt::Debug for UdpSocketBridge<T>
where
    T: IoSelector,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "UdpSocketBridge<fd: {:?}>", self.inner.as_inner().as_raw_fd())
    }
}

kyron_foundation::impl_score_debug_for_type_t!(UdpSocketBridge<T>, IoSelector, |x: &UdpSocketBridge<T>| format!(
    "UdpSocketBridge<fd: {:?}>",
    x.inner.as_inner().as_raw_fd()
));

impl<T: IoSelector> UdpSocketBridge<T> {
    pub fn bind(addr: SocketAddr) -> IoResult<UdpSocketBridge<T>> {
        //TODO: not finished
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;

        Ok(UdpSocketBridge {
            inner: T::IoProxy::new(socket),
        })
    }

    pub fn connect(&self, addr: SocketAddr) -> IoResult<()> {
        self.inner.io_call(|socket| socket.connect(addr))
    }

    pub fn send(&self, buf: &[u8]) -> IoResult<usize> {
        self.inner.io_call(|socket| socket.send(buf))
    }

    pub fn send_to(&self, buf: &[u8], addr: SocketAddr) -> IoResult<usize> {
        self.inner.io_call(|socket| socket.send_to(buf, addr))
    }

    pub fn recv(&self, buf: &mut [u8]) -> IoResult<usize> {
        self.inner.io_call(|socket| socket.recv(buf))
    }

    pub fn recv_from(&self, buf: &mut [u8]) -> IoResult<(usize, SocketAddr)> {
        self.inner.io_call(|socket| socket.recv_from(buf))
    }

    pub fn ttl(&self) -> IoResult<u32> {
        self.inner.as_inner().ttl()
    }

    pub fn set_ttl(&self, ttl: u8) -> IoResult<()> {
        self.inner.as_inner().set_ttl(ttl as u32)
    }

    pub fn local_addr(&self) -> IoResult<SocketAddr> {
        self.inner.as_inner().local_addr()
    }

    pub fn broadcast(&self) -> IoResult<bool> {
        self.inner.as_inner().broadcast()
    }

    pub fn set_broadcast(&self, broadcast: bool) -> IoResult<()> {
        self.inner.as_inner().set_broadcast(broadcast)
    }

    pub fn take_error(&self) -> IoResult<Option<Error>> {
        self.inner.as_inner().take_error()
    }
}

impl_io_source_proxy!(UdpSocketBridge<T>, inner);
