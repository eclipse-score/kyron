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

#[cfg(loom)]
pub use loom::cell::{ConstPtr, MutPtr, UnsafeCell};

#[cfg(not(loom))]
#[derive(Debug)]
pub struct UnsafeCell<T: ?Sized>(::core::cell::UnsafeCell<T>);

#[cfg(not(loom))]
impl<T> UnsafeCell<T> {
    pub fn new(data: T) -> UnsafeCell<T> {
        UnsafeCell(::core::cell::UnsafeCell::new(data))
    }

    pub fn with<R>(&self, f: impl FnOnce(*const T) -> R) -> R {
        f(self.0.get())
    }

    pub fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }

    pub fn get(&self) -> ConstPtr<T> {
        ConstPtr { ptr: self.0.get() }
    }

    pub fn get_mut(&self) -> MutPtr<T> {
        MutPtr { ptr: self.0.get() }
    }
}

/// This wrapper mirrors `loom::cell::ConstPtr`.
#[cfg(not(loom))]
pub struct ConstPtr<T: ?Sized> {
    ptr: *const T,
}

#[cfg(not(loom))]
impl<T: ?Sized> ConstPtr<T> {
    /// Immutably dereferences the pointer.
    ///
    /// # Safety
    ///
    /// This is equivalent to dereferencing a `*const T` pointer, so all the same
    /// safety considerations apply here.
    pub unsafe fn deref(&self) -> &T {
        &*self.ptr
    }
}

/// This wrapper mirrors `loom::cell::MutPtr`.
#[cfg(not(loom))]
pub struct MutPtr<T: ?Sized> {
    ptr: *mut T,
}

#[cfg(not(loom))]
impl<T: ?Sized> MutPtr<T> {
    /// Mutably dereferences the pointer.
    ///
    /// # Safety
    ///
    /// This is equivalent to dereferencing a `*mut T` pointer, so all the same
    /// safety considerations apply here.
    #[expect(clippy::mut_from_ref, reason = "UnsafeCell is allowed to create &mut from &")]
    pub unsafe fn deref(&self) -> &mut T {
        &mut *self.ptr
    }
}

pub trait UnsafeCellExt<T: ?Sized> {
    /// Returns a shared reference to the value within the `UnsafeCell`.
    ///
    /// **Note:** This method has no equivalent when using the `loom` version;
    /// use [`Self::get()`] instead if possible.
    ///
    /// # Safety
    ///
    /// - It is *Undefined Behavior* to call this while any mutable reference to the wrapped value
    ///   is alive.
    /// - Mutating the wrapped value while the returned reference is alive is *Undefined Behavior*.
    unsafe fn as_ref_unchecked(&self) -> &T;

    /// Returns an exclusive reference to the value within the `UnsafeCell`.
    ///
    /// **Note:** This method has no equivalent when using the `loom` version;
    /// use [`Self::get_mut()`] instead if possible.
    ///
    /// # Safety
    ///
    /// - It is *Undefined Behavior* to call this while any other reference(s) to the wrapped value
    ///   are alive.
    /// - Mutating the wrapped value through other means while the returned reference is alive
    ///   is *Undefined Behavior*.
    #[expect(clippy::mut_from_ref, reason = "UnsafeCell is allowed to create &mut from &")]
    unsafe fn as_mut_unchecked(&self) -> &mut T;
}

#[cfg(not(loom))]
impl<T: ?Sized> UnsafeCellExt<T> for UnsafeCell<T> {
    /// **Note:** This method has no equivalent when using the `loom` version;
    /// use [`UnsafeCell::get()`] instead if possible.
    unsafe fn as_ref_unchecked(&self) -> &T {
        &*self.0.get()
    }

    /// **Note:** This method has no equivalent when using the `loom` version;
    /// use [`UnsafeCell::get_mut()`] instead if possible.
    unsafe fn as_mut_unchecked(&self) -> &mut T {
        &mut *self.0.get()
    }
}

#[cfg(loom)]
impl<T: ?Sized> UnsafeCellExt<T> for UnsafeCell<T> {
    /// **Note:** This method has no equivalent when using the `loom` version;
    /// use [`UnsafeCell::get()`] instead if possible.
    unsafe fn as_ref_unchecked(&self) -> &T {
        panic!("UnsafeCellExt::as_ref_unchecked() is unavailable for loom::cel::UnsafeCell");
    }

    /// **Note:** This method has no equivalent when using the `loom` version;
    /// use [`UnsafeCell::get_mut()`] instead if possible.
    unsafe fn as_mut_unchecked(&self) -> &mut T {
        panic!("UnsafeCellExt::as_mut_unchecked() is unavailable for loom::cel::UnsafeCell");
    }
}
