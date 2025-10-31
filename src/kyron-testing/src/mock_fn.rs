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

use kyron_foundation::containers::reusable_objects::ReusableObjectTrait;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

#[macro_export]
macro_rules! build_with_location {
    ($expr:expr $(,)?) => {
        $expr.build_with_location(format!("{}:{}", file!(), line!()))
    };
}
pub trait CallableTrait<OutType> {
    fn call(&mut self) -> OutType;
}

#[derive(Clone, Default)]
pub struct MockFnBuilder<OutType>(MockFn<OutType>);

///
/// A mock object that can be used to monitor the invocation count of its method, i.e. call().
/// Each invocation returns predefined values configured via will_once() or will_repeatedly().
///
pub struct MockFn<OutType> {
    call_count: AtomicUsize,
    default_value: OutType,
    expected_count: isize,
    is_built: bool,
    is_times_set: bool,
    is_will_once_set: bool,
    is_will_repeatedly_set: bool,
    min_count: usize,
    returns: VecDeque<OutType>,
    location: Option<String>,
    id: usize,
    action_log: Option<Arc<Mutex<Vec<usize>>>>,
}

impl<T> MockFn<T> {
    pub fn times(&self) -> usize {
        self.call_count.load(Relaxed)
    }
}

impl<OutType: Default> Default for MockFn<OutType> {
    fn default() -> MockFn<OutType> {
        Self {
            call_count: AtomicUsize::new(0),
            default_value: OutType::default(),
            expected_count: -1,
            is_built: false,
            is_times_set: false,
            is_will_once_set: false,
            is_will_repeatedly_set: false,
            min_count: 0,
            returns: VecDeque::new(),
            location: None,
            id: 0,
            action_log: None,
        }
    }
}

impl<OutType: Clone> Clone for MockFn<OutType> {
    fn clone(&self) -> Self {
        MockFn {
            call_count: AtomicUsize::new(self.call_count.load(Relaxed)),
            default_value: self.default_value.clone(),
            expected_count: self.expected_count,
            is_built: self.is_built,
            is_times_set: self.is_times_set,
            is_will_once_set: self.is_will_once_set,
            is_will_repeatedly_set: self.is_will_repeatedly_set,
            min_count: self.min_count,
            returns: self.returns.clone(),
            location: self.location.clone(),
            id: self.id,
            action_log: self.action_log.clone(),
        }
    }
}

impl<OutType: Default> MockFnBuilder<OutType> {
    pub fn new() -> MockFnBuilder<OutType> {
        Self(MockFn::default())
    }
}

impl<OutType> MockFnBuilder<OutType> {
    ///
    /// Create MockFn whose output types do not have automatic default values, i.e. does not
    /// implement the Default trait, such as ActionResult
    ///
    pub fn new_in_global(def_val: OutType) -> MockFnBuilder<OutType> {
        Self(MockFn {
            call_count: AtomicUsize::new(0),
            default_value: def_val,
            expected_count: -1,
            is_built: false,
            is_times_set: false,
            is_will_once_set: false,
            is_will_repeatedly_set: false,
            min_count: 0,
            returns: VecDeque::new(),
            location: None,
            id: 0,
            action_log: None,
        })
    }

    ///
    /// Set how many times exactly the call() must be invoked
    ///
    pub fn times(mut self, count: usize) -> Self {
        assert!(!self.0.is_will_repeatedly_set, "times() called after will_repeatedly()!");

        self.0.is_times_set = true;
        self.0.expected_count = count as isize;
        self
    }

    ///
    /// Ensure that the call() is invoked at least one more time and the call() returns the ret_val
    ///
    pub fn will_once(mut self, ret_val: OutType) -> Self {
        assert!(!self.0.is_will_repeatedly_set, "will_once() called after will_repeatedly()!");

        self.0.is_will_once_set = true;
        self.0.returns.push_back(ret_val);
        self.0.min_count += 1;
        self
    }

    ///
    /// Allow the call() to be invoked multiple times and the call() returns the ret_val
    /// If used, will_repeatedly() must be called the last
    ///
    pub fn will_repeatedly(mut self, ret_val: OutType) -> Self {
        assert!(!self.0.is_will_repeatedly_set, "will_repeatedly() is called more than once!");

        self.0.is_will_repeatedly_set = true;
        self.0.default_value = ret_val;
        self.0.min_count += 1;
        self
    }

    pub fn with_log(mut self, id: usize, action_log: Arc<Mutex<Vec<usize>>>) -> Self {
        self.0.id = id;
        self.0.action_log = Some(action_log);
        self
    }

    pub fn build(mut self) -> MockFn<OutType> {
        // if only will_once is set, the min_count becomes the expected_count
        if self.0.is_will_once_set && !self.0.is_will_repeatedly_set {
            self.0.expected_count = self.0.min_count as isize;
        }
        self.0.is_built = true;
        self.0
    }

