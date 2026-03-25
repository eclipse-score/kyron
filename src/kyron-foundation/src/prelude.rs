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

#[cfg(not(feature = "bazel_build_iceoryx2_qnx8"))]
pub use iceoryx2_bb_container;
#[cfg(feature = "bazel_build_iceoryx2_qnx8")]
pub use iceoryx2_bb_container_qnx8 as iceoryx2_bb_container;

#[cfg(not(feature = "bazel_build_iceoryx2_qnx8"))]
pub use iceoryx2_bb_elementary;
#[cfg(feature = "bazel_build_iceoryx2_qnx8")]
pub use iceoryx2_bb_elementary_qnx8 as iceoryx2_bb_elementary;

#[cfg(not(feature = "bazel_build_iceoryx2_qnx8"))]
pub use iceoryx2_bb_elementary_traits;
#[cfg(feature = "bazel_build_iceoryx2_qnx8")]
pub use iceoryx2_bb_elementary_traits_qnx8 as iceoryx2_bb_elementary_traits;

#[cfg(not(feature = "bazel_build_iceoryx2_qnx8"))]
pub use iceoryx2_bb_lock_free;
#[cfg(feature = "bazel_build_iceoryx2_qnx8")]
pub use iceoryx2_bb_lock_free_qnx8 as iceoryx2_bb_lock_free;

#[cfg(not(feature = "bazel_build_iceoryx2_qnx8"))]
pub use iceoryx2_bb_memory;
#[cfg(feature = "bazel_build_iceoryx2_qnx8")]
pub use iceoryx2_bb_memory_qnx8 as iceoryx2_bb_memory;

#[cfg(not(feature = "bazel_build_iceoryx2_qnx8"))]
pub use iceoryx2_bb_posix;
#[cfg(feature = "bazel_build_iceoryx2_qnx8")]
pub use iceoryx2_bb_posix_qnx8 as iceoryx2_bb_posix;

pub use crate::sync::foundation_atomic::*;
pub use iceoryx2_bb_elementary::scope_guard::*;
pub use iceoryx2_bb_lock_free::*;

pub use crate::containers::*;
pub use crate::types::*;
pub use iceoryx2_bb_memory::pool_allocator::*;

#[cfg(not(any(feature = "tracing", feature = "log", feature = "score-log")))]
compile_error!("At least one of features 'tracing', 'log' or 'score-log' must be enabled!");

#[cfg(feature = "tracing")]
pub use tracing::{debug, error, info, span, trace, warn, Level};

#[cfg(feature = "log")]
pub use log::{debug, error, info, trace, warn, Level};

#[cfg(feature = "score-log")]
pub use score_log::{
    self, debug, error, fatal,
    fmt::{DebugStruct, DebugTuple, Error, FormatSpec, ScoreDebug, ScoreWrite, Writer},
    info, log_enabled, trace, warn, Level, ScoreDebug,
};

// Adapter super trait with blanket implementations to use wherever ScoreDebug is needed.
// This helps to avoid many cfg conditions and code duplications.
#[cfg(feature = "score-log")]
pub trait ScoreLogDebug: ScoreDebug {}
#[cfg(not(feature = "score-log"))]
pub trait ScoreLogDebug {}

// Blanket impls
#[cfg(feature = "score-log")]
impl<T: ScoreDebug> ScoreLogDebug for T {}
#[cfg(not(feature = "score-log"))]
impl<T> ScoreLogDebug for T {}

#[cfg(feature = "log")]
#[macro_export]
macro_rules! tracing_adapter {
    // Case 1: key = ?value, then more
    ($key:ident = ?$val:expr, $($rest:tt)*) => {
        $crate::tracing_adapter!(@accum concat!(stringify!($key), "={:?}, "), $val; $($rest)*);
    };

    // Case 2: ?value, then more
    (?$val:expr, $($rest:tt)*) => {
        $crate::tracing_adapter!(@accum "{:?}, ", $val; $($rest)*);
    };

    // Message only case
    ($msg:literal) => {
        $crate::prelude::trace!($msg);
    };

    // Internal accumulator: finish
    (@accum $fmt:expr, $($vals:expr),*; $msg:literal) => {
        $crate::prelude::trace!(concat!($fmt, $msg), $($vals),*);
    };

    // Continue accumulating: key = ?value
    (@accum $fmt:expr, $($vals:expr),*; $key:ident = ?$val:expr, $($rest:tt)*) => {
        $crate::tracing_adapter!(
            @accum concat!($fmt, stringify!($key), "={:?}, "),
            $($vals,)* $val;
            $($rest)*
        );
    };

    // Continue accumulating: ?value
    (@accum $fmt:expr, $($vals:expr),*; ?$val:expr, $($rest:tt)*) => {
        $crate::tracing_adapter!(
            @accum concat!($fmt, "{:?}, "),
            $($vals,)* $val;
            $($rest)*
        );
    };
}

#[cfg(feature = "tracing")]
#[macro_export]
macro_rules! tracing_adapter {
     ($($t:tt)*) => {
         $crate::prelude::trace!($($t)*);
    };
}

#[cfg(any(feature = "tracing", feature = "log"))]
/// Proxy for `trace!` macro, works with `tracing` and `log` features.
/// `tracing` key-value syntax (`name =? value`) is supported for `log` derived crates.
/// NOTE: provided variables cannot be interpolated into the message literal directly.
pub use tracing_adapter;
