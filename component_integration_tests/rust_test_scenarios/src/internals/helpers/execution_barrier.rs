use async_runtime::scheduler::join_handle::JoinHandle;
use foundation::threading::thread_wait_barrier::{ThreadReadyNotifier, ThreadWaitBarrier};

use std::sync::Arc;
use std::time::Duration;
use tracing::trace;

pub struct ExecutionNotifier {
    notifier: ThreadReadyNotifier,
    handles: Vec<JoinHandle<()>>,
}

impl ExecutionNotifier {
    pub fn new(notifier: ThreadReadyNotifier) -> Self {
        ExecutionNotifier { notifier, handles: vec![] }
    }

    pub fn add_handle(&mut self, handle: JoinHandle<()>) {
        self.handles.push(handle);
    }

    pub async fn wait_and_notify(self) {
        for handle in self.handles {
            let _ = handle.await;
        }
        self.notifier.ready();
    }

    pub fn notify(self) {
        self.notifier.ready();
    }
}

pub struct ExecutionBarrier {
    barrier: Arc<ThreadWaitBarrier>,
}

impl ExecutionBarrier {
    pub fn new() -> Self {
        ExecutionBarrier {
            barrier: Arc::new(ThreadWaitBarrier::new(1)),
        }
    }

    pub fn get_notifier(&self) -> ExecutionNotifier {
        ExecutionNotifier::new(self.barrier.get_notifier().unwrap())
    }

    pub fn wait_for_notification(self, duration: Duration) -> Result<(), String> {
        trace!("ExecutionBarrier::wait_for_notification waits...");
        let res = self.barrier.wait_for_all(duration);
        trace!("ExecutionBarrier::wait_for_notification finished!");

        match res {
            Ok(_) => Ok(()),
            Err(_) => Err(format!("Failed to join tasks after {} seconds", duration.as_secs())),
        }
    }
}

pub struct MultiExecutionBarrier {
    barrier: Arc<ThreadWaitBarrier>,
}

impl MultiExecutionBarrier {
    pub fn new(capacity: usize) -> Self {
        MultiExecutionBarrier {
            barrier: Arc::new(ThreadWaitBarrier::new(capacity as u32)),
        }
    }

    pub fn get_notifiers(&self) -> Vec<ExecutionNotifier> {
        let mut notifiers = Vec::new();
        loop {
            if let Some(notifier) = self.barrier.get_notifier() {
                notifiers.push(ExecutionNotifier::new(notifier));
            } else {
                break;
            }
        }
        notifiers
    }

    pub fn wait_for_notification(self, duration: Duration) -> Result<(), String> {
        trace!("MultiExecutionBarrier::wait_for_notification waits...");
        let res = self.barrier.wait_for_all(duration);
        match res {
            Ok(_) => Ok(()),
            Err(_) => Err(format!("Failed to join tasks after {} seconds", duration.as_secs())),
        }
    }
}
