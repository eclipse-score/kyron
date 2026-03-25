// *******************************************************************************
// Copyright (c) 2026 Contributors to the Eclipse Foundation
//
// See the NOTICE file(s) distributed with this work for additional
// information regarding copyright ownership.
//
// This program and the accompanying materials are made available under the
// terms of the Apache License Version 2.0 which is available at
// <https://www.apache.org/licenses/LICENSE-2.0>
//
// SPDX-License-Identifier: Apache-2.0
// *******************************************************************************

use super::workers::worker_types::*;
use crate::common::types::BoxInternal;
use crate::common::types::UniqueWorkerId;
use crate::ctx_get_handler;
use crate::io::driver::IoDriverUnparker;
use crate::macros::log::*;
use crate::{scheduler::context::ctx_get_worker_id, TaskRef};
use ::core::ops::Deref;
use kyron_foundation::containers::trigger_queue::TriggerQueue;
use kyron_foundation::not_recoverable_error;
use kyron_foundation::{
    containers::{mpmc_queue::MpmcQueue, spmc_queue::BoundProducerConsumer, vector_extension::VectorExtension, *},
    prelude::FoundationAtomicU8,
};
use std::sync::Arc;

pub(crate) trait SchedulerTrait {
    ///
    /// Respawns task that was already once Polled and now is awaked.
    ///
    fn respawn(&self, task: TaskRef);

    ///
    /// Respawns task that was already once Polled and now is awaked into safety executor
    ///
    fn respawn_into_safety(&self, task: TaskRef);
}

pub(crate) const SCHEDULER_MAX_SEARCHING_WORKERS_DIVIDER: u8 = 2; // Tune point: We allow only half of worker to steal, to limit contention

pub(crate) struct AsyncScheduler {
    worker_access: BoxInternal<[WorkerInteractor]>,

    // Hot path for figuring out if we shall wake-up someone, or we shall go to sleep from worker
    pub(super) num_of_searching_workers: FoundationAtomicU8,

    pub(super) parked_workers_indexes: std::sync::Mutex<Vec<usize>>,

    pub(super) global_queue: MpmcQueue<TaskRef>,

    pub(super) safety_worker_queue: Option<Arc<TriggerQueue<TaskRef>>>,

    // We need to keep io driver here so scheduler used outside of runtime can still access it
    pub(super) io_unparker: IoDriverUnparker,
}

impl AsyncScheduler {
    pub(super) fn new(
        worker_access: BoxInternal<[WorkerInteractor]>,
        global_queue: MpmcQueue<TaskRef>,
        safety_worker_queue: Option<Arc<TriggerQueue<TaskRef>>>,
        io_unparker: IoDriverUnparker,
    ) -> Self {
        let workers_cnt = worker_access.len();

        AsyncScheduler {
            worker_access,
            num_of_searching_workers: FoundationAtomicU8::new(0),
            parked_workers_indexes: std::sync::Mutex::new(Vec::new_in_global(workers_cnt)),
            global_queue,
            safety_worker_queue,
            io_unparker,
        }
    }

    pub(super) fn spawn_from_runtime(&self, task: TaskRef, local_queue: &BoundProducerConsumer<TaskRef>) {
        match local_queue.push(task, &self.global_queue) {
            Ok(_) => {
                self.try_notify_siblings_workers(Some(ctx_get_worker_id()));
            },
            Err(_) => {
                // TODO: Add error hooks so we can notify app owner that we are done
                panic!("Cannot push to queue anymore, overflow!");
            },
        }
    }

    pub(super) fn spawn_outside_runtime(&self, task: TaskRef) {
        if self.global_queue.push(task) {
            self.try_notify_siblings_worker_unconditional(None);
        } else {
            // TODO: Add error hooks so we can notify app owner that we are done
            panic!("Cannot push to global queue anymore, overflow!");
        }
    }

    pub(super) fn get_worker_access(&self, id: WorkerId) -> &WorkerInteractor {
        &self.worker_access[id.worker_id() as usize]
    }

    pub(super) fn as_worker_access_slice(&self) -> &[WorkerInteractor] {
        &self.worker_access
    }
}

impl SchedulerTrait for AsyncScheduler {
    fn respawn(&self, task: TaskRef) {
        if let Some(handler) = ctx_get_handler() {
            // This flow may seems confusing but it tries to decouple info between objects (it's not ideal but for now done like that):
            // - we come here from AsyncTask::schedule call
            // - if we are here, means we are somewhere in runtime, let's try to get required info to move on
            handler.try_with_local_producer(|pc| {
                if let Some(pc) = pc {
                    self.spawn_from_runtime(task, pc);
                } else {
                    self.spawn_outside_runtime(task);
                }
            });
        } else {
            self.spawn_outside_runtime(task);
        }
    }

