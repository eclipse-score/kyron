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

// use foundation::prelude::*;
use std::{
    cell::Cell,
    ops::{Add, Sub},
    time::Duration,
};

/// A measurement of a monotonically nondecreasing clock. This is thin wrapper for std version so our runtime can choose
/// to use different source without changing the code base.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Instant(std::time::Instant);

impl Instant {
    /// Returns the duration since the clock started.
    pub fn elapsed(&self) -> std::time::Duration {
        self.0.elapsed()
    }

    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.0.saturating_duration_since(earlier.0)
    }

    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.0.duration_since(earlier.0)
    }

    pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
        self.0.checked_duration_since(earlier.0)
    }

    pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
        self.0.checked_add(duration).map(Instant)
    }
}

impl Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Self::Output {
        Instant(self.0.checked_sub(rhs).expect("overflow when subtracting duration from instant"))
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Self::Output {
        Instant(self.0.checked_add(rhs).expect("overflow when adding duration to instant"))
    }
}

impl Sub for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Self::Output {
        self.0.duration_since(rhs.0)
    }
}

thread_local! {
    /// A thread-local clock that provides the current time as an `Instant`.
    pub static LAST_TIME: Cell<std::time::Instant> = Cell::new(std::time::Instant::now());
}
/// A clock that provides the current time as an `Instant`.
pub struct Clock;

impl Clock {
    /// Returns the current time as an `Instant`.
    pub fn now() -> Instant {
        // let now = std::time::Instant::now();
        // let a = Instant(LAST_TIME.with(|cell| {
        //     let last_time = cell.get();
        //     if now > last_time {
        //         cell.set(now);
        //         now
        //     } else {
        //         last_time
        //     }
        // }));

        // info!("now was {:?}  we returned {:?}", now, a);
        Instant(std::time::Instant::now())
    }

    /// Returns the current time as a `Duration` since the clock started.
    pub fn now_duration() -> Duration {
        Self::now().elapsed()
    }
}
