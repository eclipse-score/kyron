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

use core::net::{AddrParseError, SocketAddr};
use std::io::ErrorKind;

use crate::macros::log::*;

use crate::net::NetResult;

pub async fn resolve_as_single_address<A: ToSocketAddrs>(addrs: A) -> NetResult<SocketAddr> {
    let mut iter = addrs.to_socket_addrs().await?;
    iter.next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "No address found"))
}

pub trait ToSocketAddrs {
    type Iter: Iterator<Item = SocketAddr>;

    // Required method
    fn to_socket_addrs(&self) -> impl core::future::Future<Output = NetResult<Self::Iter>> + Send;
}

impl ToSocketAddrs for SocketAddr {
    type Iter = core::iter::Once<SocketAddr>;

    #[allow(clippy::manual_async_fn)] // Keep same fn declaration as trait
    fn to_socket_addrs(&self) -> impl core::future::Future<Output = NetResult<Self::Iter>> + Send {
        async move { Ok(core::iter::once(*self)) }
    }
}

impl ToSocketAddrs for &str {
    type Iter = core::iter::Once<SocketAddr>;

    #[allow(clippy::manual_async_fn)] // Keep same fn declaration as trait
    fn to_socket_addrs(&self) -> impl core::future::Future<Output = NetResult<Self::Iter>> + Send {
        async move {
            let parsed: Result<SocketAddr, AddrParseError> = self.parse();
            match parsed {
                Ok(addr) => Ok(core::iter::once(addr)),
                Err(e) => {
                    error!("Failed to parse socket address: {}", e);
                    Err(std::io::Error::from(ErrorKind::Unsupported))
                },
            }
        }
    }
}

impl ToSocketAddrs for String {
    type Iter = core::iter::Once<SocketAddr>;

    #[allow(clippy::manual_async_fn)] // Keep same fn declaration as trait
    fn to_socket_addrs(&self) -> impl core::future::Future<Output = NetResult<Self::Iter>> + Send {
        async move { self.as_str().to_socket_addrs().await }
    }
}
