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

/// Internal structure to hold the mock function information
struct MockFnInfo<InType, OutType> {
    id: usize,
    expected_count: Option<usize>,
    min_count: usize,
    once: VecDeque<Box<dyn FnMut(InType) -> OutType + Send>>,
    default: Box<dyn FnMut(InType) -> OutType + Send>,
    // Arc Mutex to allow shared ownership and mutability,
    seq_inner: Option<Arc<Mutex<SeqInner>>>,
    location: Option<String>,
}

impl<InType, OutType> MockFnInfo<InType, OutType> {
    fn new(f: impl FnMut(InType) -> OutType + Send + 'static) -> Self {
        Self {
            id: 0,
            expected_count: None,
            min_count: 0,
            once: VecDeque::new(),
            default: Box::new(f),
            seq_inner: None,
            location: None,
        }
    }
}

///
/// A builder for creating MockFn objects
///
pub struct MockFnBuilder<InType, OutType> {
    // Arc is used for easier cloning and to avoid complex FnMut closure cloning
    info: Arc<Mutex<MockFnInfo<InType, OutType>>>,
    is_will_repeatedly_set: bool,
}

///
/// A mock object that can be used to monitor the invocation count of its method, i.e. call() and invocation order.
/// Each invocation returns predefined values configured via will_once() or will_repeatedly().
/// If the invocation count does not match the expected count or the invocation order is incorrect, a panic occurs when the object is dropped.
///
pub struct MockFn<InType, OutType> {
    info: Arc<Mutex<MockFnInfo<InType, OutType>>>,
    call_count: AtomicUsize,
}

impl<InType, OutType: Default> Default for MockFnBuilder<InType, OutType> {
    fn default() -> Self {
        Self::new()
    }
}

impl<InType, OutType> Clone for MockFnBuilder<InType, OutType> {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            is_will_repeatedly_set: self.is_will_repeatedly_set,
        }
    }
}

impl<InType, OutType: Default> Default for MockFn<InType, OutType> {
    fn default() -> Self {
        MockFnBuilder::default().build()
    }
}

impl<InType, OutType> Clone for MockFn<InType, OutType> {
    fn clone(&self) -> Self {
        Self {
            info: self.info.clone(),
            call_count: AtomicUsize::new(self.call_count.load(Relaxed)),
        }
    }
}

impl<InType, OutType> MockFn<InType, OutType> {
    ///
    /// Get the number of times the mock function has been called
    ///
    pub fn times(&self) -> usize {
        self.call_count.load(Relaxed)
    }

    ///
    /// Call the mock function with the given input and return the output
    ///
    pub fn call(&mut self, input: InType) -> OutType {
        // Increment call count
        self.call_count.fetch_add(1, Relaxed);

        let mut info = self.info.lock().unwrap();
        let id = info.id;
        // Store execution order
        if let Some(seq) = &mut info.seq_inner {
            seq.lock().unwrap().execution_order.push(id);
        }

        // Return value for once calls
        if let Some(mut once) = info.once.pop_front() {
            return once(input);
        }

        // Return default value or repeated value
        (info.default)(input)
    }
}

impl<InType, OutType> MockFnBuilder<InType, OutType> {
    ///
    /// Create MockFn whose output types have automatic default values, i.e. implements the Default trait
    ///
    pub fn new() -> Self
    where
        OutType: Default,
    {
        Self {
            info: Arc::new(Mutex::new(MockFnInfo::new(|_| OutType::default()))),
            is_will_repeatedly_set: false,
        }
    }

    ///
    /// Create MockFn whose output types do not have automatic default values, i.e. does not
    /// implement the Default trait, such as ActionResult
    ///
    pub fn new_in_global(f: impl FnMut(InType) -> OutType + Send + 'static) -> Self {
        Self {
            info: Arc::new(Mutex::new(MockFnInfo::new(f))),
            is_will_repeatedly_set: false,
        }
    }

    ///
    /// Set how many times exactly the call() must be invoked
    ///
    pub fn times(&mut self, count: usize) -> &mut Self {
        assert!(!self.is_will_repeatedly_set, "times() called after will_repeatedly()!");
        self.info.lock().unwrap().expected_count = Some(count);
        self
    }

    ///
    /// Ensure that the call() is invoked at least one more time and the call() returns constant value, ignoring input.
    ///
    pub fn will_once_return(&mut self, value: OutType) -> &mut Self
    where
        OutType: Clone + Send + 'static,
    {
        self.will_once(move |_| value.clone())
    }

    ///
    /// Ensure that the call() is invoked at least one more time and the call() returns callback f's return value.
    ///
    pub fn will_once_invoke<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(InType) -> OutType + Send + 'static,
    {
        self.will_once(f)
    }

    fn will_once<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(InType) -> OutType + Send + 'static,
    {
        assert!(!self.is_will_repeatedly_set, "will_once() called after will_repeatedly()!");
        {
            let mut info = self.info.lock().unwrap();
            info.once.push_back(Box::new(f));
            info.min_count += 1;
        }
        self
    }

