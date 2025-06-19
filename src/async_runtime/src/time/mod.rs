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

use std::sync::RwLock;
use std::time::Duration;
use std::{alloc::Layout, ptr::NonNull, task::Waker};

use foundation::create_arr_storage;
use foundation::prelude::{AllocationError, FixedSizePoolAllocator};
use foundation::prelude::{BaseAllocator, CommonErrors};

use crate::time::clock::*;
use crate::time::wheel::ExpireInfo;
use crate::time::wheel::TimeEntry;

pub mod clock;
pub mod wheel;

//
// This is a time driver that uses a hierarchical time wheel to manage timeouts. It runs with milliseconds resolution,
// and depends on being polled by external code to process a time. It does not do any work on its own.
//
// We split TimeWheel into given hierarchy of levels, each level has its own slots.
// Level 1 - 64 slots, each slot is 1ms
// Level 2 - 64 slots, each slot is 64ms
// Level 3 - 64 slots, each slot is 4096ms (4 seconds)
// Level 4 - 64 slots, each slot is 262144ms (4 minutes and 22 seconds)
// Level 5 - 64 slots, each slot is 16777216ms (around 4 hours)
//
// This gives coverage of around 250 hours, which is more than enough for automotive applications.
//

const SLOTS_CNT: u64 = 64;
const MAX_TIMEOUT_TIME: u64 = SLOTS_CNT * 16777216;
const MAX_LEVELS: usize = 5;

pub struct TimeDriver {
    inner: RwLock<Inner>,
    start_time: Instant,
}

impl TimeDriver {
    /// Creates a new TimeDriver with a specified number of timers supported.
    pub fn new(num_of_timers: usize) -> Self {
        let now: Instant = Clock::now();

        Self {
            inner: RwLock::new(Inner::new(num_of_timers, now)),
            start_time: now,
        }
    }

    /// Returns the maximum timeout time that can be registered with the TimeDriver.
    pub const fn max_timeout() -> u64 {
        MAX_TIMEOUT_TIME
    }

    ///
    /// Processes the timeouts that have expired since the last poll.
    ///
    pub fn process_timeouts(&self, now: Instant) {
        let mut inner = self.inner.write().unwrap();
        inner.process_timeouts(now);
    }

    ///
    /// Register a `waker` to be called when the timeout expires. The timeout limit is `MAX_TIMEOUT_TIME`.
    ///
    /// The `expire_at` parameter specifies the time when the timeout should expire.
    ///
    /// # Errors
    /// `CommonErrors::WrongArgs` if the timeout is too far in the future
    /// `CommonErrors::AlreadyDone` if the timeout is before last processed time.
    /// `CommonErrors::NoSpaceLeft` if there is no space left to register the timeout.
    ///
    pub fn register_timeout(&self, expire_at: Instant, waker: Waker) -> Result<(), CommonErrors> {
        let inner = self.inner.read().unwrap();
        inner.register_timeout(expire_at, waker)
    }

    ///
    /// Returns next time the poll shall happen to not miss next deadline.
    /// # ATTENTION
    /// After this call user can register timeout closer to returned time, upper layer need to ensure correct logic to keep up with deadlines
    ///
    pub fn next_process_time(&self) -> Option<Instant> {
        let inner = self.inner.read().unwrap();
        inner.next_process_time().map(|(instant, _)| instant)
    }

