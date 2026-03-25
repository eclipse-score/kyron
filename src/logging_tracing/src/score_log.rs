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

use crate::LogAndTraceBuilder;
#[cfg(feature = "stdout-logger")]
use score_log::LevelFilter;

pub struct ScoreLogLibraryImpl;

impl TryFrom<LogAndTraceBuilder> for ScoreLogLibraryImpl {
    type Error = ();

    fn try_from(builder: LogAndTraceBuilder) -> Result<Self, Self::Error> {
        if builder.enable_logging {
            #[cfg(feature = "stdout-logger")]
            {
                let level = builder.log_level;
                stdout_logger::StdoutLoggerBuilder::new()
                    .log_level(level.into())
                    .context(builder.score_log_config.context)
                    .show_module(builder.score_log_config.show_module)
                    .show_file(builder.score_log_config.show_file)
                    .show_line(builder.score_log_config.show_line)
                    .show_timestamp(builder.score_log_config.show_timestamp)
                    .set_as_default_logger();
            }

            #[cfg(feature = "score-log-bridge")]
            {
                let mut sc_builder = score_log_bridge::ScoreLogBridgeBuilder::new()
                    .context(builder.score_log_config.context)
                    .show_module(builder.score_log_config.show_module)
                    .show_file(builder.score_log_config.show_file)
                    .show_line(builder.score_log_config.show_line);
                if let Some(path) = builder.score_log_config.config_path {
                    sc_builder = sc_builder.config(path);
                }
                sc_builder.set_as_default_logger();
            }
        }
        Ok(ScoreLogLibraryImpl)
    }
}

#[cfg(feature = "stdout-logger")]
impl From<crate::Level> for LevelFilter {
    fn from(level: crate::Level) -> Self {
        match level {
            crate::Level::FATAL => LevelFilter::Fatal,
            crate::Level::ERROR => LevelFilter::Error,
            crate::Level::WARN => LevelFilter::Warn,
            crate::Level::INFO => LevelFilter::Info,
            crate::Level::DEBUG => LevelFilter::Debug,
            crate::Level::TRACE => LevelFilter::Trace,
        }
    }
}