    // Used from async task to bring back task to work queue
    fn respawn_into_safety(&self, task: TaskRef) {
        if let Some(ref safety) = self.safety_worker_queue {
            let ret = safety.push(task);

            // TODO: For now simply abort, we can consider blocking push here, or polling push
            not_recoverable_error!(on_cond(ret), "Cannot push to safety queue anymore, overflow!");
        }
    }
}

impl AsyncScheduler {
    ///
    /// Tries to move worker to searching state if conditions are met. No more than half of workers shall be in searching state to avoid too much contention on stealing queue
    ///
    pub(super) fn try_transition_worker_to_searching(&self) -> bool {
        let searching = self
            .num_of_searching_workers
            .load(::core::sync::atomic::Ordering::SeqCst);
        let predicted = (searching * SCHEDULER_MAX_SEARCHING_WORKERS_DIVIDER) as usize;

        if predicted >= self.worker_access.len() {
            return false;
        }

        self.num_of_searching_workers
            .fetch_add(1, ::core::sync::atomic::Ordering::SeqCst);
        true
    }

    pub(super) fn transition_worker_to_executing(&self) {
        self.num_of_searching_workers
            .fetch_sub(1, ::core::sync::atomic::Ordering::SeqCst);
    }

    pub(super) fn transition_to_parked(&self, was_searching: bool, id: WorkerId) -> bool {
        let mut guard = self.parked_workers_indexes.lock().unwrap();

        let mut num_of_searching = 2; //2 as false condition

        if was_searching {
            num_of_searching = self
                .num_of_searching_workers
                .fetch_sub(1, ::core::sync::atomic::Ordering::SeqCst);
        }

        guard.push(id.worker_id() as usize).expect("Failed to push worker id"); // worker_id is index in worker_access
        num_of_searching == 1
    }

    pub(super) fn transition_from_parked(&self, id: WorkerId) {
        let mut guard = self.parked_workers_indexes.lock().unwrap();
        // Find and remove the worker from the list. The order of the parked workers is not important after removal.
        if let Some(pos) = guard.iter().position(|x| *x == id.worker_id() as usize) {
            guard.swap_remove(pos);
        }
    }

    //
    // Private
    //

    pub(crate) fn try_notify_siblings_workers(&self, current_worker: Option<WorkerId>) {
        if !self.should_notify_some_worker() {
            trace!("Too many searchers while scheduling, no one will be woken up!");
            return; // Too much searchers already, let them find a job
        }

        self.try_notify_siblings_worker_unconditional(current_worker)
    }

    fn try_notify_siblings_worker_unconditional(&self, current: Option<WorkerId>) {
        let mut index_opt = None;

        // Keep section short not overlapping with worker access in unpark which can also take another mutex
        {
            let mut guard = self.parked_workers_indexes.lock().unwrap();
            // Pop the worker index so that another worker can be notified next time (instead of the same one)
            let removal = guard
                .iter()
                .position(|e| current.is_none() || *e != current.unwrap().worker_id() as usize);

            if let Some(rindex) = removal {
                index_opt = guard.swap_remove(rindex);
            }
        }

        if let Some(index) = index_opt {
            trace!("Notifying worker at index {} to wakeup", index);
            self.worker_access[index].unpark(&self.io_unparker);
        } else {
            // No one is sleeping but no one is searching, means they are before sleep.
            // For now we simply notify all (as we only use syscall once someone really sleeps)
            for w in &self.worker_access {
                w.unpark(&self.io_unparker);
            }
        }
    }

    //
    // A worker should be notified only if no other workers are already in the searching state.
    //
    fn should_notify_some_worker(&self) -> bool {
        self.num_of_searching_workers
            .fetch_sub(0, ::core::sync::atomic::Ordering::SeqCst)
            == 0
    }
}

///
/// Scheduler that is able to spawn a `task` into a dedicated worker
///
pub(crate) struct DedicatedScheduler {
    pub(super) dedicated_queues: BoxInternal<[(WorkerId, Arc<TriggerQueue<TaskRef>>)]>, // TODO: consider HashMap, but now iterating over 2-10 items should not be a big deal.
}

impl DedicatedScheduler {
    pub(crate) fn spawn(&self, task: TaskRef, worker_id: UniqueWorkerId) -> bool {
        self.with_queue(worker_id, |queue| queue.push(task)) //TODO: Error here means there is no more space -> miss-config, this is fatal error that cannot be handled by user.
    }