    pub fn instant_into_u64(&self, point: Instant) -> u64 {
        point
            .saturating_duration_since(self.start_time)
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    pub fn duration_since_now(&self, point: Instant) -> Duration {
        let now = Clock::now();

        Duration::from_millis(self.instant_into_u64(point) - self.instant_into_u64(now))
    }
}

// TODO: This is a problem because this takes stack memory during construction.
// We can get rid of it implementing FixedSizePoolAllocator facade that uses fully heap memory at start
const MAX_TIMERS: usize = 1024 * 32;

struct Inner {
    levels: Box<[wheel::TimeWheel]>,
    pool: FixedSizePoolAllocator<MAX_TIMERS>,
    last_check_time: u64,
    start_time: Instant,
}

impl Inner {
    fn new(num_of_timers: usize, start_time: Instant) -> Self {
        assert!(
            num_of_timers <= MAX_TIMERS,
            "Currently num of timers limited at compile time to max {}",
            MAX_TIMERS
        );

        let levels = create_arr_storage(MAX_LEVELS, |i| wheel::TimeWheel::new(i as u8));

        let storage_size = num_of_timers * Layout::new::<TimeEntry>().pad_to_align().size();
        let x = Box::leak(create_arr_storage(storage_size, |_| 0));

        Self {
            levels,
            pool: FixedSizePoolAllocator::new(Layout::new::<TimeEntry>(), NonNull::from(&mut x[0]), storage_size),
            last_check_time: 0,
            start_time,
        }
    }

    ///
    /// Processes the timeouts that have expired since the last poll.
    ///
    fn process_timeouts(&mut self, now: Instant) {
        let now_milis = self.instant_into_u64(now);

        self.process_internal(now_milis);
    }

    ///
    /// Returns the next time when the poll should happen.
    /// This will return a time when first thing will expire. Keep in mind that this can change after this call since user could schedule something afterwards
    ///
    fn next_process_time(&self) -> Option<(Instant, u64)> {
        self.find_next_expiring(self.last_check_time).map(|info| {
            (
                self.start_time + std::time::Duration::from_millis(info.deadline),
                info.deadline - self.last_check_time,
            )
        })
    }

    fn register_timeout(&self, expire_at: Instant, waker: Waker) -> Result<(), CommonErrors> {
        assert!(
            expire_at >= self.start_time,
            "Cannot register timeout in the past {:?}, start {:?}",
            expire_at,
            self.start_time
        );

        let expire_at_msec = self.instant_into_u64(expire_at);

        if expire_at_msec < self.last_check_time {
            return Err(CommonErrors::AlreadyDone);
        }

        if (expire_at_msec - self.last_check_time) > MAX_TIMEOUT_TIME {
            return Err(CommonErrors::WrongArgs);
        }

        self.register_timeout_internal(expire_at_msec, waker)
    }

    fn process_internal(&mut self, now: u64) {
        assert!(
            now >= self.last_check_time,
            "Cannot process time in the past (now {} last check {})",
            now,
            self.last_check_time
        );

        loop {
            match self.find_next_expiring(self.last_check_time) {
                // we are checking what expires when from last time
                Some(info) if info.deadline <= now => {
                    self.process_expired(&info);
                    self.set_last_check_time(info.deadline); // advance last_check_time to the expiration time since this is where we are now
                }
                _ => {
                    // finally. no one expires so we can set current processing time
                    self.set_last_check_time(now);
                    break;
                }
            }
        }
    }

    fn set_last_check_time(&mut self, now: u64) {
        debug_assert!(
            self.last_check_time <= now,
            "Now cannot go back, now {}, last check {}",
            now,
            self.last_check_time
        );

        self.last_check_time = now;
    }

    fn find_next_expiring(&self, now: u64) -> Option<ExpireInfo> {
        for e in &self.levels {
            if let Some(info) = e.next_expiration(now) {
                // TODO: would be good to add some debug checks
                return Some(info);
            }
        }

        None
    }

    fn process_expired(&mut self, info: &ExpireInfo) {
        let iter = self.levels[info.level as usize].aquire_slot(info);
        // let now = self.instant_into_u64(Clock::now());

        for e in iter {
            let data = unsafe { e.as_ref() };

            //TODO: We could keep waker list on side to fire them outside of the lock. This is next step improvement once we will connect this all into workers
            if data.data.expire_at <= info.deadline {
                // Wake up the task
                data.data.waker.wake_by_ref();

                unsafe {
                    std::ptr::drop_in_place(e.as_ptr());
                    self.pool.deallocate(e.cast::<u8>(), Layout::new::<TimeEntry>())
                };
            } else {
                let level = self.compute_level_for_timer(info.deadline, data.data.expire_at);
                self.levels[level].register_wakeup_on_timeout(data.data.expire_at, e);
            }
        }
    }