    pub fn build_with_location(mut self, location: String) -> MockFn<OutType> {
        // if only will_once is set, the min_count becomes the expected_count
        if self.0.is_will_once_set && !self.0.is_will_repeatedly_set {
            self.0.expected_count = self.0.min_count as isize;
        }
        self.0.is_built = true;
        self.0.location = Some(location);
        self.0
    }
}

impl<OutType: Clone> CallableTrait<OutType> for MockFn<OutType> {
    fn call(&mut self) -> OutType {
        self.call_count.fetch_add(1, Relaxed);

        if let Some(action_log) = &self.action_log {
            let mut log = action_log.lock().unwrap();
            log.push(self.id);
        }

        if !self.returns.is_empty() {
            // return the ret_val in the order it was inserted (FIFO)
            self.returns.pop_front().unwrap()
        } else {
            // return the default_value if set or default otherwise
            self.default_value.clone()
        }
    }
}

impl<OutType> Drop for MockFn<OutType> {
    fn drop(&mut self) {
        // check the call counts only after build() is invoked, since we do not care about any
        // transient objects
        if self.is_built {
            let call_count = self.call_count.load(Relaxed);
            if self.expected_count >= 0 {
                // the times() clause or only will_once() is set, so check for exact call counts
                assert_eq!(
                    call_count as isize, self.expected_count,
                    "MockFn is called {} times, but should be {} times! Location is: {:?}",
                    call_count, self.expected_count, self.location
                );
            } else {
                // check whether the mock fn is called at least min_count due to its configuration
                assert!(
                    self.min_count <= call_count,
                    "MockFn is called {} times, but should be at least {} times! Location is: {:?}",
                    call_count,
                    self.min_count,
                    self.location
                );
            }
        }
    }
}

impl<OutType> ReusableObjectTrait for MockFn<OutType> {
    fn reusable_clear(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_times_only() {
        let mut mock = MockFnBuilder::<bool>::new().times(3).build();

        for _ in 0..3 {
            assert!(!mock.call());
        }
    }

    #[test]
    #[should_panic]
    fn test_panic_call_count_less_than_specified_times() {
        let mut mock = MockFnBuilder::<bool>::new().times(3).build();

        for _ in 0..2 {
            assert!(!mock.call());
        }
    }

    #[test]
    #[should_panic]
    fn test_panic_call_count_more_than_specified_times_should_panic() {
        let mut mock = MockFnBuilder::<bool>::new().times(3).build();

        for _ in 0..4 {
            assert!(!mock.call());
        }
    }

    #[test]
    fn test_call_count_equals_will_once_count() {
        let mut mock = MockFnBuilder::<bool>::new().will_once(true).will_once(false).build();

        assert!(mock.call());
        assert!(!mock.call());
    }

    #[test]
    #[should_panic]
    fn test_call_count_more_than_will_once_count_should_panic() {
        let mut mock = MockFnBuilder::<bool>::new().will_once(true).will_once(false).build();

        assert!(mock.call());
        assert!(!mock.call());
        mock.call();
    }

    #[test]
    #[should_panic]
    fn test_panic_call_count_less_than_will_once_count_should_panic() {
        let mut mock = MockFnBuilder::<bool>::new().will_once(true).will_once(false).build();

        assert!(mock.call());
    }

    #[test]
    fn test_with_will_repeated_only() {
        let mut mock = MockFnBuilder::<bool>::new().will_repeatedly(true).build();

        for _ in 0..3 {
            assert!(mock.call());
        }
    }

    #[test]
    fn test_with_all_clauses() {
        let mut mock = MockFnBuilder::<bool>::new()
            .times(5)
            .will_once(false)
            .will_once(true)
            .will_repeatedly(false)
            .build();

        assert!(!mock.call());
        assert!(mock.call());
        for _ in 0..3 {
            assert!(!mock.call());
        }
    }

    #[test]
    #[should_panic]
    fn test_panic_call_count_less_than_min_count_with_repeatedly_should_panic() {
        let mut mock = MockFnBuilder::<bool>::new().will_once(true).will_repeatedly(false).build();

        assert!(mock.call());
    }

    #[test]
    fn test_err_with_multiple_will_repeatedly() {
        let result = std::panic::catch_unwind(|| MockFnBuilder::<bool>::new().will_repeatedly(true).will_repeatedly(false).build());

        assert!(result.is_err());
    }

    #[test]
    fn test_err_with_will_once_after_will_repeatedly() {
        let result = std::panic::catch_unwind(|| MockFnBuilder::<bool>::new().will_repeatedly(false).will_once(true).build());

        assert!(result.is_err());
    }

    #[test]
    fn test_err_with_times_after_will_repeatedly() {
        let result = std::panic::catch_unwind(|| MockFnBuilder::<bool>::new().will_repeatedly(false).times(1).build());

        assert!(result.is_err());
    }
}
