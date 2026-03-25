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

pub mod prelude;
#[cfg(feature = "score-log-bridge")]
use std::path::PathBuf;

#[cfg(feature = "tracing")]
mod tracing;
#[cfg(feature = "tracing")]
type LogTraceLibrary = tracing::TracingLibraryImpl;

#[cfg(feature = "log")]
mod log;
#[cfg(feature = "log")]
type LogTraceLibrary = log::LogLibraryImpl;

#[cfg(feature = "score-log")]
mod score_log;
#[cfg(feature = "score-log")]
type LogTraceLibrary = score_log::ScoreLogLibraryImpl;

#[derive(Debug, Clone, Copy)]
pub enum Level {
    FATAL = 0,
    ERROR = 1,
    WARN = 2,
    INFO = 3,
    DEBUG = 4,
    TRACE = 5,
}

#[derive(Debug, Clone, Copy)]
pub enum LogMode {
    Logging,
    Tracing,
}

#[derive(Debug, Clone, Copy)]
pub enum TraceScope {
    ///
    /// Logs events from App and if `traced` is running also logs system events (if not, no kernel traces will be there)
    ///
    AppScope,

    ///
    /// Logs app event to `traced` and the events need to be dumped by `perfetto` tool. This is useful once you want to trace multiple apps
    ///
    SystemScope,
}

#[cfg(feature = "score-log")]
struct ScoreLogConfig {
    context: &'static str,
    show_module: bool,
    show_file: bool,
    show_line: bool,
    #[cfg(feature = "stdout-logger")]
    show_timestamp: bool,
    #[cfg(feature = "score-log-bridge")]
    config_path: Option<PathBuf>,
}

pub struct LogAndTraceBuilder {
    log_level: Level,
    enable_tracing: Option<TraceScope>,
    enable_logging: bool,
    #[cfg(feature = "score-log")]
    score_log_config: ScoreLogConfig,
}

impl Default for LogAndTraceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LogAndTraceBuilder {
    pub fn new() -> Self {
        Self {
            log_level: Level::INFO,
            enable_tracing: None,
            enable_logging: false,
            #[cfg(feature = "score-log")]
            score_log_config: ScoreLogConfig {
                context: "DFLT",
                show_module: false,
                show_file: false,
                show_line: false,
                #[cfg(feature = "stdout-logger")]
                show_timestamp: false,
                #[cfg(feature = "score-log-bridge")]
                config_path: None,
            },
        }
    }

    /// Sets the global log level except for score_log + score-log-bridge/bazel build, which needs to be set in the configuration file.
    pub fn global_log_level(mut self, level: Level) -> Self {
        self.log_level = level;
        self
    }

    ///
    /// Enables tracing in given mode (applicable for 'tracing + perfetto' feature only). Not supported on QNX target now.
    ///
    pub fn enable_tracing(mut self, scope: TraceScope) -> Self {
        self.enable_tracing = Some(scope);
        self
    }

    ///
    /// Enables logging
    ///
    pub fn enable_logging(mut self, enable: bool) -> Self {
        self.enable_logging = enable;
        self
    }

    #[cfg(feature = "score-log")]
    /// Sets the context for score_log.
    pub fn context(mut self, context: &'static str) -> Self {
        self.score_log_config.context = context;
        self
    }

    #[cfg(feature = "score-log")]
    /// Configures whether to show the module path in score_log output.
    pub fn show_module(mut self, show: bool) -> Self {
        self.score_log_config.show_module = show;
        self
    }

    #[cfg(feature = "score-log")]
    /// Configures whether to show the file name in score_log output.
    pub fn show_file(mut self, show: bool) -> Self {
        self.score_log_config.show_file = show;
        self
    }

    #[cfg(feature = "score-log")]
    /// Configures whether to show the line number in score_log output.
    pub fn show_line(mut self, show: bool) -> Self {
        self.score_log_config.show_line = show;
        self
    }

    #[cfg(feature = "stdout-logger")]
    /// Configures whether to show the timestamp in score_log output (only for stdout-logger/cargo build).
    pub fn show_timestamp(mut self, show: bool) -> Self {
        self.score_log_config.show_timestamp = show;
        self
    }
    #[cfg(feature = "score-log-bridge")]
    /// Configures the path to the score_log configuration file (only for score-log-bridge/bazel build).
    pub fn config_path(mut self, path: PathBuf) -> Self {
        self.score_log_config.config_path = Some(path);
        self
    }

    ///
    /// Builds and starts log and trace backend
    ///
    #[allow(clippy::result_unit_err)]
    pub fn build(self) -> Result<LogTraceLibrary, ()> {
        LogTraceLibrary::try_from(self)
    }
}