    ///
    /// Allow the call() to be invoked multiple times and the call() returns constant value, ignoring input.
    /// If used, will_repeatedly_return() must be called the last.
    ///
    pub fn will_repeatedly_return(&mut self, value: OutType) -> &mut Self
    where
        OutType: Clone + Send + 'static,
    {
        self.will_repeatedly(move |_| value.clone())
    }

    ///
    /// Allow the call() to be invoked multiple times and the call() returns the callback f's return value.
    /// If used, will_repeatedly_invoke() must be called the last.
    ///
    pub fn will_repeatedly_invoke<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(InType) -> OutType + Send + 'static,
    {
        self.will_repeatedly(f)
    }

    fn will_repeatedly<F>(&mut self, f: F) -> &mut Self
    where
        F: FnMut(InType) -> OutType + Send + 'static,
    {
        assert!(!self.is_will_repeatedly_set, "will_repeatedly() called more than once!");
        self.is_will_repeatedly_set = true;
        {
            let mut info = self.info.lock().unwrap();
            info.default = Box::new(f);
            info.min_count += 1;
        }
        self
    }

    ///
    /// Register the MockFn in a sequence to verify the execution order.
    /// The execution order is same as registration order. If the execution order is incorrect, a panic occurs.
    ///
    pub fn in_sequence(&mut self, seq: &Sequence) -> &mut Self {
        {
            let mut info = self.info.lock().unwrap();
            info.id = seq.register();
            info.seq_inner = Some(seq.seq_inner.clone());
        }
        self
    }

    ///
    /// Build the MockFn object
    ///
    pub fn build(&mut self) -> MockFn<InType, OutType> {
        // if only will_once is set, the min_count becomes the expected_count
        let mut info = self.info.lock().unwrap();
        if !info.once.is_empty() && !self.is_will_repeatedly_set {
            info.expected_count = Some(info.min_count);
        }
        MockFn {
            info: self.info.clone(),
            call_count: AtomicUsize::new(0),
        }
    }

    ///
    /// Build the MockFn object with location information
    ///
    pub fn build_with_location(&mut self, location: String) -> MockFn<InType, OutType> {
        // if only will_once is set, the min_count becomes the expected_count
        let mut info = self.info.lock().unwrap();
        if !info.once.is_empty() && !self.is_will_repeatedly_set {
            info.expected_count = Some(info.min_count);
        }
        info.location = Some(location);
        MockFn {
            info: self.info.clone(),
            call_count: AtomicUsize::new(0),
        }
    }
}

impl<InType, OutType> Drop for MockFn<InType, OutType> {
    fn drop(&mut self) {
        let call_count = self.call_count.load(Relaxed);
        let info = self.info.lock().unwrap();
        if let Some(expected) = info.expected_count {
            assert_eq!(
                call_count, expected,
                "MockFn is called {} times, but should be {} times! Location is: {:?}",
                call_count, expected, info.location
            );
        } else {
            assert!(
                call_count >= info.min_count,
                "MockFn is called {} times, but should be at least {} times! Location is: {:?}",
                call_count,
                info.min_count,
                info.location
            );
        }
    }
}

impl<InType, OutType> ReusableObjectTrait for MockFn<InType, OutType> {
    fn reusable_clear(&mut self) {}
}

struct SeqInner {
    registration_id_next: usize,
    execution_order: Vec<usize>,
}

///
/// A sequence to verify the execution order of mock functions.
/// Each mock function registered in the sequence gets a unique registration ID.
/// The execution order is same as registration order. If the execution order is incorrect, a panic occurs.
///
pub struct Sequence {
    seq_inner: Arc<Mutex<SeqInner>>,
}

impl Default for Sequence {
    fn default() -> Self {
        Self::new()
    }
}

impl Sequence {
    ///
    /// Create a new Sequence
    ///
    pub fn new() -> Self {
        Self {
            seq_inner: Arc::new(Mutex::new(SeqInner {
                registration_id_next: 0,
                execution_order: Vec::new(),
            })),
        }
    }

    fn register(&self) -> usize {
        let mut seq_inner = self.seq_inner.lock().unwrap();
        let id = seq_inner.registration_id_next;
        seq_inner.registration_id_next += 1;
        id
    }

    fn verify_executed_order(&self) {
        let seq_inner = self.seq_inner.lock().unwrap();
        for (expected_id, &executed_id) in seq_inner.execution_order.iter().enumerate() {
            assert_eq!(
                expected_id, executed_id,
                "MockFn with registration ID {} was expected to be called, but MockFn with registration ID {} was called instead!",
                expected_id, executed_id
            );
        }
    }

    ///
    /// Verify that the sequence was executed in order and prepare for the next iteration.
    /// This is useful when the sequence is used in a loop.
    ///
    pub fn verify_executed_order_and_prepare_for_next_iteration(&self) {
        self.verify_executed_order();
        self.seq_inner.lock().unwrap().execution_order.clear();
    }
}

