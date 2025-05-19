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

use std::{future::Future, sync::Arc, task::Waker, time::Duration};

use foundation::prelude::FoundationAtomicU16;

use crate::{
    core::types::{box_future, ArcInternal},
    scheduler::{scheduler_mt::SchedulerTrait, waker::create_waker},
    AsyncTask, TaskRef,
};

#[derive(Default)]
pub struct SchedulerSyncMock {
    mtx: std::sync::Mutex<bool>,
    cv: std::sync::Condvar,
}

impl SchedulerSyncMock {
    #[allow(dead_code)]
    pub fn wait_for_wake(&self) -> bool {
        let mut u = self
            .cv
            .wait_timeout_while(self.mtx.lock().unwrap(), Duration::from_millis(10), |g| !*g)
            .unwrap();

        if u.1.timed_out() {
            false
        } else {
            *u.0 = false;
            true
        }
    }
}

impl SchedulerTrait for SchedulerSyncMock {
    fn respawn(&self, _: TaskRef) {
        // eprintln!("WAKING FROM WAKER ?!");
        *self.mtx.lock().unwrap() = true;
        self.cv.notify_one();
    }

    fn respawn_into_safety(&self, _task: TaskRef) {
        todo!()
    }
}

#[derive(Default)]
pub struct SchedulerMock {
    pub spawn_count: FoundationAtomicU16,
    pub safety_spawn_count: FoundationAtomicU16,
}

impl SchedulerMock {
    pub fn safety_spawn_count(&self) -> u16 {
        self.safety_spawn_count.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn spawn_count(&self) -> u16 {
        self.spawn_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl SchedulerTrait for SchedulerMock {
    fn respawn(&self, _: TaskRef) {
        self.spawn_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn respawn_into_safety(&self, _: TaskRef) {
        self.safety_spawn_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

//Creators

pub fn create_mock_scheduler() -> ArcInternal<SchedulerMock> {
    ArcInternal::new(SchedulerMock::default())
}

#[allow(dead_code)]
pub fn create_mock_scheduler_sync() -> ArcInternal<SchedulerSyncMock> {
    ArcInternal::new(SchedulerSyncMock::default())
}

// Dummy stub functions
pub async fn test_function<T: Default>() -> T {
    T::default()
}

pub async fn test_function_ret<T>(ret: T) -> T {
    ret
}

pub fn get_dummy_task_waker() -> (
    Waker,
    Arc<AsyncTask<(), Box<dyn Future<Output = ()> + Send + 'static>, Arc<SchedulerMock>>>,
) {
    let task = Arc::new(AsyncTask::new(box_future(async {}), 0, create_mock_scheduler()));

    (create_waker(TaskRef::new(task.clone())), task)
}

pub fn get_waker_from_task(task: &Arc<AsyncTask<(), Box<dyn Future<Output = ()> + Send + 'static>, Arc<SchedulerMock>>>) -> Waker {
    create_waker(TaskRef::new(task.clone()))
}

#[allow(dead_code)]
pub fn get_dummy_sync_task_waker(sched: Arc<SchedulerSyncMock>) -> Waker {
    let task = Arc::new(AsyncTask::new(box_future(async {}), 0, sched));

    create_waker(TaskRef::new(task.clone()))
}
