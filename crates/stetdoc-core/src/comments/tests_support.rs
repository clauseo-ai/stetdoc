//! A transaction and a compiler over a package, for the tests of the operations.

use crate::revisions::compiler::Compiler;
use crate::revisions::stamp::{RevStamp, Track};
use crate::state::{State, Tx};
use crate::wordml::{BlockKey, BlockKind};

/// The stamp of the agent: Clauseo, at a fixed time.
pub(super) fn stamp() -> RevStamp {
    RevStamp::new("Clauseo", "2026-10-06T10:00:00Z").unwrap_or_else(|e| panic!("{e}"))
}

/// A stamp for another author.
pub(super) fn stamp_of(author: &str) -> RevStamp {
    RevStamp::new(author, "2026-10-07T11:00:00Z").unwrap_or_else(|e| panic!("{e}"))
}

/// The key of the `n`th paragraph (from 0) of the body.
pub(super) fn para(state: &State, n: usize) -> BlockKey {
    let view = state.parts();
    let main = view
        .story_specs()
        .unwrap()
        .0
        .first()
        .map(|s| s.part.clone())
        .unwrap();
    let story = view.story(view.id(&main).unwrap()).unwrap();
    let entry = story
        .blocks()
        .iter()
        .filter(|e| e.kind == BlockKind::Paragraph)
        .nth(n)
        .unwrap();
    story.key(entry)
}

/// A compiler over `tx`, tracked as the stamp's author, as the operations will use it.
pub(super) fn compiler<'a, 's>(tx: &'a mut Tx<'s>, who: &RevStamp) -> Compiler<'a, 's> {
    Compiler::new(tx, Track::Tracked(who.clone())).unwrap_or_else(|e| panic!("{e}"))
}

/// The staged text of a part.
pub(super) fn text(tx: &Tx<'_>, part: &str) -> String {
    let view = tx.parts();
    let id = view.id(part).unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

/// Runs `f` on another thread and fails if it does not finish in 60 seconds: a quadratic walk over a long
/// thread is a hang, and a test that hangs says nothing.
pub(super) fn finishes<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(60))
        .unwrap_or_else(|_| panic!("the call did not finish in 60 seconds"))
}
