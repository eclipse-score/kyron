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

/// Macro to import score_log related traits and macros if the feature is enabled.
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! import_score_log {
    () => {
        use $crate::prelude::{score_log, ScoreDebug};
    };
}
/// Macro to import score_log related traits and macros if the feature is enabled.
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! import_score_log {
    () => {};
}

/// Implement ScoreDebug for a newtype struct by formatting the inner value with a custom closure.
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! impl_score_debug_for_newtype {
    ($t:ty, $closure:expr) => {
        const _: () = {
            use $crate::prelude::{Error, FormatSpec, ScoreDebug, Writer};
            impl ScoreDebug for $t {
                fn fmt(&self, f: Writer, spec: &FormatSpec) -> Result<(), Error> {
                    let formatted = $closure(self);
                    f.write_str(&formatted, spec)
                }
            }
        };
    };
}
/// Implement ScoreDebug for a newtype struct by formatting the inner value with a custom closure.
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! impl_score_debug_for_newtype {
    ($t:ty, $closure:expr) => {};
}

/// Implement ScoreDebug for a type T with a custom closure that formats the value.
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! impl_score_debug_for_type_t {
    ($type:ty, $t:path, $closure:expr) => {
        const _: () = {
            use $crate::prelude::{Error, FormatSpec, ScoreDebug, Writer};
            impl<T> ScoreDebug for $type
            where
                T: $t,
            {
                fn fmt(&self, f: Writer, spec: &FormatSpec) -> std::result::Result<(), Error> {
                    let formatted = $closure(self);
                    f.write_str(&formatted, spec)
                }
            }
        };
    };
}
/// Implement ScoreDebug for a type T with a custom closure that formats the value.
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! impl_score_debug_for_type_t {
    ($type:ty, $t:path, $closure:expr) => {};
}

/// Derive ScoreDebug for a struct by adding it to the list of derives if score-log is enabled.
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! derive_score_debug_for_struct {
    (
        #[derive($($derives:tt)*)]
        $vis:vis struct $name:ident $($rest:tt)*
    ) => {
        #[derive($($derives)*, ScoreDebug)]
        $vis struct $name $($rest)*
    };
    // This arm allows to support additional attributes on the struct or comments.
    (
        #[derive($($derives:tt)*)]
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $($rest:tt)*
    ) => {

        #[derive($($derives)*, ScoreDebug)]
        $(#[$meta])*
        $vis struct $name $($rest)*
    };
}
/// Derive ScoreDebug for a struct by adding it to the list of derives if score-log is enabled.
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! derive_score_debug_for_struct {
    (
        #[derive($($derives:tt)*)]
        $vis:vis struct $name:ident $($rest:tt)*
    ) => {
        #[derive($($derives)*)]
        $vis struct $name $($rest)*
    };
    // This arm allows to support additional attributes on the struct or comments.
    (
        #[derive($($derives:tt)*)]
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $($rest:tt)*
    ) => {
        #[derive($($derives)*)]
        $(#[$meta])*
        $vis struct $name $($rest)*
    };
}

/// Derive ScoreDebug for an enum by adding it to the list of derives if score-log is enabled.
#[cfg(feature = "score-log")]
#[macro_export]
macro_rules! derive_score_debug_for_enum {
    (
        #[derive($($derives:tt)*)]
        $vis:vis enum $name:ident $($rest:tt)*
    ) => {
        #[derive($($derives)*, ScoreDebug)]
        $vis enum $name $($rest)*
    };
    // This arm allows to support additional attributes on the enum or comments.
    (
        #[derive($($derives:tt)*)]
        $(#[$meta:meta])*
        $vis:vis enum $name:ident $($rest:tt)*
    ) => {

        #[derive($($derives)*, ScoreDebug)]
        $(#[$meta])*
        $vis enum $name $($rest)*
    };
}
/// Derive ScoreDebug for an enum by adding it to the list of derives if score-log is enabled.
#[cfg(not(feature = "score-log"))]
#[macro_export]
macro_rules! derive_score_debug_for_enum {
    (
        #[derive($($derives:tt)*)]
        $vis:vis enum $name:ident $($rest:tt)*
    ) => {
        #[derive($($derives)*)]
        $vis enum $name $($rest)*
    };
    // This arm allows to support additional attributes on the enum or comments.
    (
        #[derive($($derives:tt)*)]
        $(#[$meta:meta])*
        $vis:vis enum $name:ident $($rest:tt)*
    ) => {
        #[derive($($derives)*)]
        $(#[$meta])*
        $vis enum $name $($rest)*
    };
}
