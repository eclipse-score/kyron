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

use foundation::prelude::FoundationAtomicPtr;

use super::task::async_task::*;
use core::task::{RawWaker, RawWakerVTable, Waker};

fn clone_waker(data: *const ()) -> RawWaker {
    let task_header_ptr = data as *const TaskHeader;
    let task_ref = unsafe { TaskRef::from_raw(task_header_ptr) };

    let new_waker = task_ref.clone();

    std::mem::forget(task_ref); // We need to make sure the instance from which we clone, is forgotten since we did not consumed it, it was only bring back for a moment to clone

    let raw = TaskRef::into_raw(new_waker);
    RawWaker::new(raw as *const (), &VTABLE)
}

fn wake(data: *const ()) {
    let task_header_ptr = data as *const TaskHeader;
    let task_ref = unsafe { TaskRef::from_raw(task_header_ptr) };

    task_ref.schedule();

    drop(task_ref); // wake uses move semantic, so we are owner of data now, so we need to cleanup
}

fn wake_by_ref(data: *const ()) {
    let task_header_ptr = data as *const TaskHeader;
    let task_ref = unsafe { TaskRef::from_raw(task_header_ptr) };

    task_ref.schedule();

    std::mem::forget(task_ref); // don't touch refcount from our data since this is done by drop_waker
}

fn drop_waker(data: *const ()) {
    let task_header_ptr = data as *const TaskHeader;
    let task_ref = unsafe { TaskRef::from_raw(task_header_ptr) };

    drop(task_ref); // We get rid of instance, so ref count will be decremented
}

// Define the RawWakerVTable
static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_waker, wake, wake_by_ref, drop_waker);

///
/// Waker will store internally a pointer to the ref counted Task.
///
pub(crate) fn create_waker(ptr: TaskRef) -> Waker {
    let ptr = TaskRef::into_raw(ptr); // Extracts the pointer from TaskRef not decreasing it's reference count. Since we have a clone here, ref cnt was already increased
    let raw_waker = RawWaker::new(ptr as *const (), &VTABLE);

    // Convert RawWaker to Waker
    unsafe { Waker::from_raw(raw_waker) }
}

pub(crate) struct AtomicWakerStore {
    data: FoundationAtomicPtr<TaskHeader>,
}

unsafe impl Send for AtomicWakerStore {}
unsafe impl Sync for AtomicWakerStore {}

impl AtomicWakerStore {
    ///
    /// Exchange waker and returns previous one
    ///
    pub(crate) fn swap(&self, waker: Option<Waker>) -> Option<Waker> {
        let mut data = std::ptr::null_mut();

        if let Some(task) = waker {
            data = task.data() as *mut TaskHeader;
            std::mem::forget(task);
        }

        let old = self.data.swap(data, std::sync::atomic::Ordering::AcqRel);

        if old.is_null() {
            None
        } else {
            let raw_waker = RawWaker::new(old as *const (), &VTABLE);
            Some(unsafe { Waker::from_raw(raw_waker) })
        }
    }

    ///
    /// Sets  waker
    ///
    #[allow(dead_code)]
    pub(crate) fn set(&self, waker: Waker) {
        let _ = self.swap(Some(waker));
    }

    ///
    /// Returns current waker
    ///
    pub(crate) fn take(&self) -> Option<Waker> {
        self.swap(None)
    }
}

impl From<Waker> for AtomicWakerStore {
    fn from(value: Waker) -> Self {
        let data = value.data() as *mut TaskHeader;
        std::mem::forget(value);
        assert!(!data.is_null()); // data cannot be nullptr

        Self {
            data: FoundationAtomicPtr::new(data),
        }
    }
}

impl Default for AtomicWakerStore {
    fn default() -> Self {
        Self {
            data: FoundationAtomicPtr::new(std::ptr::null_mut()),
        }
    }
}

impl Drop for AtomicWakerStore {
    fn drop(&mut self) {
        // Free any data
        let _ = self.take();
    }
}

#[cfg(test)]
#[cfg(loom)]
mod tests {

    use super::*;
    use crate::testing::*;
    use loom::model::Builder;
    use std::sync::Arc;

    #[test]
    fn test_atomic_waker_mt() {
        let builder = Builder::new();

        builder.check(|| {
            let store = Arc::new(AtomicWakerStore::default());

            let handle = {
                let store_clone = store.clone();
                loom::thread::spawn(move || store_clone.take())
            };

            let (w, _) = get_dummy_task_waker();

            let prev = store.swap(Some(w.clone()));

            let waker_from_thread = handle.join().unwrap();

            match prev {
                Some(waker) => {
                    // If we had prev, then it shall be from current store
                    assert_eq!(waker.data(), w.data());
                }
                None => {
                    // The value must have been either here or there
                    assert!(store.take().is_some() ^ waker_from_thread.is_some());
                }
            };
        });
    }
}