    // API assumes that expire_at_msec fits into MAX_TIMEOUT_TIME
    fn register_timeout_internal(&self, expire_at_msec: u64, waker: Waker) -> Result<(), CommonErrors> {
        let entry = self.allocate_entry(waker, expire_at_msec).map_err(|_| CommonErrors::NoSpaceLeft)?;

        let level = self.compute_level_for_timer(self.last_check_time, expire_at_msec);
        self.levels[level].register_wakeup_on_timeout(expire_at_msec, entry);

        Ok(())
    }

    // find out on which level we shall register timer
    fn compute_level_for_timer(&self, last_check_time: u64, expire_at: u64) -> usize {
        const SLOT_MASK: u64 = (1 << 6) - 1; // Slots are using 6 bits

        // XOR here "turn on" first bit that tells us whats the logical distance between last_check_time and expire_at
        // OR only prevents issues with 0
        let masked = last_check_time ^ expire_at | SLOT_MASK;

        let leading_zeros = masked.leading_zeros() as usize;
        let significant = 63 - leading_zeros;

        // The u64 range for 6 bits can cover around 10 levels. We will not return more than 6 from
        // here anyway since expire_at is limited to MAX_TIMEOUT_TIME on upper levels
        significant / 6
    }

    fn allocate_entry(&self, waker: Waker, expire_at: u64) -> Result<NonNull<TimeEntry>, AllocationError> {
        let e = self.pool.allocate(Layout::new::<TimeEntry>())?.cast::<TimeEntry>();

        unsafe {
            std::ptr::write(
                e.as_ptr(),
                TimeEntry {
                    next: foundation::sync::foundation_atomic::FoundationAtomicPtr::new(std::ptr::null_mut()),
                    data: wheel::TimeoutData { waker, expire_at },
                },
            );
        };

        Ok(e)
    }

    fn instant_into_u64(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.start_time).as_millis().try_into().unwrap_or(u64::MAX)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Drop all entries in a wheel
        for l in &mut self.levels {
            for i in 0..64 {
                let info = ExpireInfo {
                    level: l.level(),
                    slot_id: i,
                    deadline: u64::MAX,
                };
                let iter = l.aquire_slot(&info);

                for e in iter {
                    unsafe {
                        std::ptr::drop_in_place(e.as_ptr());
                    }
                }
            }
        }
    }
}

// Disabled for miri since iceoryx2 pool makes miri stuck
#[cfg(not(miri))]
#[cfg(test)]
#[cfg(not(loom))]
mod tests {

