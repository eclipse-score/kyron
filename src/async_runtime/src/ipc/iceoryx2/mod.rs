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
use iceoryx2::port::listener::ListenerCreateError;
use iceoryx2::service;

// Should be part of iceoryx2 module, this will be RUNTIME depended as Events are IO based
pub trait EventBuilderAsyncExt {
    type Service: service::Service;
    type ListenerType;

    /// Creates the [`Listener`] async port or returns a [`ListenerCreateError`] on failure.
    fn create_async(self) -> Result<Self::ListenerType, ListenerCreateError>;
}

mod event;

pub use event::Listener as AsyncListener;
