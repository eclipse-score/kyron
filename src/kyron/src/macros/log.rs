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

//! Provide logging macros for kyron with "KYRN" context if score-log is used.
//! The context is not applicable if score-log is not used, and the macros are proxied to the original ones without context.

#![allow(unused_macros)]

#[allow(dead_code)]
pub(crate) const CONTEXT: &str = "KYRN";

#[clippy::format_args]
macro_rules! fatal {
    ($($arg:tt)+) => (kyron_foundation::fatal!($crate::macros::log::CONTEXT, $($arg)+));
}

#[clippy::format_args]
macro_rules! error {
    ($($arg:tt)+) => (kyron_foundation::error!($crate::macros::log::CONTEXT, $($arg)+));
}

#[clippy::format_args]
macro_rules! warning {
    ($($arg:tt)+) => (kyron_foundation::warn!($crate::macros::log::CONTEXT, $($arg)+));
}

#[clippy::format_args]
macro_rules! info {
    ($($arg:tt)+) => (kyron_foundation::info!($crate::macros::log::CONTEXT, $($arg)+));
}

#[clippy::format_args]
macro_rules! debug {
    ($($arg:tt)+) => (kyron_foundation::debug!($crate::macros::log::CONTEXT, $($arg)+));
}

#[clippy::format_args]
macro_rules! trace {
    ($($arg:tt)+) => (kyron_foundation::trace!($crate::macros::log::CONTEXT, $($arg)+));
}

#[allow(unused_imports)]
// Export macros from this module
pub(crate) use {debug, error, fatal, info, trace, warning as warn};
