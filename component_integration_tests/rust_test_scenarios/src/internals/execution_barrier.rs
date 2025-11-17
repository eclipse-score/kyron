use kyron::JoinHandle;
use kyron_foundation::threading::thread_wait_barrier::{ThreadReadyNotifier, ThreadWaitBarrier};

use futures::future;
use std::sync::Arc;
use std::time::Duration;
use tracing::trace;

pub struct RuntimeJoiner<T>
where
    T: Send + 'static,
{
    handles: Vec<JoinHandle<T>>,
}

impl<T> RuntimeJoiner<T>
where
    T: Send + 'static,
{
    pub fn new() -> Self {
        RuntimeJoiner { handles: Vec::new() }
    }

    pub fn add_handle(&mut self, handle: JoinHandle<T>) {
        self.handles.push(handle);
    }

    pub async fn wait_for_all(self) -> u32 {
        future::join_all(self.handles).await;
        0
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

    pub fn get_notifiers(&self) -> Vec<ThreadReadyNotifier> {
        let mut notifiers = Vec::new();
        while let Some(notifier) = self.barrier.get_notifier() {
            notifiers.push(notifier);
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
