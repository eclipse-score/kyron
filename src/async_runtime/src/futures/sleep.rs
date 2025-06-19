use crate::{scheduler::driver::Drivers, time::clock::*};
use std::{future::Future, task::Waker, time::Duration};

use foundation::{not_recoverable_error, prelude::error, prelude::CommonErrors};

use crate::{
    futures::{FutureInternalReturn, FutureState},
    scheduler::context::ctx_get_drivers,
};

///
/// Waits until duration has elapsed. No work is performed while awaiting on the sleep future to complete.
/// Sleep operates at millisecond granularity and should not be used for tasks that require high-resolution timers.
/// > ATTENTION: Time is counted from the moment the sleep function is called, not from the moment the future is awaited.
///
#[allow(private_interfaces)]
pub fn sleep(duration: std::time::Duration) -> Sleep<impl TimerAccessor> {
    Sleep::new(duration, Clock::now(), ctx_get_drivers())
}

#[allow(private_bounds)]
pub struct Sleep<T: TimerAccessor> {
    state: FutureState,
    expire_at: Instant,
    drivers: T,
}

impl<T: TimerAccessor> Unpin for Sleep<T> {}

// Trait used for testing purpose only
pub trait TimerAccessor {
    fn register(&self, expire_at: Instant, waker: Waker) -> Result<(), CommonErrors>;
}

impl TimerAccessor for Drivers {
    fn register(&self, expire_at: Instant, waker: Waker) -> Result<(), CommonErrors> {
        self.register_timeout(expire_at, waker)
    }
}

impl<T: TimerAccessor> Sleep<T> {
    /// Creates a new Sleep future that will complete after the specified duration.
    fn new(duration: std::time::Duration, now: Instant, drivers: T) -> Self {
        Self {
            state: FutureState::New,
            expire_at: now + duration,
            drivers,
        }
    }

    fn rounded_deadline(&self) -> Instant {
        // Round the deadline to the nearest millisecond
        self.expire_at + Duration::from_nanos(999999)
    }
}

impl<T: TimerAccessor> Future for Sleep<T> {
    type Output = ();

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let res: FutureInternalReturn<()> = match self.state {
            // We need to round deadline to the nearest millisecond (round up always) to ensure that we are not awaken before the actual deadline.
            FutureState::New => match self.drivers.register(self.rounded_deadline(), cx.waker().clone()) {
                Ok(_) => FutureInternalReturn::polled(),
                Err(CommonErrors::AlreadyDone) => FutureInternalReturn::ready(()),
                Err(e) => not_recoverable_error!(with e, "Failed to register timeout for sleep future"),
            },
            FutureState::Polled => {
                let now = Clock::now();

                if now >= self.expire_at {
                    FutureInternalReturn::ready(())
                } else {
                    FutureInternalReturn::polled()
                }
            }
            FutureState::Finished => not_recoverable_error!("Cannot be here, future is already finished"),
        };

        self.state.assign_and_propagate(res)
    }
}

#[cfg(test)]
#[cfg(not(loom))]
mod tests {
    use std::cell::RefCell;

    use super::*;

    use testing::{
        assert_poll_ready,
        poller::TestingFuturePoller,
        prelude::{CallableTrait, MockFn, MockFnBuilder},
    };

    pub struct TimeDriverMock {
        mock: RefCell<MockFn<Result<(), CommonErrors>>>,
    }

    impl TimerAccessor for TimeDriverMock {
        fn register(&self, _expire_at: Instant, _waker: Waker) -> Result<(), CommonErrors> {
            self.mock.borrow_mut().call()
        }
    }

    #[test]
    fn when_sleep_is_awaited_on_already_timeouted_time_its_ready_right_away() {
        let drv = TimeDriverMock {
            mock: RefCell::new(MockFnBuilder::new_with_default(Err(CommonErrors::AlreadyDone)).times(1).build()),
        };
        let sleep_future = Sleep::new(Duration::from_millis(20), Clock::now(), drv);

        let mut poller = TestingFuturePoller::new(sleep_future);

        assert_poll_ready(poller.poll(), ());
    }

    #[test]
    fn when_sleep_is_registered_ok_wait_should_be_ready_after_time() {
        let drv = TimeDriverMock {
            mock: RefCell::new(MockFnBuilder::new_with_default(Ok(())).times(1).build()),
        };
        let sleep_future = Sleep::new(Duration::from_millis(20), Clock::now(), drv);

        let mut poller = TestingFuturePoller::new(sleep_future);

        assert!(poller.poll().is_pending());

        std::thread::sleep(Duration::from_millis(20));

        assert_poll_ready(poller.poll(), ());
    }

    #[test]
    fn when_polling_not_ready_shall_be_pending() {
        let drv = TimeDriverMock {
            mock: RefCell::new(MockFnBuilder::new_with_default(Ok(())).times(1).build()),
        };
        let sleep_future = Sleep::new(Duration::from_millis(100), Clock::now(), drv);

        let mut poller = TestingFuturePoller::new(sleep_future);

        assert!(poller.poll().is_pending());
        std::thread::sleep(Duration::from_millis(20));
        assert!(poller.poll().is_pending());
    }

    #[test]
    #[should_panic(expected = "Cannot be here, future is already finished")]
    fn polling_ready_panics() {
        let drv = TimeDriverMock {
            mock: RefCell::new(MockFnBuilder::new_with_default(Ok(())).times(1).build()),
        };
        let sleep_future = Sleep::new(Duration::from_millis(20), Clock::now(), drv);

        let mut poller = TestingFuturePoller::new(sleep_future);

        assert!(poller.poll().is_pending());

        std::thread::sleep(Duration::from_millis(20));

        assert_poll_ready(poller.poll(), ());
        let _ = poller.poll();
    }
}