    use std::{
        collections::HashSet,
        sync::Mutex,
        task::Wake,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use testing::prelude::{CallableTrait, *};

    use super::*;

    pub struct MockWaker {
        pub mock: Mutex<MockFn<()>>,
    }

    impl MockWaker {
        pub fn new(mock: MockFn<()>) -> Self {
            MockWaker { mock: Mutex::new(mock) }
        }

        pub fn into_arc(self) -> std::sync::Arc<Self> {
            std::sync::Arc::new(self)
        }

        pub fn times(&self) -> usize {
            self.mock.lock().unwrap().times()
        }
    }

    impl Wake for MockWaker {
        fn wake(self: std::sync::Arc<Self>) {
            self.mock.lock().unwrap().call();
        }

        fn wake_by_ref(self: &std::sync::Arc<Self>) {
            self.mock.lock().unwrap().call();
        }
    }

    fn poll_time_as_instant(start_time: Instant, time: u64) -> Instant {
        if time < 64 {
            start_time + Duration::from_millis(time) // no need to adjust, it is already in the first slot
        } else if time < 64 * 64 {
            start_time + Duration::from_millis(time - time % 64) // 1 second
        } else if time < 64 * 64 * 64 {
            start_time + Duration::from_millis(time - time % 64 * 64) // 4 seconds
        } else {
            start_time + Duration::from_millis(time - time % 64 * 64 * 64) // 4 minutes and 22 seconds
        }
    }

    fn expire_time_into_next_poll_time(time: u64) -> u64 {
        if time < 64 {
            time // no need to adjust, it is already in the first slot
        } else if time < 64 * 64 {
            time - time % 64 // 1 second
        } else if time < 64 * 64 * 64 {
            time - time % 64 * 64 // 4 seconds
        } else {
            time - time % 64 * 64 * 64 // 4 minutes and 22 seconds
        }
    }

    #[test]
    fn when_no_more_timers_returns_error() {
        let start_time = Clock::now();

        let driver = Inner::new(1, start_time);

        driver.register_timeout_internal(3800, Waker::noop().clone()).unwrap();

        assert!(driver.register_timeout_internal(3800, Waker::noop().clone()).is_err());
    }

    #[test]
    fn test_time_driver() {
        let start_time = Clock::now();

        let mock = MockWaker::new(MockFnBuilder::new().times(2).build());
        let mut driver = Inner::new(123, start_time);

        let waker: Waker = mock.into_arc().into();

        driver.register_timeout_internal(3800, waker.clone()).unwrap();

        driver.register_timeout_internal(3800, waker.clone()).unwrap();

        driver.register_timeout_internal(3800, waker.clone()).unwrap();
        driver.register_timeout_internal(3800, waker.clone()).unwrap();
        driver.register_timeout_internal(3800, waker.clone()).unwrap();

        driver.register_timeout_internal(3456, waker.clone()).unwrap();

        driver.register_timeout_internal(2345, waker.clone()).unwrap();
        driver.register_timeout_internal(1230, waker).unwrap();

        let next_process_time = driver.next_process_time().unwrap();
        assert_eq!(next_process_time.1, expire_time_into_next_poll_time(1230));
        assert_eq!(next_process_time.0, poll_time_as_instant(start_time, 1230));

        driver.process_internal(2500);

        let next_process_time = driver.next_process_time().unwrap();
        assert_eq!(next_process_time.1, expire_time_into_next_poll_time(3456) - 2500);
        assert_eq!(next_process_time.0, poll_time_as_instant(start_time, 3456));
    }

    #[test]
    fn poll_within_same_level_window() {
        let start_time = Clock::now();

        let mock = MockWaker::new(MockFnBuilder::new().times(0).build());
        let mut driver = Inner::new(123, start_time);

        let waker: Waker = mock.into_arc().into();

        driver.process_internal(1238);
        driver.register_timeout_internal(1916, waker.clone()).unwrap();
        driver.process_internal(1239);
    }

    #[test]
    fn public_register_before_last_poll() {
        let start_time = Clock::now();

        let mock = MockWaker::new(MockFnBuilder::new().times(1).build());

        let mut driver = Inner::new(123, start_time);

        std::thread::sleep(Duration::from_millis(10)); // Ensure we have a different time

        let waker: Waker = mock.into_arc().into();

        let now = Clock::now();
        let reg_time = start_time + now.checked_duration_since(start_time).unwrap() / 2;

        driver.register_timeout(reg_time, waker.clone()).unwrap();

        let poll_time = Clock::now();
        assert!(poll_time > reg_time, "Poll time should not be equal to registration time");

        driver.process_timeouts(poll_time);

        assert_eq!(driver.register_timeout(reg_time, waker.clone()), Err(CommonErrors::AlreadyDone));
    }

    #[test]
    fn public_register_too_long_timeout() {
        let start_time = Clock::now();

        let mock = MockWaker::new(MockFnBuilder::new().times(0).build());
        let driver = Inner::new(123, start_time);
        std::thread::sleep(Duration::from_millis(10)); // Ensure we have a different time

        let waker: Waker = mock.into_arc().into();

        let reg_time = Clock::now().checked_add(Duration::from_millis(MAX_TIMEOUT_TIME + 1)).unwrap();
        assert_eq!(driver.register_timeout(reg_time, waker.clone()), Err(CommonErrors::WrongArgs));
    }

    struct SimpleRng {
        seed: u64,
        state: u64,
    }

    impl SimpleRng {
        #[allow(dead_code)]
        fn new() -> Self {
            let seed = SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards").as_nanos() as u64;

            SimpleRng { seed, state: seed }
        }

        #[allow(dead_code)]
        fn from(seed: u64) -> Self {
            SimpleRng { seed, state: seed }
        }

        fn get_seed(&self) -> u64 {
            self.seed
        }

        // Linear Congruential Generator (LCG)
        fn next_u32(&mut self) -> u64 {
            // Constants from Numerical Recipes
            self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (self.state >> 32) as u64
        }

        /// Generates a random number in the range [0, max)
        fn gen_range(&mut self, max: u64) -> u64 {
            self.next_u32() % max
        }
    }

    fn are_unique<T: Ord>(sorted: &Vec<T>) -> bool {
        sorted.windows(2).all(|w| w[0] != w[1])
    }

    fn generate_unique_numbers(n: usize, rng: &mut SimpleRng) -> Vec<u64> {
        let mut numbers = HashSet::new();

        while numbers.len() < n {
            let num = rng.gen_range(MAX_TIMEOUT_TIME);
            numbers.insert(num);
        }

        numbers.into_iter().collect()
    }

    // Below are fuzzy tests based on random generated data. When tests fails it logs seed that was used for the test.
    // If you find a failure, you can re-run the test with the seed to reproduce the issue (SimpleRng::from).

    #[test]
    #[cfg(not(miri))]
    fn fuzzy_polling_before_in_after() {
        const NUM_OF_TIMERS: usize = 30000;
        let start_time = Clock::now();

        let mut rng = SimpleRng::new();
        let seed: u64 = rng.get_seed();
        println!("Generated seed: {}", seed);

        let mock = MockWaker::new(MockFnBuilder::new().times(NUM_OF_TIMERS - 1).build()).into_arc();
        let waker: Waker = mock.clone().into();
        let mut driver = Inner::new(NUM_OF_TIMERS + 1, start_time);
        let mut timeouts: Vec<u64> = generate_unique_numbers(NUM_OF_TIMERS, &mut rng);

        for (i, t) in timeouts.iter().enumerate() {
            match driver.register_timeout_internal(*t, waker.clone()) {
                Ok(_) => {}
                Err(e) => assert!(false, "Failed to register timeout at {} with {:?}, iter {}", t, e, i),
            }
        }

        timeouts.sort(); // Sorted for easier processing
        assert!(are_unique(&timeouts), "Timeouts are not unique"); // This tests works only with unique timeouts

        let mut prev = 0;

        for i in 0..timeouts.len() - 1 {
            let mut current_call_times = mock.times();

            let current = timeouts[i];
            let next = timeouts[i + 1];

            let before = prev + rng.next_u32() as u64 % (current - prev);

            driver.process_internal(before);
            assert_eq!(current_call_times, mock.times(), "Polled in {} should timeout in {}", before, current); // Nothing shall change

            driver.process_internal(current);
            current_call_times += 1;

            assert_eq!(current_call_times, mock.times(), "Polled in {} should timeout in {}", current, current); // Shall fire

            let after = current + rng.next_u32() as u64 % (next - current);
            driver.process_internal(after);
            assert_eq!(current_call_times, mock.times(), "Polled in {} already timeout at {}", after, current); // Nothing shall change

            prev = after;
        }
    }

    fn count_less_eq_than<T: Ord>(sorted: &[T], x: &T) -> usize {
        match sorted.binary_search(x) {
            Ok(index) => {
                let mut reps = 0;
                let mut start = sorted[index..].iter();
                let mut startr = sorted[index..].iter().rev();

                while let Some(v) = start.next() {
                    if *v == *x {
                        reps += 1;
                    } else {
                        break;
                    }
                }

                while let Some(v) = startr.next() {
                    if *v == *x {
                        reps += 1;
                    } else {
                        break;
                    }
                }

                index + reps
            }
            Err(index) => index,
        }
    }

    #[test]
    #[cfg(not(miri))]
    fn fuzzy_polling_in_random_places() {
        const NUM_OF_TIMERS: usize = 30000;
        let start_time = Clock::now();

        let mut rng = SimpleRng::new();
        let seed: u64 = rng.get_seed();
        println!("Generated seed: {}", seed);

        let mut poll_times: Vec<u64> = generate_unique_numbers(NUM_OF_TIMERS / 10, &mut rng);
        poll_times.sort();

        let mut timeouts: Vec<u64> = generate_unique_numbers(NUM_OF_TIMERS, &mut rng);

        let mut sorted_timeouts: Vec<u64> = timeouts.clone();
        sorted_timeouts.sort();
        let samples_before_last_poll = count_less_eq_than(&sorted_timeouts, poll_times.last().unwrap());

        let mock = MockWaker::new(MockFnBuilder::new().times(samples_before_last_poll).build()).into_arc();
        let waker: Waker = mock.clone().into();
        let mut driver = Inner::new(NUM_OF_TIMERS + 1, start_time);

        for (i, t) in timeouts.iter().enumerate() {
            match driver.register_timeout_internal(*t, waker.clone()) {
                Ok(_) => {}
                Err(e) => assert!(false, "Failed to register timeout at {} with {:?}, iter {}", t, e, i),
            }
        }

        timeouts.sort(); // Sorted for easier processing
        assert!(are_unique(&timeouts), "Timeouts are not unique"); // This tests works only with unique timeouts

        for p in poll_times {
            let cnt_before = count_less_eq_than(&timeouts, &p);
            driver.process_internal(p);
            assert_eq!(cnt_before, mock.times(), "Polled in {} should now be called  {} times", p, cnt_before);
        }
    }

    #[test]
    #[cfg(not(miri))]
    fn fuzzy_polling_in_random_places_with_many_nodes_in_slots() {
        const NUM_OF_TIMERS: usize = 30000;
        let start_time = Clock::now();

        let mut rng = SimpleRng::new();
        let seed: u64 = rng.get_seed();
        println!("Generated seed: {}", seed);

        fn generate(rng: &mut SimpleRng, level: u32) -> Vec<u64> {
            let num_of_slots_taken = rng.gen_range(SLOTS_CNT); // how many slots will be taken at start in single wheel

            // Level from 0
            let slot_range = SLOTS_CNT.pow(level);
            let level_range = slot_range * SLOTS_CNT;

            let mut ret = Vec::<u64>::new();

            for _ in 0..num_of_slots_taken {
                let slot_taken = rng.gen_range(SLOTS_CNT) + 1;

                let mut numbers = generate_unique_numbers(rng.gen_range(100) as usize, rng);

                for n in numbers.iter_mut() {
                    *n = *n / (slot_taken * level_range); // align to slot beginning

                    *n += rng.gen_range(slot_range); // randomize offset in slot
                }

                ret.extend(numbers);
            }

            ret
        }

        let mut poll_times: Vec<u64> = generate_unique_numbers(NUM_OF_TIMERS / 10, &mut rng);
        poll_times.sort();

        // Generate timeouts for each level
        let mut timeouts = generate(&mut rng, 0);
        timeouts.extend(generate(&mut rng, 1));
        timeouts.extend(generate(&mut rng, 2));
        timeouts.extend(generate(&mut rng, 3));
        timeouts.extend(generate(&mut rng, 4));

        let mut sorted_timeouts: Vec<u64> = timeouts.clone();
        sorted_timeouts.sort();
        let samples_before_last_poll = count_less_eq_than(&sorted_timeouts, poll_times.last().unwrap());

        let mock = MockWaker::new(MockFnBuilder::new().times(samples_before_last_poll).build()).into_arc();
        let waker: Waker = mock.clone().into();
        let mut driver = Inner::new(NUM_OF_TIMERS + 1, start_time);

        for (i, t) in timeouts.iter().enumerate() {
            match driver.register_timeout_internal(*t, waker.clone()) {
                Ok(_) => {}
                Err(e) => assert!(false, "Failed to register timeout at {} with {:?}, iter {}", t, e, i),
            }
        }

        timeouts.sort(); // Sorted for easier processing

        for p in poll_times {
            let cnt_before = count_less_eq_than(&timeouts, &p);
            driver.process_internal(p);
            assert_eq!(cnt_before, mock.times(), "Polled in {} should now be called  {} times", p, cnt_before);
        }
    }
}
