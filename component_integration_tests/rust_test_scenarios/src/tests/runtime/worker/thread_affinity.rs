use crate::internals::execution_barrier::MultiExecutionBarrier;
use crate::internals::{execution_barrier::RuntimeJoiner, runtime_helper::Runtime};
use async_runtime::spawn;
use foundation::threading::thread_wait_barrier::ThreadReadyNotifier;
use std::mem::MaybeUninit;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use test_scenarios_rust::scenario::Scenario;
use tracing::info;

fn show_thread_affinity(id: usize) {
    unsafe {
        let current_thread = 0;
        let mut cpu_set = MaybeUninit::<libc::cpu_set_t>::zeroed().assume_init();
        let cpu_set_size = std::mem::size_of::<libc::cpu_set_t>();
        let rc = libc::sched_getaffinity(current_thread, cpu_set_size, &mut cpu_set);
        if rc != 0 {
            let errno = *libc::__errno_location();
            panic!("libc::sched_getaffinity failed, rc: {rc}, errno: {errno}");
        }

        let mut affinity = Vec::new();
        for i in 0..libc::CPU_SETSIZE as usize {
            if libc::CPU_ISSET(i, &cpu_set) {
                affinity.push(i);
            }
        }

        let id = format!("worker_{id}");
        let affinity = format!("{affinity:?}");

        info!(id, affinity);
    }
}

async fn blocking_task(id: usize, block_condition: Arc<(Condvar, Mutex<bool>)>, notifier: ThreadReadyNotifier) {
    let (cv, mtx) = &*block_condition;
    let mut block = mtx.lock().expect("Unable to lock mutex");

    show_thread_affinity(id);

    // Notify task done.
    notifier.ready();

    // Block until allowed to finish.
    while *block {
        block = cv.wait(block).expect("Unable to wait - poisoned mutex?");
    }
}

pub struct ThreadAffinity;

impl Scenario for ThreadAffinity {
    fn name(&self) -> &str {
        "thread_affinity"
    }

    fn run(&self, input: &str) -> Result<(), String> {
        let builder = Runtime::from_json(input)?;
        let exec_engine = builder.exec_engines().first().expect("No execution engine configuration found");
        let num_workers = exec_engine.workers;
        let mut rt = builder.build();

        let _ = rt.block_on(async move {
            let mut joiner = RuntimeJoiner::new();
            let mid_barrier = MultiExecutionBarrier::new(num_workers - 1);
            let mut mid_notifiers = mid_barrier.get_notifiers();

            // Condition variable and mutex containing `block` variable.
            let block_condition = Arc::new((Condvar::new(), Mutex::new(true)));

            // Show parameters of current thread.
            show_thread_affinity(0);

            // Spawn tasks for other threads.
            for id in 1..num_workers {
                let notifier = mid_notifiers.pop().expect("Failed to pop notifier");
                joiner.add_handle(spawn(blocking_task(id, block_condition.clone(), notifier)));
            }

            // Wait until all tasks shown params.
            let wait_s = 1;
            let result = mid_barrier.wait_for_notification(Duration::from_secs(wait_s));

            // Allow tasks to finish.
            {
                let (cv, mtx) = &*block_condition;
                let mut block = mtx.lock().expect("Unable to lock mutex");
                *block = false;
                cv.notify_all();
            }

            result.expect("Failed to join tasks in given time");

            Ok(joiner.wait_for_all().await)
        });

        Ok(())
    }
}
