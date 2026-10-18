//! Fault injection for the atomicity tests.
//!
//! Every step of a transaction that can fail calls `here` with its name. In a release build it does
//! nothing. In a test, a thread can arm it to fail at the n-th call, which lets the tests run a
//! scripted edit once for each step and check that the base state is untouched whichever step fails.

#[cfg(not(test))]
#[inline(always)]
pub(super) fn here(_step: &'static str) -> Result<(), crate::api::error::StetError> {
    Ok(())
}

#[cfg(test)]
pub(super) use armed::{arm, disarm, here};

#[cfg(test)]
mod armed {
    use crate::api::error::StetError;
    use std::cell::{Cell, RefCell};

    thread_local! {
        static COUNTDOWN: Cell<Option<usize>> = const { Cell::new(None) };
        static SEEN: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    /// Makes the call after `skip` more calls fail, on this thread.
    pub(in crate::state) fn arm(skip: usize) {
        COUNTDOWN.with(|c| c.set(Some(skip)));
        SEEN.with(|s| s.borrow_mut().clear());
    }

    /// Stops injecting, and returns the steps that were reached since `arm`, the failing one last.
    pub(in crate::state) fn disarm() -> Vec<&'static str> {
        COUNTDOWN.with(|c| c.set(None));
        SEEN.with(|s| std::mem::take(&mut *s.borrow_mut()))
    }

    pub(in crate::state) fn here(step: &'static str) -> Result<(), StetError> {
        let fail = COUNTDOWN.with(|c| match c.get() {
            None => false,
            Some(0) => {
                c.set(None);
                true
            }
            Some(n) => {
                c.set(Some(n - 1));
                false
            }
        });
        SEEN.with(|s| s.borrow_mut().push(step));
        if fail {
            Err(StetError::new(
                crate::api::error::ErrorCode::Internal,
                format!("internal: an injected failure at {step}"),
            ))
        } else {
            Ok(())
        }
    }
}
