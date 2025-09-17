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

use crate::mio::{
    registry::Registry,
    types::{IoCall, IoEvent, IoEventInterest, IoId, IoRegistryEntry, IoResult, IoSelector, IoSelectorEventContainer, Result},
};
use core::time::Duration;
use foundation::{containers::vector_extension::VectorExtension, not_recoverable_error, prelude::*};
use iceoryx2_bb_container::flatmap::FlatMap;
use libc::{
    close, fcntl, pipe, poll, pollfd, read, write, EAGAIN, EINTR, FD_CLOEXEC, F_SETFD, F_SETFL, O_CLOEXEC, O_NONBLOCK, POLLERR, POLLHUP, POLLIN,
    POLLOUT, POLLPRI,
};
use std::{
    ffi,
    os::fd::{AsRawFd, RawFd},
};

#[cfg(loom)]
use loom::sync::{Arc, Condvar, Mutex};
#[cfg(not(loom))]
use std::sync::{Arc, Condvar, Mutex};

/// An object used to wait for events on the registered file descriptors.
#[derive(Clone)]
pub struct Selector {
    inner: Arc<Inner>,
}

impl Selector {
    /// Create an `IoSelector` with a set file descriptor capacity.
    pub fn new(fd_capacity: usize) -> Self {
        let mut fds = Fds::new(1 + fd_capacity);
        let poll_waker = InternalWaker::new().expect("Failed to create the internal InternalWaker");
        fds.add(poll_waker.read_fd, IoId::new(u64::default()), IoEventInterest::READABLE, true)
            .expect("Failed to add the InternalWaker");

        Selector {
            inner: Arc::new(Inner {
                fds: Mutex::new(fds),
                fd_capacity,
                poll_waker,
                num_of_accesses: FoundationAtomicUsize::new(0),
                accesses_finished: Condvar::new(),
            }),
        }
    }
}

impl IoSelector for Selector {
    type IoProxy<T: AsRawFd> = Proxy<T>;
    type Waker = SelectWaker;

    /// Register a file descriptor for events.
    ///
    ///`select` will generate an event with the given `id` if the file matches `interest`.
    /// An event is only generated once per `interest`. For `select` to generate the event again the file descriptor needs to be reregistered.
    ///
    /// Returns `Err(CommonErrors::AlreadyDone)` if `fd` was already registered.
    /// Returns `Err(CommonErrors::NoSpaceLeft)` if capacity was reached.
    /// Returns `Err(CommonErrors::WrongArgs)` if the fd is negative.
    fn register(&self, fd: RawFd, id: IoId, interest: IoEventInterest) -> Result<()> {
        self.inner.register(fd, id, interest)
    }

    /// Update the `IoEventInterest` and `IoId` for an already registered file descriptor.
    ///
    /// Returns `Err(CommonErrors::NotFound)` if `fd` wasn't already registered.
    /// Returns `Err(CommonErrors::NotSupported)` if an error was reported for the `fd`.
    fn reregister(&self, fd: RawFd, id: IoId, interest: IoEventInterest) -> Result<()> {
        self.inner.reregister(fd, id, interest)
    }

    /// Deregister the file descriptor.
    ///
    /// Returns `Err(CommonErrors::NotFound)` if `fd` wasn't already registered.
    fn deregister(&self, fd: RawFd) -> Result<()> {
        self.inner.deregister(fd)
    }

    /// Creates a waker that can be used to unblock the `select` call.
    ///
    /// Returns `Err(CommonErrors::GenericError)` if the waker couldn't be created.
    fn create_waker(&self, id: IoId) -> Result<SelectWaker> {
        Ok(SelectWaker(self.inner.create_waker(id)?, Arc::clone(&self.inner)))
    }

    /// Returns how many file descriptors and wakers can be registered at the same time.
    fn capacity(&self) -> usize {
        self.inner.fd_capacity
    }

    /// Block the thread and wait for events on registered file descriptors.
    ///
    /// Events for registered file descriptors will be reported with `interest` that's available for the file.
    /// Calling `wake` on a created waker will generate an event with the waker `id`, and an unset `interest`.
    ///
    /// Returns `Err(CommonErrors::Timeout)` if the timeout was reached.
    /// Returns `Err(CommonErrors::NoSpaceLeft)` if `events` capacity was reached, and thus some events were dropped.
    fn select<C: IoSelectorEventContainer>(&self, events: &mut C, timeout: Option<Duration>) -> Result<()> {
        self.inner.select(events, timeout)
    }
}

