//! Host-side helpers for driving async drivers in unit tests.
extern crate std;

use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, Waker};

/// Runs a future to completion. The mocks used in tests never return
/// `Pending`, so a single poll loop with a no-op waker is enough.
pub fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return out;
        }
    }
}
