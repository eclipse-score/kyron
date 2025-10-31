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

use crate::cell::UnsafeCell;
use core::marker::PhantomPinned;
use core::ptr;

/// Every [`List`] item needs to have this as one of its fields.
pub struct Link {
    prev: UnsafeCell<*const Link>,
    next: UnsafeCell<*const Link>,
    _pin: PhantomPinned,
}

impl Link {
    /// Create an unlinked link.
    pub fn new() -> Self {
        Self {
            prev: UnsafeCell::new(ptr::null_mut()),
            next: UnsafeCell::new(ptr::null_mut()),
            _pin: PhantomPinned,
        }
    }

    unsafe fn prev(&self) -> *const Link {
        unsafe { *self.prev.get().deref() }
    }

    unsafe fn next(&self) -> *const Link {
        unsafe { *self.next.get().deref() }
    }

    unsafe fn set_prev(&self, value: *const Link) {
        self.prev.with_mut(|next| *next = value);
    }

    unsafe fn set_next(&self, value: *const Link) {
        self.next.with_mut(|next| *next = value);
    }
}

unsafe impl Send for Link {}

impl Default for Link {
    fn default() -> Self {
        Self::new()
    }
}

unsafe fn to_nonnull(ptr: *const Link) -> ptr::NonNull<Link> {
    debug_assert_ne!(ptr, ptr::null());
    ptr::NonNull::new_unchecked(ptr as *mut Link)
}

/// Enables a type containing a [`Link`] to be inserted into a [`List`].
///
/// # Safety
///
/// The implementation of [`link_field_offset()`](Item::link_field_offset) must return
/// the real offset of the [`Link`].
pub unsafe trait Item {
    /// Returns the offset of the [`Link`] field on the implementing object in bytes.
    ///
    /// This method should be implemented using [`offset_of!`](core::mem::offset_of),
    /// like this:
    ///
    /// ```
    /// # use kyron_foundation::containers::intrusive_linked_list::{Item, Link};
    /// # struct MyStruct { link: Link }
    /// unsafe impl Item for MyStruct {
    ///     fn link_field_offset() -> usize {
    ///         core::mem::offset_of!(Self, link)
    ///     }
    /// }
    /// ```
    fn link_field_offset() -> usize;
}

/// An intrusive linked list.
///
/// The list doesn't manage memory. It doesn't do anything on drop.
/// The [`Link`]s used by the list need to be stored in a field on the inserted objects.
/// It is the user's responsibility to ensure that the objects outlive the list.
pub struct List<I: Item> {
    // Can't have a sentinel node because the list can't manage memory.
    head: ptr::NonNull<Link>,
    len: usize,
    _phantom_data: core::marker::PhantomData<I>,
}

impl<I: Item> List<I> {
    /// Inserts an item at the back of the list.
    ///
    /// # Safety
    ///
    /// - The item must not be an element in *any* list, including this list.
    /// - The inserted item must outlive its presence in this list.
    ///   Specifically, if the item is never removed, it must outlive the lifetime of this list.
    pub unsafe fn push_back(&mut self, item: &I) {
        unsafe {
            let link = Self::item_to_link(item);

            if self.len == 0 {
                // If this is the first item being inserted, set it up as a sentinel.
                debug_assert_eq!(link.as_ref().prev(), ptr::null_mut());
                debug_assert_eq!(link.as_ref().next(), ptr::null_mut());
                link.as_ref().set_prev(link.as_ptr());
                link.as_ref().set_next(link.as_ptr());
                self.head = link;
            } else {
                Self::insert_link_before(link, self.head);
            }
        }

        self.len += 1;
    }

    /// Removes the item from the front of the list.
    ///
    /// Returns the item if the list wasn't empty.
    pub fn pop_front(&mut self) -> Option<ptr::NonNull<I>> {
        if self.len > 0 {
            self.len -= 1;

            unsafe {
                let old_head = self.head;
                // Since the head is set up as a sentinel, thus points to itself,
                // these operations are safe, even if the head is the last element being removed.
                self.head = to_nonnull(old_head.as_ref().next());
                Self::remove_link(old_head.as_ref());

                Some(Self::link_to_item(old_head))
            }
        } else {
            None
        }
    }