pub struct Proxy<S> {
    resource: S,
    registry: Option<Registry<Selector>>,
    id: IoId,
    interest: IoEventInterest,
}

impl<S> IoCall<S> for Proxy<S>
where
    S: AsRawFd,
{
    fn io_call<F, R>(&self, f: F) -> IoResult<R>
    where
        F: FnOnce(&S) -> IoResult<R>,
    {
        f(&self.resource).map_err(|e| {
            if e.kind() == std::io::ErrorKind::WouldBlock {
                if let Err(e) = self
                    .registry
                    .as_ref()
                    .expect("Registry must be set before calling io_call")
                    .selector
                    .reregister(self.resource.as_raw_fd(), self.id, self.interest)
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))
                {
                    return e;
                };
            }

            e
        })
    }

    fn new(resource: S) -> Self {
        Proxy {
            resource,
            registry: None,
            id: IoId::new(0),
            interest: IoEventInterest(0),
        }
    }

    fn as_inner(&self) -> &S {
        &self.resource
    }

    fn as_inner_mut(&mut self) -> &mut S {
        &mut self.resource
    }
}

impl<S> IoRegistryEntry<Selector> for Proxy<S>
where
    S: AsRawFd,
{
    fn register(&mut self, registry: &Registry<Selector>, id: IoId, interest: IoEventInterest) -> Result<()> {
        self.id = id;
        self.interest = interest;
        self.registry
            .get_or_insert_with(|| registry.clone())
            .selector
            .register(self.resource.as_raw_fd(), id, interest)
    }

    fn reregister(&mut self, id: IoId, interest: IoEventInterest) -> Result<()> {
        self.id = id;
        self.interest = interest;
        self.registry
            .as_ref()
            .expect("Registry must be set before calling reregister")
            .selector
            .reregister(self.resource.as_raw_fd(), id, interest)
    }

    fn deregister(&mut self) -> Result<()> {
        self.registry
            .as_ref()
            .expect("Registry must be set before calling deregister")
            .selector
            .deregister(self.resource.as_raw_fd())
    }
}

/// An object that can unblock a `select` call for the associated `IoSelector`.
pub struct SelectWaker(InternalWaker, Arc<Inner>);

impl SelectWaker {
    /// Unblock a `select` call for the `IoSelector` object for which this waker was registered.
    pub fn wake(&self) {
        self.0.wake();
    }
}

impl Drop for SelectWaker {
    fn drop(&mut self) {
        self.1.deregister(self.0.read_fd).expect("Failed to deregister SelectWaker");
    }
}

struct InternalWaker {
    read_fd: RawFd,
    write_fd: RawFd,
}

impl InternalWaker {
    /// Wrapper around pipe to provide functionality similar to pipe2 since it is not available on QNX.
    /// This creates a pipe and then sets the flags manually. The supported flags are O_NONBLOCK and O_CLOEXEC.
    fn pipe_with_flags(fds: &mut [RawFd; 2], flags: i32) -> i32 {
        if unsafe { pipe(fds.as_mut_ptr()) } == -1 {
            return -1;
        }

        for fd in fds.iter() {
            if (flags & O_NONBLOCK) != 0 {
                unsafe {
                    if fcntl(*fd, F_SETFL, O_NONBLOCK) == -1 {
                        close(fds[0]);
                        close(fds[1]);
                        return -1;
                    }
                }
            }
            if (flags & O_CLOEXEC) != 0 {
                unsafe {
                    if fcntl(*fd, F_SETFD, FD_CLOEXEC) == -1 {
                        close(fds[0]);
                        close(fds[1]);
                        return -1;
                    }
                }
            }
        }
        0 // Success
    }

    fn new() -> Option<Self> {
        let mut fds: [RawFd; 2] = [-1, -1];

        match Self::pipe_with_flags(&mut fds, O_NONBLOCK | O_CLOEXEC) {
            -1 => None,
            _ => Some(InternalWaker {
                read_fd: fds[0],
                write_fd: fds[1],
            }),
        }
    }

