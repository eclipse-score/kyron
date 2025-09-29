use crate::internals::execution_barrier::MultiExecutionBarrier;
use crate::internals::{execution_barrier::RuntimeJoiner, runtime_helper::Runtime};
use async_runtime::spawn;
use foundation::threading::thread_wait_barrier::ThreadReadyNotifier;
use std::mem::MaybeUninit;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use test_scenarios_rust::scenario::Scenario;
use tracing::info;

/// Return policy name from given policy identifier.
fn policy_to_string(policy: i32) -> String {
    match policy {
        0 => "other",
        1 => "fifo",
        2 => "round_robin",
        _ => panic!("Unknown scheduler type"),
    }
    .to_string()
}

/// Recalculate native->`iceoryx2` priority value.
/// `iceoryx2` uses <0;255> priority range.
/// It is then recalculated into scheduler-specific value.
fn priority_to_iceoryx2(native_prio: i32, native_min: i32, native_max: i32) -> i32 {
    let iceoryx2_max = u8::MAX;
    let range = native_max - native_min;
    let priority_iceoryx2 = (iceoryx2_max as f64 * (native_prio - native_min) as f64) / range as f64;
    priority_iceoryx2.round() as i32
}

fn show_thread_params(id: usize) {
    unsafe {
        let thread = libc::pthread_self();
        let mut policy: libc::c_int = -1;
        let mut param = MaybeUninit::<libc::sched_param>::zeroed().assume_init();
        let rc = libc::pthread_getschedparam(thread, &mut policy as *mut libc::c_int, &mut param as *mut libc::sched_param);
        if rc != 0 {
            let errno = *libc::__errno_location();
            panic!("libc::pthread_getschedparam failed, rc: {rc}, errno: {errno}");
        }

        let scheduler = policy_to_string(policy);
        let priority_native = param.sched_priority;
        let priority_min = libc::sched_get_priority_min(policy);
        let priority_max = libc::sched_get_priority_max(policy);

        // Recalculate native->`iceoryx2` priority value.
        let priority = priority_to_iceoryx2(priority_native, priority_min, priority_max);
        let id = format!("worker_{id}");
        info!(id, scheduler, priority, priority_min, priority_max);
    }
}

async fn blocking_task(id: usize, block_condition: Arc<(Condvar, Mutex<bool>)>, notifier: ThreadReadyNotifier) {
    let (cv, mtx) = &*block_condition;
    let mut block = mtx.lock().expect("Unable to lock mutex");

    show_thread_params(id);

    // Notify task done.
    notifier.ready();

    // Block until allowed to finish.
    while *block {
        block = cv.wait(block).expect("Unable to wait - poisoned mutex?");
    }
}

pub struct ThreadPriority;

impl Scenario for ThreadPriority {
    fn name(&self) -> &'static str {
        "thread_priority"
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
            show_thread_params(0);

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