impl Drop for Sequence {
    fn drop(&mut self) {
        self.verify_executed_order();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_times_only() {
        let mut mock = MockFnBuilder::<(), bool>::new().times(3).build();

        for _ in 0..3 {
            assert!(!mock.call(()));
        }
    }

    #[test]
    #[should_panic]
    fn test_panic_call_count_less_than_specified_times() {
        let mut mock = MockFnBuilder::<(), bool>::new().times(3).build();

        for _ in 0..2 {
            assert!(!mock.call(()));
        }
    }

    #[test]
    #[should_panic]
    fn test_panic_call_count_more_than_specified_times_should_panic() {
        let mut mock = MockFnBuilder::<(), bool>::new().times(3).build();

        for _ in 0..4 {
            assert!(!mock.call(()));
        }
    }

    #[test]
    fn test_call_count_equals_will_once_count() {
        let mut mock = MockFnBuilder::<(), bool>::new().will_once_return(true).will_once_return(false).build();

        assert!(mock.call(()));
        assert!(!mock.call(()));
    }

    #[test]
    #[should_panic]
    fn test_call_count_more_than_will_once_count_should_panic() {
        let mut mock = MockFnBuilder::<(), bool>::new().will_once_return(true).will_once_return(false).build();

        assert!(mock.call(()));
        assert!(!mock.call(()));
        mock.call(());
    }

    #[test]
    #[should_panic]
    fn test_panic_call_count_less_than_will_once_count_should_panic() {
        let mut mock = MockFnBuilder::<(), bool>::new().will_once_return(true).will_once_return(false).build();

        assert!(mock.call(()));
    }

    #[test]
    fn test_with_will_repeated_only() {
        let mut mock = MockFnBuilder::<(), bool>::new().will_repeatedly_return(true).build();

        for _ in 0..3 {
            assert!(mock.call(()));
        }
    }

    #[test]
    fn test_with_all_clauses() {
        let mut mock = MockFnBuilder::<(), bool>::new()
            .times(5)
            .will_once_return(false)
            .will_once_return(true)
            .will_repeatedly_return(false)
            .build();

        assert!(!mock.call(()));
        assert!(mock.call(()));
        for _ in 0..3 {
            assert!(!mock.call(()));
        }
    }

    #[test]
    #[should_panic]
    fn test_panic_call_count_less_than_min_count_with_repeatedly_should_panic() {
        let mut mock = MockFnBuilder::<(), bool>::new()
            .will_once_return(true)
            .will_repeatedly_return(false)
            .build();

        assert!(mock.call(()));
    }

    #[test]
    fn test_err_with_multiple_will_repeatedly() {
        let result = std::panic::catch_unwind(|| {
            MockFnBuilder::<(), bool>::new()
                .will_repeatedly_return(true)
                .will_repeatedly_return(false)
                .build()
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_err_with_will_once_after_will_repeatedly() {
        let result = std::panic::catch_unwind(|| {
            MockFnBuilder::<(), bool>::new()
                .will_repeatedly_return(false)
                .will_once_return(true)
                .build()
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_err_with_times_after_will_repeatedly() {
        let result = std::panic::catch_unwind(|| MockFnBuilder::<(), bool>::new().will_repeatedly_return(false).times(1).build());

        assert!(result.is_err());
    }

    #[test]
    fn test_sequence_execution_order_ok() {
        let seq = Sequence::new();

        let mut mock1 = MockFnBuilder::<(), bool>::new().in_sequence(&seq).will_once_return(true).build();
        let mut mock2 = MockFnBuilder::<(), bool>::new().in_sequence(&seq).will_once_return(false).build();

        assert!(mock1.call(()));
        assert!(!mock2.call(()));
    }

    #[test]
    #[should_panic]
    fn test_sequence_execution_order_err() {
        let seq = Sequence::new();

        let mut mock1 = MockFnBuilder::<(), bool>::new().in_sequence(&seq).will_once_return(true).build();
        let mut mock2 = MockFnBuilder::<(), bool>::new().in_sequence(&seq).will_once_return(false).build();

        assert!(!mock2.call(()));
        assert!(mock1.call(()));
    }

    #[test]
    fn test_sequence_execution_order_in_loop_ok() {
        let seq = Sequence::new();

        let mut mock1 = MockFnBuilder::<(), bool>::new().in_sequence(&seq).times(3).build();
        let mut mock2 = MockFnBuilder::<(), bool>::new().in_sequence(&seq).times(3).build();

        for _ in 0..3 {
            assert!(!mock1.call(()));
            assert!(!mock2.call(()));
            seq.verify_executed_order_and_prepare_for_next_iteration();
        }
    }

    #[test]
    fn test_mockfn_intype_outtype() {
        let mut mock1 = MockFnBuilder::<i32, i32>::new().will_once_invoke(|x| x + 1).build();
        let mut mock2 = MockFnBuilder::<String, usize>::new().will_once_invoke(|x| x.len()).build();
        let mut mock3 = MockFnBuilder::<f64, f64>::new().will_repeatedly_invoke(|x| x * 2.0).build();

        assert_eq!(mock1.call(1), 2);
        assert_eq!(mock2.call("Hello".into()), 5);
        for _ in 0..3 {
            assert_eq!(mock3.call(3.0), 6.0);
        }
    }
}
