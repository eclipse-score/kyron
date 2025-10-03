use std::mem::{size_of, MaybeUninit};

/// Get affinity of current thread.
///
/// # Safety
///
/// This function uses `unsafe` code internally to call system APIs.
///
/// # Panics
///
/// This function will panic if a system call fails.
pub fn current_thread_affinity() -> Vec<usize> {
    unsafe {
        let current_thread = 0;
        let mut cpu_set = MaybeUninit::<libc::cpu_set_t>::zeroed().assume_init();
        let cpu_set_size = size_of::<libc::cpu_set_t>();
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

        affinity
    }
}

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

/// Scheduling policy and priority parameters of a thread.
pub struct ThreadPriorityParams {
    /// Name of the scheduling policy ("other", "fifo", "round_robin").
    pub scheduler: String,
    /// Thread priority in `iceoryx2` range (0-255).
    pub priority: i32,
    /// Min native priority value.
    pub priority_min: i32,
    /// Max native priority value.
    pub priority_max: i32,
}

/// Get priority and scheduler parameters of current thread.
///
/// # Safety
///
/// This function uses `unsafe` code internally to call system APIs.
///
/// # Panics
///
/// This function will panic if a system call fails.
pub fn current_thread_priority_params() -> ThreadPriorityParams {
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

        ThreadPriorityParams {
            scheduler,
            priority,
            priority_min,
            priority_max,
        }
    }
}
