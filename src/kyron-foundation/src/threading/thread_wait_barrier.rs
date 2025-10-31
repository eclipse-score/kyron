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

use ::core::time::Duration;
use std::sync::{Arc, Condvar, Mutex};

use crate::prelude::*;

/// A barrier that waits for a specified number of threads to signal readiness.
///
/// This struct is used to synchronize threads by waiting for all threads to signal readiness.
pub struct ThreadWaitBarrier {
    mtx: std::sync::Mutex<u32>,
    cv: std::sync::Condvar,
    ntf_cnt: FoundationAtomicU32,
    expected_cnt: u32,
}

/// A notifier that signals readiness to the `ThreadWaitBarrier`.
///
/// Each thread can use this struct to notify the barrier that it is ready.
pub struct ThreadReadyNotifier {
    inner: Arc<ThreadWaitBarrier>,
}

impl ThreadReadyNotifier {
    /// Signals readiness to the associated `ThreadWaitBarrier`.
    ///
    /// This method decreases the internal counter of the barrier and notifies it.
    pub fn ready(self) {
        {
            let mut value = self.inner.mtx.lock().unwrap();
            *value -= 1;
        }
        self.inner.cv.notify_one();
    }
}

impl ThreadWaitBarrier {
    /// Creates a new `ThreadWaitBarrier` that waits for the specified number of threads.
    ///
    /// # Arguments
    ///
    /// * `thread_to_wait` - The number of threads to wait for.
    pub fn new(thread_to_wait: u32) -> Self {
        Self {
            mtx: Mutex::new(thread_to_wait),
            cv: Condvar::new(),
            ntf_cnt: FoundationAtomicU32::new(0),
            expected_cnt: thread_to_wait,
        }
    }

    /// Returns a `ThreadReadyNotifier` for signaling readiness.
    ///
    /// Returns `None` if the maximum number of notifiers has already been created.
    pub fn get_notifier(self: &Arc<Self>) -> Option<ThreadReadyNotifier> {
        let new_cnt = self
            .ntf_cnt
            .fetch_update(
                ::core::sync::atomic::Ordering::SeqCst,
                ::core::sync::atomic::Ordering::SeqCst,
                |current| {
                    if current >= self.expected_cnt {
                        None
                    } else {
                        Some(current + 1)
                    }
                },
            )
            .unwrap_or_else(|e| e);

        if new_cnt >= self.expected_cnt {
            None
        } else {
            Some(ThreadReadyNotifier { inner: self.clone() })
        }
    }

    /// Waits for all threads to signal readiness or times out.
    ///
    /// # Arguments
    ///
    /// * `dur` - The maximum duration to wait.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all threads signaled readiness within the timeout.
    /// * `Err(CommonErrors::Timeout)` - If the timeout was reached before all threads signaled readiness.
    pub fn wait_for_all(&self, dur: Duration) -> Result<(), CommonErrors> {
        let res = self.cv.wait_timeout_while(self.mtx.lock().unwrap(), dur, |cond| *cond > 0).unwrap();

        if res.1.timed_out() {
            Err(CommonErrors::Timeout)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use super::*;
    use ::core::time::Duration;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_thread_wait_barrier_all_threads_ready() {
        let barrier = Arc::new(ThreadWaitBarrier::new(3));
        let mut handles = vec![];

        for _ in 0..3 {
            let notifier = barrier.get_notifier().unwrap();
            handles.push(thread::spawn(move || {
                thread::sleep(Duration::from_millis(100));
                notifier.ready();
            }));
        }

        assert!(barrier.wait_for_all(Duration::from_secs(1)).is_ok());

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_thread_wait_barrier_timeout() {
        let barrier = Arc::new(ThreadWaitBarrier::new(3));
        let notifier = barrier.get_notifier().unwrap();

        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(500));
            notifier.ready();
        });

        // Only one thread will notify, so this should timeout
        assert!(barrier.wait_for_all(Duration::from_millis(100)).is_err());

        handle.join().unwrap();
    }

    #[test]
    fn test_thread_wait_barrier_exceed_notifiers() {
        let barrier = Arc::new(ThreadWaitBarrier::new(2));

        // Create more notifiers than expected
        let notifier1 = barrier.get_notifier().unwrap();
        let notifier2 = barrier.get_notifier().unwrap();
        let extra_notifier = barrier.get_notifier();

        assert!(extra_notifier.is_none()); // No more notifiers should be allowed

        notifier1.ready();
        notifier2.ready();

        assert!(barrier.wait_for_all(Duration::from_secs(1)).is_ok());
    }
}
