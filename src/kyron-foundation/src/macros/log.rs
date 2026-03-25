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

//! Provide logging macros with context.
//! The context is not applicable if score-log is not used, and the macros are proxied to the original ones without context.

/// Proxy for `fatal!`.
#[clippy::format_args]
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! fatal {
    ($ctx:expr, $($arg:tt)*) => {{
        use $crate::prelude::score_log;
        $crate::prelude::fatal!(context: $ctx, $($arg)*)
    }};
}
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! fatal {
    ($ctx:expr, $($arg:tt)*) => ($crate::prelude::fatal!($($arg)*));
}

/// Proxy for `error!`.
#[clippy::format_args]
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! error {
    ($ctx:expr, $($arg:tt)*) => {{
        use $crate::prelude::score_log;
        $crate::prelude::error!(context: $ctx, $($arg)*)
    }};
}
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! error {
    ($ctx:expr, $($arg:tt)*) => ($crate::prelude::error!($($arg)*));
}

/// Proxy for `warn!`.
#[clippy::format_args]
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! warn {
    ($ctx:expr, $($arg:tt)*) => {{
        use $crate::prelude::score_log;
        $crate::prelude::warn!(context: $ctx, $($arg)*)
    }};
}
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! warn {
    ($ctx:expr, $($arg:tt)*) => ($crate::prelude::warn!($($arg)*));
}

/// Proxy for `info!`.
#[clippy::format_args]
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! info {
    ($ctx:expr, $($arg:tt)*) => {{
        use $crate::prelude::score_log;
        $crate::prelude::info!(context: $ctx, $($arg)*)
    }};
}
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! info {
    ($ctx:expr, $($arg:tt)*) => ($crate::prelude::info!($($arg)*));
}

/// Proxy for `debug!`.
#[clippy::format_args]
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! debug {
    ($ctx:expr, $($arg:tt)*) => {{
        use $crate::prelude::score_log;
        $crate::prelude::debug!(context: $ctx, $($arg)*)
    }};
}
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! debug {
    ($ctx:expr, $($arg:tt)*) => ($crate::prelude::debug!($($arg)*));
}

/// Proxy for `trace!`.
#[clippy::format_args]
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! trace {
    ($ctx:expr, $($arg:tt)*) => {{
        use $crate::prelude::score_log;
        $crate::prelude::trace!(context: $ctx, $($arg)*)
    }};
}
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! trace {
    ($ctx:expr, $($arg:tt)*) => ($crate::prelude::trace!($($arg)*));
}