    fn wake(&self) {
        let flag = 1_u8;
        loop {
            let res = unsafe { write(self.write_fd, &flag as *const u8 as *const ffi::c_void, 1) };

            match res {
                0 => {
                    not_recoverable_error!("There shall be no write with 0 bytes written, some error happened");
                }
                -1 => {
                    let err = std::io::Error::last_os_error().raw_os_error().unwrap();
                    match err {
                        // EAGAIN means the read would block. Since this is a pipe, the assumtion is
                        // that it's thus readable and that poll will be awaken, thus don't have to do anything.
                        EAGAIN => break,
                        EINTR => (), // Retry.
                        _ => not_recoverable_error!(with err, "InternalWaker write failed"),
                    }
                }
                _ => break, // Successfully wrote some data.
            }
        }
    }

    fn clear(&self) {
        Self::clear_read_fd(self.read_fd);
    }

    fn clear_read_fd(fd: RawFd) {
        let mut buff: [u8; 32] = Default::default();
        loop {
            if unsafe { read(fd, buff.as_mut_ptr().cast::<ffi::c_void>(), buff.len()) } == -1 {
                match std::io::Error::last_os_error().raw_os_error().unwrap() {
                    EAGAIN => break, // No more data to read.
                    EINTR => (),     // Retry.
                    e => {
                        warn!("InternalWaker read failed with error {}", e);
                        break;
                    }
                }
            } else {
                // All data has been read.
                break;
            }
        }
    }
}

