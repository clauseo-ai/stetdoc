//! A way for a test to make the transaction of an edit wrong in a way the operations never do, so that the two nets under the
//! compiler can be shown to hold: `check::edit` (a violation that the edit adds) and the second pass of the scope guard (a block
//! that changed and that no operation declared). A refusal of either is a bug in stetdoc, and a bug is not something an agent
//! can provoke, so the test arms a change that `edit` applies to the staged transaction right after the operations, on this thread.

use crate::state::Tx;
use std::cell::RefCell;

type Tamper = Box<dyn for<'a, 's> FnOnce(&'a mut Tx<'s>)>;

thread_local! {
    static ARMED: RefCell<Option<Tamper>> = const { RefCell::new(None) };
}

/// Arms a change to the next transaction that `edit` stages on this thread. It is used once.
pub(super) fn arm(change: impl for<'a, 's> FnOnce(&'a mut Tx<'s>) + 'static) {
    ARMED.with(|a| *a.borrow_mut() = Some(Box::new(change)));
}

/// What `edit` calls after the operations: applies the armed change, if there is one.
pub(super) fn tamper(tx: &mut Tx<'_>) {
    if let Some(change) = ARMED.with(|a| a.borrow_mut().take()) {
        change(tx);
    }
}
