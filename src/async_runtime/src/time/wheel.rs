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

use foundation::prelude::*;
use std::ptr::NonNull;
use std::sync::atomic::Ordering;

use std::task::Waker;

pub(super) struct TimeoutData {
    pub waker: Waker,
    pub expire_at: u64,
}

pub(super) struct TimeWheel {
    slots: [TimeSlot; super::SLOTS_CNT as usize], // Slots for the wheel, each slot is a linked list of TimeEntry
    occupied_slots: FoundationAtomicU64,          // bit0 is slot 0
    level: u8,                                    // Level of the wheel, used to calculate slot range
}

pub(super) struct TimeSlot {
    head: FoundationAtomicPtr<TimeEntry>,
}

pub(super) struct TimeEntry {
    pub data: TimeoutData,
    pub next: FoundationAtomicPtr<TimeEntry>,
}

impl Default for TimeSlot {
    fn default() -> Self {
        TimeSlot {
            head: FoundationAtomicPtr::new(std::ptr::null_mut()),
        }
    }
}

pub(super) struct ExpireInfo {
    pub level: u8,
    pub slot_id: u8,
    pub deadline: u64,
}

impl TimeWheel {
    pub(super) fn new(level: u8) -> Self {
        let mut slots = std::array::from_fn(|_| TimeSlot::default());

        for slot in &mut slots {
            slot.head.store(std::ptr::null_mut(), Ordering::Relaxed);
        }

        TimeWheel {
            slots,
            level,
            occupied_slots: FoundationAtomicU64::new(0),
        }
    }

    ///
    /// Returns the next slot time that expires (this is likely less than the real deadline in this slot in specific timer)
    ///
    pub(super) fn next_expiration(&self, now: u64) -> Option<ExpireInfo> {
        let next_occupied_slot = self.next_occupied_slot(now)?;

        let wheel_range = self.level_range(); // how much time this wheel covers
        let slot_range = self.slot_range(); // how much time each slot covers

        // Compute in which time in comparison to now this wheel started (so its past)
        let wheel_start = now & !(wheel_range - 1);

        // Compute when occupied slot will expire
        let slot_deadline = wheel_start + next_occupied_slot as u64 * slot_range;

        assert!(
            slot_deadline >= now,
            "Slot deadline should be in the future now {} deadline {} on level {}, wheel_start {}",
            now,
            slot_deadline,
            self.level,
            wheel_start
        );

        Some(ExpireInfo {
            level: self.level,
            slot_id: next_occupied_slot,
            deadline: slot_deadline,
        })
    }

    pub(super) fn next_occupied_slot(&self, now: u64) -> Option<u8> {
        let occupied = self.occupied_slots.load(Ordering::Relaxed);

        if occupied == 0 {
            return None;
        }

        let curr_slot = now / self.slot_range();
        let x = occupied.rotate_right(curr_slot as u32); // Move a slot wheel into now
        let next = (x.trailing_zeros() + curr_slot as u32) % super::SLOTS_CNT as u32; // Current wheel position + zeros that are there indicating number of free slots till first free one
        Some(next as u8)
    }

    pub(super) fn level(&self) -> u8 {
        self.level
    }

    pub(super) fn aquire_slot(&mut self, info: &ExpireInfo) -> TimeSlotIterator {
        assert!(info.level == self.level, "Trying to acquire slot from a different level");

        self.occupied_slots.fetch_and(!(1 << info.slot_id), Ordering::Relaxed);

        let entries = self.slots[info.slot_id as usize].head.swap(std::ptr::null_mut(), Ordering::Relaxed);

        TimeSlotIterator { head: entries }
    }

    pub(super) fn register_wakeup_on_timeout(&self, expire_at: u64, mut data: NonNull<TimeEntry>) {
        let slot_id = ((expire_at / self.slot_range()) as usize) % 64;

        self.occupied_slots.fetch_or(1 << slot_id, Ordering::Relaxed);

        loop {
            let curr = self.slots[slot_id].head.load(Ordering::Relaxed);

            unsafe { data.as_mut().next.store(curr, Ordering::Relaxed) }; // pin head into our next

            match self.slots[slot_id]
                .head
                .compare_exchange(curr, data.as_ptr(), Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    break; // Head points to new item, we can exit the loop
                }
                Err(_) => {
                    // If the head was changed, we retry
                    continue;
                }
            }
        }
    }

    fn slot_range(&self) -> u64 {
        super::SLOTS_CNT.pow(self.level as u32)
    }

    fn level_range(&self) -> u64 {
        self.slot_range() * super::SLOTS_CNT
    }
}

pub(super) struct TimeSlotIterator {
    head: *mut TimeEntry,
}

impl Iterator for TimeSlotIterator {
    type Item = NonNull<TimeEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.head.is_null() {
            return None;
        }

        let current = self.head;
        self.head = (unsafe { current.as_ref().unwrap() }).next.load(Ordering::Relaxed);

        Some(unsafe { NonNull::new_unchecked(current) })
    }
}