    fn with_queue<U: FnOnce(&TriggerQueue<TaskRef>) -> bool>(&self, worker_id: UniqueWorkerId, c: U) -> bool {
        let x = self.dedicated_queues.iter().find(|(id, _)| id.unique_id() == worker_id);
        if x.is_none() {
            return false;
        }

        c(&(x.unwrap().1))
    }
}

///
/// This is small proxy scheduler used in `AsyncTask` that keeps a track of `worker_id` on which the `task` has been spawn. This let us
/// respawn a task correctly into the same worker.
///
///
pub(super) struct DedicatedSchedulerLocal(DedicatedSchedulerLocalInner);

impl DedicatedSchedulerLocal {
    pub(super) fn new(worker_id: UniqueWorkerId, scheduler: Arc<DedicatedScheduler>) -> Self {
        Self(DedicatedSchedulerLocalInner { worker_id, scheduler })
    }
}

impl Deref for DedicatedSchedulerLocal {
    type Target = DedicatedSchedulerLocalInner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

///
/// Real impl for `DedicatedSchedulerLocal` to facilitate ability to use Deref in `AsyncTask` as common access to all scheduler types.
///
pub(super) struct DedicatedSchedulerLocalInner {
    worker_id: UniqueWorkerId,
    scheduler: Arc<DedicatedScheduler>,
}

impl SchedulerTrait for DedicatedSchedulerLocalInner {
    fn respawn(&self, task: TaskRef) {
        self.scheduler.spawn(task, self.worker_id);
    }

    fn respawn_into_safety(&self, _: TaskRef) {
        not_recoverable_error!("Respawning into safety for a task that was created in dedicated worker is not allowed");
    }
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn scheduler_new(
    workers_cnt: usize,
    local_queue_size: u32,
    drivers: &super::driver::Drivers,
) -> AsyncScheduler {
    // artificially construct a scheduler

    let mut worker_interactors = BoxInternal::<[WorkerInteractor]>::new_uninit_slice(workers_cnt);
    let mut queues: Vec<TaskStealQueue> = Vec::new_in_global(workers_cnt);

    for i in 0..workers_cnt {
        let c = create_steal_queue(local_queue_size);

        queues.push(c.clone()).unwrap();

        worker_interactors[i].write(WorkerInteractor::new(
            c,
            WorkerId::new(format!("{}", i).into(), 0, i as u8, WorkerType::Async),
        ));
    }

    let safety_worker_queue = Some(Arc::new(TriggerQueue::new(64)));

    let global_queue = MpmcQueue::new(32);

    AsyncScheduler::new(
        unsafe { worker_interactors.assume_init() },
        global_queue,
        safety_worker_queue,
        drivers.get_io_driver().get_unparker(),
    )
}

#[cfg(test)]
#[cfg(not(loom))]
#[allow(unused_imports)]
mod tests {
    use crate::scheduler::driver::Drivers;

    use super::*;

    #[test]
    #[cfg(not(miri))] // Provenance issues
    fn should_transition_to_searching_test() {
        let drivers = Drivers::new();
        // scheduler with one worker and a queue size of 2
        let scheduler = scheduler_new(1, 2, &drivers);
        // if no worker is in searching state one worker should steal work
        assert!(scheduler.try_transition_worker_to_searching());
        // if our one and only worker is in searching state no other worker should steal work
        // transition from searching to searching should fail
        assert!(!scheduler.try_transition_worker_to_searching());

        // scheduler with two workers and a queue size of 2
        let scheduler = scheduler_new(2, 2, &drivers);

        assert!(scheduler.should_notify_some_worker());
        // if no worker is in searching state one worker should steal work
        assert!(scheduler.try_transition_worker_to_searching());

        assert!(!scheduler.should_notify_some_worker());
        // if one worker is in searching state, half workers are searching, so the other one should
        // not transition to searching also

        // scheduler with 10 workers and a queue size of 2
        let scheduler = scheduler_new(10, 2, &drivers);
        // 0 searching
        assert!(scheduler.should_notify_some_worker());
        assert!(scheduler.try_transition_worker_to_searching());
        // 1 searching
        assert!(!scheduler.should_notify_some_worker());
        assert!(scheduler.try_transition_worker_to_searching());
        // 2 searching
        assert!(!scheduler.should_notify_some_worker());
        assert!(scheduler.try_transition_worker_to_searching());
        // 3 searching
        assert!(!scheduler.should_notify_some_worker());
        assert!(scheduler.try_transition_worker_to_searching());
        // 4 searching
        assert!(!scheduler.should_notify_some_worker());
        assert!(scheduler.try_transition_worker_to_searching());
        // 5 searching
        assert!(!scheduler.should_notify_some_worker());
        assert!(!scheduler.try_transition_worker_to_searching());
    }
}
