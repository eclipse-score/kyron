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

use crate::impl_io_source_proxy;
use crate::mio::{
    net::tcp_stream::TcpStreamBridge,
    types::{IoCall, IoResult, IoSelector},
};
use core::net::SocketAddr;
use std::{net::TcpListener, os::fd::AsRawFd};

pub struct TcpListenerBridge<T: IoSelector> {
    inner: T::IoProxy<TcpListener>,
}

impl<T> core::fmt::Debug for TcpListenerBridge<T>
where
    T: IoSelector,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "TcpListenerBridge<fd: {:?}>", self.inner.as_inner().as_raw_fd())
    }
}

kyron_foundation::impl_score_debug_for_type_t!(TcpListenerBridge<T>, IoSelector, |x: &TcpListenerBridge<T>| format!(
    "TcpListenerBridge<fd: {:?}>",
    x.inner.as_inner().as_raw_fd()
));

impl<T: IoSelector> TcpListenerBridge<T> {
    pub fn bind(addr: SocketAddr) -> IoResult<TcpListenerBridge<T>> {
        let listener = TcpListener::bind(addr)?;
        listener.set_nonblocking(true)?;

        Ok(TcpListenerBridge {
            inner: T::IoProxy::new(listener),
        })
    }

    pub fn accept(&self) -> IoResult<(TcpStreamBridge<T>, SocketAddr)> {
        self.inner
            .io_call(|listener| listener.accept())
            .and_then(|(stream, addr)| {
                let bridge = TcpStreamBridge::new(stream)?;
                Ok((bridge, addr))
            })
    }

    pub fn local_addr(&self) -> IoResult<SocketAddr> {
        self.inner.as_inner().local_addr()
    }

    pub fn ttl(&self) -> IoResult<u32> {
        self.inner.as_inner().ttl()
    }

    pub fn set_ttl(&self, ttl: u8) -> IoResult<()> {
        self.inner.as_inner().set_ttl(ttl as u32)
    }
}

impl_io_source_proxy!(TcpListenerBridge<T>, inner);