    /// Removes all items that match the predicate `f` by returning `true`.
    pub fn remove_if<F>(&mut self, mut f: F)
    where
        F: FnMut(&I) -> bool,
    {
        if self.len == 0 {
            return;
        }

        let mut curr = self.head.as_ptr() as *const Link;
        let mut num_of_iteration = self.len;

        unsafe {
            while num_of_iteration > 0 {
                let current_link = to_nonnull(curr);
                let next = current_link.as_ref().next();
                curr = next;

                if f(Self::link_to_item(current_link).as_ref()) {
                    self.remove_internal(current_link.as_ref());
                }
                num_of_iteration -= 1; // whether removed or not, we just processed this elem
            }
        }
    }

    /// Removes a specific item from the list.
    ///
    /// Returns `true` if the item was found and removed, `false` otherwise.
    pub fn remove(&mut self, item: &I) -> bool {
        match self.len {
            0 => false, // Nothing to remove.
            1 => {
                let link = unsafe { Self::item_to_link(item) };

                if link == self.head {
                    // If this is the last item, we can just reset the list.
                    self.head = ptr::NonNull::dangling();
                    self.len = 0;
                    true
                } else {
                    false
                }
            }
            _ => {
                // If this is not the last item, we need to remove it from the list.
                let mut curr = self.head.as_ptr() as *const Link;

                unsafe {
                    while !curr.is_null() {
                        let link = to_nonnull(curr);

                        if ptr::NonNull::from(item) == Self::link_to_item(link) {
                            self.remove_internal(link.as_ref());
                            return true;
                        }

                        curr = (*curr).next();
                    }
                }

                false
            }
        }
    }

    /// The number of linked items.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the list is empty, `false` otherwise.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    unsafe fn remove_internal(&mut self, link: &Link) {
        if ptr::NonNull::from(link) == self.head {
            if self.len == 1 {
                // If this is the last item, we can just reset the list.
                self.head = ptr::NonNull::dangling();
            } else {
                self.head = unsafe { to_nonnull(link.next()) };
                Self::remove_link(link);
            }
        } else {
            Self::remove_link(link);
        }

        self.len -= 1;
    }

    unsafe fn link_to_item(link: ptr::NonNull<Link>) -> ptr::NonNull<I> {
        link.byte_sub(I::link_field_offset()).cast::<I>()
    }

    unsafe fn item_to_link(item: &I) -> ptr::NonNull<Link> {
        ptr::NonNull::from(item).byte_add(I::link_field_offset()).cast::<Link>()
    }

    unsafe fn insert_link_before(link_ptr: ptr::NonNull<Link>, before: ptr::NonNull<Link>) {
        let link = link_ptr.as_ref();
        debug_assert_eq!(link.prev(), ptr::null_mut());
        debug_assert_eq!(link.next(), ptr::null_mut());

        let left = before.as_ref().prev();
        link.set_prev(left);
        (*left).set_next(link_ptr.as_ptr());

        let right = before.as_ptr();
        link.set_next(right);
        (*right).set_prev(link_ptr.as_ptr());
    }

    unsafe fn remove_link(link: &Link) {
        let left = link.prev();
        let right = link.next();
        (*left).set_next(right);
        (*right).set_prev(left);

        link.set_prev(ptr::null_mut());
        link.set_next(ptr::null_mut());
    }
}

impl<I: Item> Default for List<I> {
    fn default() -> Self {
        Self {
            head: ptr::NonNull::dangling(),
            len: 0,
            _phantom_data: Default::default(),
        }
    }
}

unsafe impl<I: Item + Send> Send for List<I> {}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use core::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn test_push_pop() {
        // Using repr(C) to force the offset_of_link_in_item to not be 0.
        #[repr(C)]
        struct TestItem {
            value1: u64,
            link: Link,
            value2: f32,
        }

        impl Drop for TestItem {
            fn drop(&mut self) {
                println!(
                    "drop value1 {} value2 {} link.prev {:?} link.next {:?}",
                    self.value1, self.value2, self.link.prev, self.link.next,
                );
            }
        }

        unsafe impl Item for TestItem {
            fn link_field_offset() -> usize {
                std::mem::offset_of!(Self, link)
            }
        }

        let item1 = TestItem {
            value1: 1,
            link: Link::new(),
            value2: 2.0,
        };

        let item2 = TestItem {
            value1: 5,
            link: Link::new(),
            value2: 6.0,
        };

        let mut list: List<TestItem> = Default::default();

        assert_eq!(list.len(), 0);
        assert!(list.is_empty());
        unsafe {
            list.push_back(&item1);
        }
        assert_eq!(list.len(), 1);
        assert!(!list.is_empty());
        unsafe {
            list.push_back(&item2);
        }
        assert_eq!(list.len(), 2);
        assert!(!list.is_empty());

