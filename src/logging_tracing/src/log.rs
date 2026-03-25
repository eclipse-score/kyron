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

pub struct LogLibraryImpl {}

impl TryFrom<crate::LogAndTraceBuilder> for LogLibraryImpl {
    type Error = ();

    fn try_from(builder: crate::LogAndTraceBuilder) -> Result<Self, Self::Error> {
        env_logger::builder()
            .filter_level(Into::<kyron_foundation::prelude::Level>::into(builder.log_level).to_level_filter())
            .init();

        internal_iceoryx2::set_log_level(builder.log_level);

        Ok(Self {})
    }
}

impl From<crate::Level> for kyron_foundation::prelude::Level {
    fn from(level: crate::Level) -> Self {
        match level {
            crate::Level::FATAL => kyron_foundation::prelude::Level::Error,
            crate::Level::ERROR => kyron_foundation::prelude::Level::Error,
            crate::Level::WARN => kyron_foundation::prelude::Level::Warn,
            crate::Level::INFO => kyron_foundation::prelude::Level::Info,
            crate::Level::DEBUG => kyron_foundation::prelude::Level::Debug,
            crate::Level::TRACE => kyron_foundation::prelude::Level::Trace,
        }
    }
}

mod internal_iceoryx2 {
    #[cfg(feature = "bazel_build_iceoryx2_qnx8")]
    use iceoryx2_qnx8 as iceoryx2;

    pub use iceoryx2::prelude::LogLevel;
    pub fn set_log_level(level: crate::Level) {
        let iceoryx_level = match level {
            crate::Level::FATAL => LogLevel::Error,
            crate::Level::ERROR => LogLevel::Error,
            crate::Level::WARN => LogLevel::Warn,
            crate::Level::INFO => LogLevel::Info,
            crate::Level::DEBUG => LogLevel::Debug,
            crate::Level::TRACE => LogLevel::Trace,
        };
        iceoryx2::prelude::set_log_level(iceoryx_level);
    }
}
