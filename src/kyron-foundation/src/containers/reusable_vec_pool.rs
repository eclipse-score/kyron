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

//! # ReusableVecPool
//!
//! This module provides a reusable object pool for `Vec<T>` using the `ReusableObjects` infrastructure.
//! The pool allows efficient reuse of heap-allocated vectors, reducing allocation overhead in
//! performance-critical or allocation-sensitive scenarios.
//!

use super::reusable_objects::{ReusableObject, ReusableObjects};

pub use crate::containers::Vec;
pub use crate::containers::Vector;

/// Type alias for a pool of reusable vectors.
/// Each vector is managed by the pool and can be checked out, used, and returned for reuse.
pub type ReusableVecPool<T> = ReusableObjects<Vec<T>>;

use ::core::ops::{Index, IndexMut};

/// Allows mutable indexing into the pooled vector, e.g. `obj[1] = value;`
impl<T> IndexMut<usize> for ReusableObject<Vec<T>> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        // SAFETY: This is safe in pool logic because we have exclusive access to the object.
        unsafe { self.as_inner_mut().index_mut(index) }
    }
}

/// Allows immutable indexing into the pooled vector, e.g. `let x = obj[1];`
impl<T> Index<usize> for ReusableObject<Vec<T>> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        self.as_ref().index(index)
    }
}

impl<T> ReusableObject<Vec<T>> {
    /// Pushes a value onto the end of the pooled vector.
    pub fn push(&mut self, value: T) -> bool {
        unsafe { self.as_inner_mut().push(value).is_ok() }
    }

    /// Removes the last element from the pooled vector and returns it, or `None` if empty.
    pub fn pop(&mut self) -> Option<T> {
        unsafe { self.as_inner_mut().pop() }
    }

    /// Clears all elements from the pooled vector.
    pub fn clear(&mut self) {
        unsafe {
            self.as_inner_mut().clear();
        }
    }

    /// Fills the pooled vector with clones of the given value.
    pub fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        unsafe {
            self.as_inner_mut().fill(value);
        }
    }

    /// Returns an iterator over the elements of the pooled vector.
    pub fn iter_mut(&mut self) -> ::core::slice::IterMut<'_, T> {
        unsafe { self.as_inner_mut().iter_mut() }
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use crate::prelude::vector_extension::VectorExtension;

    use super::*;

    /// Test pushing values and indexing into the pooled vector.
    #[test]
    fn test_push_and_index() {
        let mut pool = ReusableVecPool::<i32>::new(2, |_| Vec::new_in_global(8));
        let mut obj = pool.next_object().unwrap();

        obj.push(10);
        obj.push(20);
        assert_eq!(obj[0], 10);
        assert_eq!(obj[1], 20);

        obj[1] = 99;
        assert_eq!(obj[1], 99);
    }

    /// Test popping values and clearing the pooled vector.
    #[test]
    fn test_pop_and_clear() {
        let mut pool = ReusableVecPool::<i32>::new(1, |_| Vec::new_in_global(5));
        let mut obj = pool.next_object().unwrap();

        obj.push(1);
        obj.push(2);
        assert_eq!(obj.pop(), Some(2));
        assert_eq!(obj.pop(), Some(1));
        assert_eq!(obj.pop(), None);

        obj.push(5);
        obj.push(6);
        obj.clear();
        assert!(obj.pop().is_none());
    }

    /// Test filling the pooled vector.
    #[test]
    fn test_fill() {
        let mut pool = ReusableVecPool::<i32>::new(1, |_| Vec::new_in_global(10));
        let mut obj = pool.next_object().unwrap();

        obj.fill(7);
        for v in obj.iter() {
            assert_eq!(*v, 7);
        }
    }
}