        let item1 = list.pop_front().expect("Failed to pop item1");
        assert_eq!(list.len(), 1);
        let item2 = list.pop_front().expect("Failed to pop item2");
        assert_eq!(list.len(), 0);

        unsafe {
            assert_eq!(item1.as_ref().value1, 1);
            assert_eq!(item1.as_ref().value2, 2.0);
            assert_eq!(item2.as_ref().value1, 5);
            assert_eq!(item2.as_ref().value2, 6.0);
        }

        assert!(list.pop_front().is_none());
    }

    #[test]
    fn test_drop_list() {
        let drop_counter = Rc::new(RefCell::new(0_u32));

        struct TestItem {
            link: Link,
            drop_counter: Rc<RefCell<u32>>,
        }

        unsafe impl Item for TestItem {
            fn link_field_offset() -> usize {
                std::mem::offset_of!(Self, link)
            }
        }

        impl Drop for TestItem {
            fn drop(&mut self) {
                *self.drop_counter.borrow_mut() += 1_u32;
            }
        }

        {
            let item1 = TestItem {
                drop_counter: Rc::clone(&drop_counter),
                link: Link::new(),
            };

            let item2 = TestItem {
                drop_counter: Rc::clone(&drop_counter),
                link: Link::new(),
            };

            let item3 = TestItem {
                drop_counter: Rc::clone(&drop_counter),
                link: Link::new(),
            };

            let mut list: List<TestItem> = Default::default();
            unsafe {
                list.push_back(&item1);
                list.push_back(&item2);
                list.push_back(&item3);
            }
            assert_eq!(list.len(), 3);
            let _ = list.pop_front();
            assert_eq!(list.len(), 2);
            assert_eq!(*drop_counter.borrow(), 0);
        }

        assert_eq!(*drop_counter.borrow(), 3);
    }

    #[test]
    fn test_remove_if_removes_matching_items() {
        #[repr(C)]
        struct TestItem {
            value: u32,
            link: Link,
        }
        unsafe impl Item for TestItem {
            fn link_field_offset() -> usize {
                std::mem::offset_of!(Self, link)
            }
        }

        {
            let item1 = TestItem { value: 1, link: Link::new() };
            let item2 = TestItem { value: 2, link: Link::new() };
            let item3 = TestItem { value: 3, link: Link::new() };
            let mut list: List<TestItem> = Default::default();
            unsafe {
                list.push_back(&item1);
                list.push_back(&item2);
                list.push_back(&item3);
            }
            // Remove all even values
            list.remove_if(|item| item.value % 2 == 0);

            assert_eq!(list.len(), 2);
            let v1 = unsafe { list.pop_front().unwrap().as_ref().value };
            let v2 = unsafe { list.pop_front().unwrap().as_ref().value };
            assert_eq!(vec![v1, v2], vec![1, 3]);
        }

        {
            let item1 = TestItem { value: 2, link: Link::new() };
            let item2 = TestItem { value: 2, link: Link::new() };
            let item3 = TestItem { value: 4, link: Link::new() };
            let item4 = TestItem {
                value: 16,
                link: Link::new(),
            };
            let mut list: List<TestItem> = Default::default();
            unsafe {
                list.push_back(&item1);
                list.push_back(&item2);
                list.push_back(&item3);
                list.push_back(&item4);
            }

            // Remove all even values
            list.remove_if(|item| item.value % 2 == 0);

            assert_eq!(list.len(), 0);
        }
    }

    #[test]
    fn test_remove_specific_item() {
        #[repr(C)]
        struct TestItem {
            value: u32,
            link: Link,
        }
        unsafe impl Item for TestItem {
            fn link_field_offset() -> usize {
                std::mem::offset_of!(Self, link)
            }
        }
        let item1 = TestItem {
            value: 10,
            link: Link::new(),
        };
        let item2 = TestItem {
            value: 20,
            link: Link::new(),
        };
        let item3 = TestItem {
            value: 30,
            link: Link::new(),
        };
        let mut list: List<TestItem> = Default::default();
        unsafe {
            list.push_back(&item1);
            list.push_back(&item2);
            list.push_back(&item3);
        }
        // Remove middle item
        assert!(list.remove(&item2));
        assert_eq!(list.len(), 2);
        let vals = [unsafe { list.pop_front().unwrap().as_ref().value }, unsafe {
            list.pop_front().unwrap().as_ref().value
        }];
        assert!(vals.contains(&10));
        assert!(vals.contains(&30));
        // Remove non-existent item
        let item4 = TestItem {
            value: 40,
            link: Link::new(),
        };
        assert!(!list.remove(&item4));
    }
}
