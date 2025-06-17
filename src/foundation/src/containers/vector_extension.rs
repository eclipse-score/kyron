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

use iceoryx2_bb_container::vec::Vec;

/// This module provides an extension trait for iceoryx2 `Vec<T>` to add `swap_remove` and `remove` methods.
pub trait VectorExtension<T> {
    /// Removes the element at the specified index by swapping it with the last element
    /// and then popping the last element. If the index is out of bounds, it returns `None`.
    fn swap_remove(&mut self, index: usize) -> Option<T>;

    /// Removes the element at the specified index by shifting elements to the left.
    /// If the index is out of bounds, it returns `None`.
    fn remove(&mut self, index: usize) -> Option<T>;
}

impl<T> VectorExtension<T> for Vec<T> {
    fn swap_remove(&mut self, index: usize) -> Option<T> {
        let length = self.len();

        if index >= length {
            return None;
        } else if index < length - 1 {
            self.swap(index, length - 1);
        }

        self.pop()
    }

    fn remove(&mut self, index: usize) -> Option<T> {
        let length = self.len();
        if index >= length {
            return None;
        } else if index < length - 1 {
            // SAFETY: We are manually handling the memory and ensuring that we do not
            // read or write out of bounds.
            unsafe {
                // Get a raw pointer to the vector's buffer
                let ptr = self.as_mut_ptr();

                // Read the element to be removed
                let removed = std::ptr::read(ptr.add(index));

                // Shift elements left
                std::ptr::copy(ptr.add(index + 1), ptr.add(index), length - index - 1);

                // Write the removed element to the last position to avoid dropping of shifted last element which is still valid
                std::ptr::write(ptr.add(length - 1), removed);
            }
        }

        // Remove the element at the end
        self.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iceoryx2_bb_container::vec::Vec;

    #[test]
    fn swap_remove_mid_element() {
        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(i);
        }

        // only the last element is swapped with the element at index 5
        assert_eq!(vec.swap_remove(5).unwrap(), 5);
        assert_eq!(vec[4], 4); // not affected
        assert_eq!(vec[5], 9); // swapped with 5
        assert_eq!(vec[6], 6); // not affected
        assert_eq!(vec.len(), VEC_SIZE - 1); // length is reduced by 1
    }

    #[test]
    fn swap_remove_first_element() {
        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(i);
        }
        assert_eq!(vec.swap_remove(0).unwrap(), 0);
        assert_eq!(vec[0], 9);
        assert_eq!(vec[1], 1);
        assert_eq!(vec.len(), VEC_SIZE - 1);
    }

    #[test]
    fn swap_remove_last_element() {
        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(i);
        }
        assert_eq!(vec.swap_remove(VEC_SIZE - 1).unwrap(), 9);
        assert_eq!(vec[0], 0);
        assert_eq!(vec[8], 8);
        assert_eq!(vec.len(), VEC_SIZE - 1);
    }

    #[test]
    fn swap_remove_vec_with_one_element() {
        let mut vec = Vec::new(1);
        vec.push(100);
        assert_eq!(vec.swap_remove(0).unwrap(), 100);
        assert_eq!(vec.len(), 0);
    }

    #[test]
    fn swap_remove_out_of_bounds() {
        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(i);
        }
        assert_eq!(vec.swap_remove(VEC_SIZE), None); // This should return None
    }

    #[test]
    fn remove_mid_element() {
        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(i);
        }

        // removes the element at index 5 and shifts elements to the left
        assert_eq!(vec.remove(5).unwrap(), 5);
        assert_eq!(vec[4], 4); // not affected
        assert_eq!(vec[5], 6); // shifted left
        assert_eq!(vec[8], 9);
        assert_eq!(vec.len(), VEC_SIZE - 1); // length is reduced by 1
    }

    #[test]
    fn remove_first_element() {
        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(i);
        }
        assert_eq!(vec.remove(0).unwrap(), 0);
        assert_eq!(vec[0], 1);
        assert_eq!(vec[8], 9);
        assert_eq!(vec.len(), VEC_SIZE - 1);
    }

    #[test]
    fn remove_last_element() {
        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(i);
        }
        assert_eq!(vec.remove(VEC_SIZE - 1).unwrap(), 9);
        assert_eq!(vec[0], 0);
        assert_eq!(vec[8], 8);
        assert_eq!(vec.len(), VEC_SIZE - 1);
    }

    #[test]
    fn remove_vec_with_one_element() {
        let mut vec = Vec::new(1);
        vec.push(100);
        assert_eq!(vec.remove(0).unwrap(), 100);
        assert_eq!(vec.len(), 0);
    }

    #[test]
    fn remove_out_of_bounds() {
        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(i);
        }
        assert_eq!(vec.remove(VEC_SIZE), None); // This should return None
    }

    #[test]
    fn remove_mid_element_and_drop() {
        // This is for testing the drop behavior of the removed element.
        struct TestData {
            data: usize,
        }

        impl Drop for TestData {
            fn drop(&mut self) {
                increment_counter(self.data);
            }
        }

        static mut DROPPED_DATA: usize = 0;
        static mut DROP_COUNTER: usize = 0;
        fn increment_counter(data: usize) {
            unsafe {
                DROPPED_DATA = data;
                DROP_COUNTER += 1;
            }
        }

        const VEC_SIZE: usize = 10;
        let mut vec = Vec::new(VEC_SIZE);
        for i in 0..VEC_SIZE {
            vec.push(TestData { data: i });
        }
        vec.remove(5);
        assert_eq!(unsafe { DROP_COUNTER }, 1); // Ensure that the drop was called exactly once
        assert_eq!(unsafe { DROPPED_DATA }, 5); // Ensure that the correct element was dropped
    }
}