impl Drop for InternalWaker {
    fn drop(&mut self) {
        if unsafe { close(self.read_fd) } == -1 {
            warn!("InternalWaker failed to close read");
        }

        if unsafe { close(self.write_fd) } == -1 {
            warn!("InternalWaker failed to close write");
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct FdInfo {
    id: IoId,
    is_waker: bool,
}

struct Fds {
    fd_to_index: FlatMap<RawFd, usize>,
    infos: Vec<FdInfo>,
    pollfds: Vec<pollfd>,
}

impl Fds {
    fn new(capacity: usize) -> Self {
        Self {
            fd_to_index: FlatMap::new(capacity),
            infos: Vec::new(capacity),
            pollfds: Vec::new(capacity),
        }
    }
    fn add(&mut self, fd: RawFd, id: IoId, interest: IoEventInterest, is_waker: bool) -> Result<()> {
        if fd.is_negative() {
            return Err(CommonErrors::WrongArgs);
        }

        if self.contains(&fd) {
            return Err(CommonErrors::AlreadyDone);
        }

        if self.is_full() {
            return Err(CommonErrors::NoSpaceLeft);
        }

        // None of these can fail. Capacity was verified above.
        self.fd_to_index.insert(fd, self.infos.len()).expect("Failed to add file descriptor");
        self.infos.push(FdInfo { id, is_waker });
        self.pollfds.push(pollfd {
            fd,
            events: poll_events_from_interest(&interest),
            revents: 0,
        });

        Ok(())
    }

    fn update(&mut self, fd: RawFd, id: IoId, interest: IoEventInterest) -> Result<()> {
        if let Some(index) = self.fd_to_index.get(&fd) {
            let info = &mut self.infos[index];
            let pollfds = &mut self.pollfds[index];

            if pollfds.fd < 0 {
                // An error occured for this fd. The fd is no longer valid.
                Err(CommonErrors::NotSupported)
            } else {
                info.id = id;
                pollfds.events = poll_events_from_interest(&interest);

                Ok(())
            }
        } else {
            Err(CommonErrors::NotFound)
        }
    }

    fn remove(&mut self, fd: RawFd) -> Result<()> {
        if let Some(index) = self.fd_to_index.get(&fd) {
            self.fd_to_index.remove(&fd);
            self.infos.swap_remove(index);
            self.pollfds.swap_remove(index);

            // If index wasn't last, then fd_to_index needs to be adjusted, as last elements of infos and pollfds are now at index.
            if index < self.pollfds.len() {
                let pollfd = &self.pollfds[index];
                // If fd is negative, then it was negated by poll, and thus needs to be negated back to get the original fd.
                let fd = if pollfd.fd.is_positive() { pollfd.fd } else { !pollfd.fd };

                *self.fd_to_index.get_mut_ref(&fd).unwrap() = index;
            }

            Ok(())
        } else {
            Err(CommonErrors::NotFound)
        }
    }

    fn contains(&self, fd: &RawFd) -> bool {
        self.fd_to_index.contains(fd)
    }

    fn len(&self) -> usize {
        self.fd_to_index.len()
    }

    fn is_full(&self) -> bool {
        self.infos.len() == self.infos.capacity()
    }

    fn get_mut(&mut self, index: usize) -> (&mut FdInfo, &mut pollfd) {
        (&mut self.infos[index], &mut self.pollfds[index])
    }
}

fn poll_events_from_interest(interest: &IoEventInterest) -> i16 {
    let mut events = 0;

    if interest.is_readable() {
        events |= POLLIN | POLLPRI;
    }
    if interest.is_writable() {
        events |= POLLOUT;
    }

    events
}

fn interest_from_poll_events(events: i16) -> IoEventInterest {
    let mut interest = IoEventInterest(0);

    if events & (POLLIN | POLLPRI) != 0 {
        interest = interest | IoEventInterest::READABLE;
    }

    if events & POLLOUT != 0 {
        interest = interest | IoEventInterest::WRITABLE;
    }

    interest
}

struct Inner {
    fds: Mutex<Fds>,
    // The file descriptor capacity is stored separately to avoid locking fds just to get capacity,
    // and because fds holds the internal waker, so its capacity is actually larger.
    fd_capacity: usize,
    poll_waker: InternalWaker,
    num_of_accesses: FoundationAtomicUsize,
    accesses_finished: Condvar,
}

impl Inner {
    fn register(&self, fd: RawFd, id: IoId, interest: IoEventInterest) -> Result<()> {
        self.access_fds(|fds| fds.add(fd, id, interest, false))
    }

    fn reregister(&self, fd: RawFd, id: IoId, interest: IoEventInterest) -> Result<()> {
        self.access_fds(|fds| fds.update(fd, id, interest))
    }

    fn deregister(&self, fd: RawFd) -> Result<()> {
        self.access_fds(|fds| fds.remove(fd))
    }

    fn create_waker(&self, id: IoId) -> Result<InternalWaker> {
        if let Some(waker) = InternalWaker::new() {
            self.access_fds(|fds| fds.add(waker.read_fd, id, IoEventInterest::READABLE, true))?;
            Ok(waker)
        } else {
            Err(CommonErrors::GenericError)
        }
    }

    fn select<Container: IoSelectorEventContainer>(&self, events: &mut Container, timeout: Option<Duration>) -> Result<()> {
        let timeout = timeout.map(|d| d.as_millis() as i32).unwrap_or(-1);
        let mut fds = self.fds.lock().unwrap();

        loop {
            // loom::sync::Condvar doesn't implement wait_while.
            while self.num_of_accesses.load(FoundationOrdering::Acquire) != 0 {
                fds = self.accesses_finished.wait(fds).unwrap();
            }

            let poll_result: i32 = unsafe { poll(fds.pollfds.as_mut_slice().as_mut_ptr(), fds.pollfds.len() as libc::nfds_t, timeout) };

            match poll_result {
                -1 => {
                    let err = std::io::Error::last_os_error().raw_os_error().unwrap();
                    match err {
                        libc::EINTR => continue,
                        _ => not_recoverable_error!(with  err, "Poll failed with error: This is a bug in implementation!"),
                    }
                }
                0 => break Err(CommonErrors::Timeout),
                _ => {
                    // If there's an event only for the internal poll waker,
                    // restart the loop to wait for the pending accesses to finish.
                    // The internal poll waker is cleared by the access operation.

                    let is_internal_waker_ready = fds.pollfds[0].revents != 0;
                    let mut events_processed = 0;

                    if is_internal_waker_ready {
                        events_processed += 1; // To keep it correct for later loop exit condition

                        if poll_result == 1 {
                            continue;
                        }
                    }

                    // Iterate from one as the first fd is the internal waker.
                    for index in 1..fds.len() {
                        if events_processed == poll_result as usize {
                            // No more events to process.
                            break;
                        }

                        if events.len() == events.capacity() {
                            // No more space for events.
                            return Err(CommonErrors::NoSpaceLeft);
                        }

                        let (info, pollfd) = fds.get_mut(index);

                        if pollfd.revents == 0 {
                            // Nothing happened for this file. No event to publish.
                            continue;
                        }

                        if pollfd.revents & (POLLHUP | POLLERR) != 0 {
                            // This sets the file descriptor to negative, which makes all future poll calls ignore it.
                            pollfd.fd = !pollfd.fd;
                        }

                        if info.is_waker {
                            // This can't fail. The capacity was checked above.
                            events.push(IoEvent::new(info.id, IoEventInterest(0)));

                            InternalWaker::clear_read_fd(pollfd.fd);
                        } else {
                            // This can't fail. The capacity was checked above.
                            events.push(IoEvent::new(info.id, interest_from_poll_events(pollfd.revents)));

                            // Clear reported interests. The user needs to re-register the interest to be notified of the event again.
                            pollfd.events &= !pollfd.revents;
                        }

                        events_processed += 1;
                    }

                    return Ok(());
                }
            }
        }
    }

    fn access_fds<R, T: FnOnce(&mut Fds) -> Result<R>>(&self, accessor: T) -> Result<R> {
        self.num_of_accesses.fetch_add(1, FoundationOrdering::AcqRel);
        let prev_num_of_accesses;

        self.poll_waker.wake();
        let result = {
            let mut fds = self.fds.lock().unwrap();
            let res = accessor(&mut fds);

            // Ensure that the final modification that can be observed by select happens under lock. This
            // makes sure that select will not go into w case where:
            // - this fn is past the lock
            // - the select observed that num_of_accesses != 0 and went into wait on CV branch and get scheduled out
            // - this fn decrement atomic to 0 and does notification
            // - select is scheduled goes into wait on CV and misses the notification
            //
            // This is missed-wakeup issue, oposition to spurious-wakeup
            prev_num_of_accesses = self.num_of_accesses.fetch_sub(1, FoundationOrdering::AcqRel);
            res
        };
        self.poll_waker.clear();

        if prev_num_of_accesses == 1 {
            self.accesses_finished.notify_one();
        }

        result
    }
}

impl IoSelectorEventContainer for Vec<IoEvent> {
    fn push(&mut self, event: IoEvent) -> bool {
        self.push(event);
        true
    }

    fn clear(&mut self) {
        self.clear();
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    fn capacity(&self) -> usize {
        self.capacity()
    }
}

#[cfg(not(any(loom, miri)))]
#[cfg(test)]
mod tests {
    use super::*;
    use libc::EAGAIN;
    use std::{io::Error, thread};

    fn create_pipe() -> (RawFd, RawFd) {
        let mut fds: [RawFd; 2] = [-1, -1];
        assert_eq!(InternalWaker::pipe_with_flags(&mut fds, O_NONBLOCK), 0);
        (fds[0], fds[1])
    }

    fn write_until_blocking(fd: RawFd) {
        loop {
            let data = 1_u8;
            if unsafe { write(fd, &data as *const u8 as *const ffi::c_void, 1) } == -1 {
                assert_eq!(Error::last_os_error().raw_os_error().unwrap(), EAGAIN);
                break;
            }
        }
    }

    fn read_until_blocking(fd: RawFd) {
        loop {
            let mut data = 0_u8;
            if unsafe { read(fd, &mut data as *mut u8 as *mut ffi::c_void, 1_usize) } == -1 {
                assert_eq!(Error::last_os_error().raw_os_error().unwrap(), EAGAIN);
                break;
            }
        }
    }

    struct SyncPoint(Arc<(Mutex<bool>, Condvar)>);

    impl SyncPoint {
        fn wait(&self, timeout: Duration) -> bool {
            let (m, c) = &*self.0;
            let mut m = m.lock().unwrap();
            while !*m {
                let result = c.wait_timeout(m, timeout).unwrap();
                if result.1.timed_out() {
                    return false;
                } else {
                    m = result.0;
                }
            }

            true
        }
    }

    fn create_thread<F, R>(f: F) -> (SyncPoint, SyncPoint, thread::JoinHandle<R>)
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let begin_sync = Arc::new((Mutex::new(false), Condvar::new()));
        let begin_sync_clone = Arc::clone(&begin_sync);
        let end_sync = Arc::new((Mutex::new(false), Condvar::new()));
        let end_sync_clone = Arc::clone(&end_sync);

        (
            SyncPoint(begin_sync),
            SyncPoint(end_sync),
            thread::spawn(move || {
                let (begin_m, begin_c) = &*begin_sync_clone;
                {
                    let mut begin_m = begin_m.lock().unwrap();
                    *begin_m = true;
                    begin_c.notify_one();
                }

                let result = f();

                let (end_m, end_c) = &*end_sync_clone;
                {
                    let mut end_m = end_m.lock().unwrap();
                    *end_m = true;
                    end_c.notify_one();
                }

                result
            }),
        )
    }

    #[test]
    fn test_internal_waker_is_registered() {
        let selector = Selector::new(8);

        // When changing this, look into select implementation as it assumes the internal waker is at index 0.
        assert_eq!(selector.inner.fds.lock().unwrap().pollfds.len(), 1);
        assert_eq!(selector.inner.poll_waker.read_fd, selector.inner.fds.lock().unwrap().pollfds[0].fd);
    }

    #[test]
    fn test_register_readable_before_select() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        selector.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert!(selector.select(&mut events, None).is_ok());
            events
        });

        begin_sync.wait(Duration::MAX);

        // Make the pipe readable.
        let data = 1_u8;
        assert_eq!(unsafe { write(write_fd, &data as *const u8 as *const ffi::c_void, 1_usize) }, 1_isize);

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id(), IoId::new(id));
        assert!(events[0].is_readable());
        assert!(!events[0].is_writable());
    }

    #[test]
    fn test_register_writable_before_select() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        selector.register(write_fd, IoId::new(id), IoEventInterest::WRITABLE).unwrap();

        // Make the pipe not writable.
        write_until_blocking(write_fd);

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert!(selector.select(&mut events, None).is_ok());
            events
        });

        begin_sync.wait(Duration::MAX);

        // Make the pipe writable.
        read_until_blocking(read_fd);

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id(), IoId::new(id));
        assert!(events[0].is_writable());
        assert!(!events[0].is_readable());
    }

    #[test]
    fn test_internal_waker_does_not_leak_as_event() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        let selector_clone = selector.clone();

        // Make the pipe readable.
        selector.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();
        let data = 1_u8;
        assert_eq!(unsafe { write(write_fd, &data as *const u8 as *const ffi::c_void, 1_usize) }, 1_isize);

        // Emulate cross thread API call so waker is also woken up but since there is also other event, it will not be put into
        // wait state again but will process all fds
        assert_eq!(
            unsafe { write(selector.inner.poll_waker.write_fd, &data as *const u8 as *const ffi::c_void, 1_usize) },
            1_isize
        );

        let mut events = Vec::<IoEvent>::new(8);
        assert!(selector_clone.select(&mut events, None).is_ok());

        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_register_readable_while_select() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        let selector_clone = selector.clone();

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert!(selector_clone.select(&mut events, None).is_ok());
            events
        });

        begin_sync.wait(Duration::MAX);
        // Wait for the thread to block on poll. Obviously this isn't guaranteed to work, but I have no better idea.
        thread::sleep(Duration::from_secs(2));
        selector.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();

        // Make the pipe readable.
        let data = 1_u8;
        assert_eq!(unsafe { write(write_fd, &data as *const u8 as *const ffi::c_void, 1_usize) }, 1_isize);

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id(), IoId::new(id));
        assert!(events[0].is_readable());
        assert!(!events[0].is_writable());
    }

    #[test]
    fn test_register_writable_while_select() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        let selector_clone = selector.clone();

        // Make the pipe not writable.
        write_until_blocking(write_fd);

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert!(selector_clone.select(&mut events, None).is_ok());
            events
        });

        begin_sync.wait(Duration::MAX);
        // Wait for the thread to block on poll. Obviously this isn't guaranteed to work, but I have no better idea.
        thread::sleep(Duration::from_secs(2));
        selector.register(write_fd, IoId::new(id), IoEventInterest::WRITABLE).unwrap();

        // Make the pipe writable.
        read_until_blocking(read_fd);

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id(), IoId::new(id));
        assert!(events[0].is_writable());
        assert!(!events[0].is_readable());
    }

    #[test]
    fn test_deregister_readable_before_select() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        selector.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();
        selector.deregister(read_fd).unwrap();

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert_eq!(selector.select(&mut events, Some(Duration::from_secs(2))), Err(CommonErrors::Timeout));
            events
        });

        begin_sync.wait(Duration::MAX);

        // Make the pipe readable.
        let data = 1_u8;
        assert_eq!(unsafe { write(write_fd, &data as *const u8 as *const ffi::c_void, 1_usize) }, 1_isize);

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_deregister_writable_before_select() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        selector.register(write_fd, IoId::new(id), IoEventInterest::WRITABLE).unwrap();
        selector.deregister(write_fd).unwrap();

        // Make the pipe not writable.
        write_until_blocking(write_fd);

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert_eq!(selector.select(&mut events, Some(Duration::from_secs(2))), Err(CommonErrors::Timeout));
            events
        });

        begin_sync.wait(Duration::MAX);

        // Make the pipe writable.
        read_until_blocking(read_fd);

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_deregister_readable_while_select() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        let selector_clone = selector.clone();
        selector.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert_eq!(
                selector_clone.select(&mut events, Some(Duration::from_secs(2))),
                Err(CommonErrors::Timeout)
            );
            events
        });

        begin_sync.wait(Duration::MAX);
        // Wait for the thread to block on poll. Obviously this isn't guaranteed to work, but I have no better idea.
        thread::sleep(Duration::from_secs(2));
        selector.deregister(read_fd).unwrap();
        // Wait for deregister to take effect before making the pipe readable, otherwise select may still return the event.
        thread::sleep(Duration::from_secs(2));

        // Make the pipe readable.
        let data = 1_u8;
        assert_eq!(unsafe { write(write_fd, &data as *const u8 as *const ffi::c_void, 1_usize) }, 1_isize);

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_deregister_writable_while_select() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        let selector_clone = selector.clone();
        selector.register(write_fd, IoId::new(id), IoEventInterest::WRITABLE).unwrap();

        // Make the pipe not writable.
        write_until_blocking(write_fd);

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert_eq!(
                selector_clone.select(&mut events, Some(Duration::from_secs(2))),
                Err(CommonErrors::Timeout)
            );
            events
        });

        begin_sync.wait(Duration::MAX);
        // Wait for the thread to block on poll. Obviously this isn't guaranteed to work, but I have no better idea.
        thread::sleep(Duration::from_secs(2));
        selector.deregister(write_fd).unwrap();
        // Wait for deregister to take effect before making the pipe writable, otherwise select may still return the event.
        thread::sleep(Duration::from_secs(2));

        // Make the pipe writable.
        read_until_blocking(read_fd);

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_reregister_readable() {
        let (read_fd, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);

        // Make the pipe readable.
        let data = 1_u8;
        assert_eq!(unsafe { write(write_fd, &data as *const u8 as *const ffi::c_void, 1_usize) }, 1_isize);

        // Register for readable events. This select should succeed.
        {
            let selector_clone = selector.clone();
            selector_clone.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();

            let (_, _, join_handle) = create_thread(move || {
                let mut events = Vec::<IoEvent>::new(8);
                assert!(selector_clone.select(&mut events, None).is_ok());
                events
            });

            let events = join_handle.join().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].id(), IoId::new(id));
            assert!(events[0].is_readable());
            assert!(!events[0].is_writable());
        }

        // The readable event should be reported only once. This select should time out.
        {
            let selector_clone = selector.clone();

            let (_, _, join_handle) = create_thread(move || {
                let mut events = Vec::<IoEvent>::new(8);
                assert_eq!(
                    selector_clone.select(&mut events, Some(Duration::from_secs(2))),
                    Err(CommonErrors::Timeout)
                );
                events
            });

            let events = join_handle.join().unwrap();
            assert_eq!(events.len(), 0);
        }

        // Re-register for readable events. This select should succeed.
        {
            let selector_clone = selector.clone();
            selector_clone.reregister(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();

            let (_, _, join_handle) = create_thread(move || {
                let mut events = Vec::<IoEvent>::new(8);
                assert!(selector_clone.select(&mut events, None).is_ok());
                events
            });

            let events = join_handle.join().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].id(), IoId::new(id));
            assert!(events[0].is_readable());
            assert!(!events[0].is_writable());
        }
    }

    #[test]
    fn test_reregister_writable() {
        let (_, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);

        // Register for writable events. This select should succeed.
        {
            let selector_clone = selector.clone();
            selector_clone.register(write_fd, IoId::new(id), IoEventInterest::WRITABLE).unwrap();

            let (_, _, join_handle) = create_thread(move || {
                let mut events = Vec::<IoEvent>::new(8);
                assert!(selector_clone.select(&mut events, None).is_ok());
                events
            });

            let events = join_handle.join().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].id(), IoId::new(id));
            assert!(events[0].is_writable());
            assert!(!events[0].is_readable());
        }

        // The writable event should be reported only once. This select should time out.
        {
            let selector_clone = selector.clone();

            let (_, _, join_handle) = create_thread(move || {
                let mut events = Vec::<IoEvent>::new(8);
                assert_eq!(
                    selector_clone.select(&mut events, Some(Duration::from_secs(2))),
                    Err(CommonErrors::Timeout)
                );
                events
            });

            let events = join_handle.join().unwrap();
            assert_eq!(events.len(), 0);
        }

        // Re-register for writable events. This select should succeed.
        {
            let selector_clone = selector.clone();
            selector_clone.reregister(write_fd, IoId::new(id), IoEventInterest::WRITABLE).unwrap();

            let (_, _, join_handle) = create_thread(move || {
                let mut events = Vec::<IoEvent>::new(8);
                assert!(selector_clone.select(&mut events, None).is_ok());
                events
            });

            let events = join_handle.join().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].id(), IoId::new(id));
            assert!(events[0].is_writable());
            assert!(!events[0].is_readable());
        }
    }

    #[test]
    fn test_readable_timeout() {
        let (read_fd, _) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        selector.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();

        let (_, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert_eq!(selector.select(&mut events, Some(Duration::from_secs(2))), Err(CommonErrors::Timeout));
            events
        });

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_writable_timeout() {
        let (_, write_fd) = create_pipe();
        let id = 1;
        let selector = Selector::new(8);
        selector.register(write_fd, IoId::new(id), IoEventInterest::WRITABLE).unwrap();

        write_until_blocking(write_fd);

        let (_, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert_eq!(selector.select(&mut events, Some(Duration::from_secs(2))), Err(CommonErrors::Timeout));
            events
        });

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn test_create_waker_before_select() {
        let (read_fd, _) = create_pipe();
        let id = 1;
        let waker_id = 2;
        let selector = Selector::new(8);
        selector.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();
        let waker = selector.create_waker(IoId::new(waker_id)).unwrap();

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert!(selector.select(&mut events, None).is_ok());
            events
        });

        begin_sync.wait(Duration::MAX);
        // Wait for the thread to block on poll. Obviously this isn't guaranteed to work, but I have no better idea.
        thread::sleep(Duration::from_secs(2));

        waker.wake();

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id(), IoId::new(waker_id));
    }

    #[test]
    fn test_create_waker_while_select() {
        let (read_fd, _) = create_pipe();
        let id = 1;
        let waker_id = 2;
        let selector = Selector::new(8);
        selector.register(read_fd, IoId::new(id), IoEventInterest::READABLE).unwrap();
        let selector_clone = selector.clone();

        let (begin_sync, _, join_handle) = create_thread(move || {
            let mut events = Vec::<IoEvent>::new(8);
            assert!(selector_clone.select(&mut events, None).is_ok());
            events
        });

        begin_sync.wait(Duration::MAX);
        // Wait for the thread to block on poll. Obviously this isn't guaranteed to work, but I have no better idea.
        thread::sleep(Duration::from_secs(2));

        let waker = selector.create_waker(IoId::new(waker_id)).unwrap();
        waker.wake();

        let events = join_handle.join().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id(), IoId::new(waker_id));
    }

    #[test]
    fn test_register_limit_reached() {
        let selector = Selector::new(2);
        assert!(selector.register(0, IoId::new(1), IoEventInterest::READABLE).is_ok());
        assert!(selector.register(1, IoId::new(2), IoEventInterest::READABLE).is_ok());
        assert_eq!(
            selector.register(2, IoId::new(3), IoEventInterest::READABLE),
            Err(CommonErrors::NoSpaceLeft)
        );
    }

    #[test]
    fn test_register_registered_fd() {
        let selector = Selector::new(2);
        assert!(selector.register(1, IoId::new(1), IoEventInterest::READABLE).is_ok());
        assert!(selector.register(1, IoId::new(2), IoEventInterest::READABLE).is_err());
        assert!(selector.register(1, IoId::new(2), IoEventInterest::WRITABLE).is_err());
    }

    #[test]
    fn test_deregister_not_registered_fd() {
        let selector = Selector::new(2);
        assert_eq!(selector.deregister(1), Err(CommonErrors::NotFound));
    }

    #[test]
    fn test_reregister_not_registered_fd() {
        let selector = Selector::new(2);
        assert_eq!(
            selector.reregister(1, IoId::new(1), IoEventInterest::READABLE),
            Err(CommonErrors::NotFound)
        );
    }
}
