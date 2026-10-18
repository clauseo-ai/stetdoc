//! The laws of the state, over the fetched corpus.
//!
//! The corpus is read through the testkit from `STETDOC_CORPUS_DIR`, and the test skips when the
//! variable is unset: `cargo xtask check` always sets it. For every document that must open:
//! 1. `State::open` succeeds, and the unedited `save` equals the input (law L1).
//! 2. The live refs are exactly the blocks of the stories, one to one, and every ref resolves to
//!    its element, with the letter of its kind.
//! 3. Two opens of the same bytes give the same token and the same ref map.
//! 4. `export` then `resume` gives the same token and the same ref map, and saving the resumed
//!    state gives the exported head.
//! 5. A ref map with one byte changed is refused, or is still the exact form `persist` writes.
//! 6. A transaction that stages every part and is discarded, and one that edits the body and is
//!    discarded, leave `save` and `export` as they were.
//!
//! A document that must be refused at `open` or at `document` is refused by `State::open` with the
//! code the index says.

use super::tests::*;
use super::*;
use crate::api::error::ErrorCode;
use crate::refs::{Claim, RefKind, RefTable};
use stetdoc_testkit::corpus::Corpus;
use stetdoc_testkit::expect::{Expect, Stage};

/// Documents that the container and XML layers accept (the manifest says `ok`) and that are not
/// Word documents: the main part is not a `w:document`, so `State::open` refuses them with
/// `malformed_input`. The manifest should record them as refused at the `document` stage.
const NOT_WORD: &[&str] = &["poi:test-data/integration/stress025.docx"];

fn recorded(e: &Exported) -> Recorded {
    Recorded {
        base: e.base,
        head_sha256: e.head_sha256,
        refs_sha256: e.refs_sha256,
    }
}

/// Checks the laws for one document. The error says which law broke.
fn laws(bytes: &[u8]) -> Result<(), String> {
    let limits = Limits::default();
    let s = State::open(bytes.to_vec(), &limits).map_err(|e| format!("open: {e}"))?;
    let saved = s
        .to_package()
        .and_then(|p| p.write())
        .map_err(|e| format!("save: {e}"))?;
    if saved != bytes {
        return Err("L1: the unedited save differs from the input".into());
    }
    // 2. One ref per block.
    let layout = s.parts().layout(true).map_err(|e| format!("layout: {e}"))?;
    let mut blocks = 0;
    let mut live = 0;
    for story in &layout.stories {
        live += s.refs.live_in(&story.part).len();
        for b in &story.blocks {
            blocks += 1;
            let r = s.refs.ref_of(&b.key).ok_or("a block has no ref")?;
            if r.kind() != RefKind::of(b.kind) {
                return Err(format!("{r} has the letter of another kind"));
            }
            match s.resolve(Claim::Implied, &r.to_string()) {
                Ok(k) if k == b.key => {}
                other => return Err(format!("{r} resolves to {other:?}")),
            }
        }
    }
    if blocks != live {
        return Err(format!("{blocks} blocks and {live} live refs"));
    }
    // 3. Determinism.
    let again = State::open(bytes.to_vec(), &limits).map_err(|e| format!("second open: {e}"))?;
    let e = s.export().map_err(|e| format!("export: {e}"))?;
    let e2 = again.export().map_err(|e| format!("second export: {e}"))?;
    if e.refs != e2.refs || e.base != e2.base || e.head != e2.head {
        return Err("two opens of the same bytes export differently".into());
    }
    // 4. The saved form comes back.
    let back = State::resume(e.head.clone(), &e.refs, &recorded(&e), Some(bytes), &limits)
        .map_err(|e| format!("resume: {e}"))?;
    let again = back
        .export()
        .map_err(|e| format!("export after resume: {e}"))?;
    if again.refs != e.refs || again.base != e.base {
        return Err("export, resume, export changed the ref map".into());
    }
    let head = back
        .to_package()
        .and_then(|p| p.write())
        .map_err(|e| format!("save after resume: {e}"))?;
    if head != e.head {
        return Err("saving the resumed state differs from the exported head".into());
    }
    // 5. A damaged ref map is refused, or is still exactly what `persist` writes: single bytes
    // changed at a spread of positions, including the first and the last.
    for k in 0..8usize {
        let at = if k == 7 {
            e.refs.len().saturating_sub(1)
        } else {
            e.refs.len() * k / 8
        };
        let mut damaged = e.refs.clone();
        if let Some(b) = damaged.get_mut(at) {
            *b = if *b == b'#' { b'$' } else { b'#' };
        }
        if let Ok(table) = RefTable::restore(&damaged, &layout, Some(bytes), &limits) {
            let again = table
                .persist(&layout)
                .map_err(|e| format!("persist: {e}"))?;
            if again != damaged {
                return Err(format!(
                    "a ref map damaged at byte {at} was accepted and is not canonical"
                ));
            }
        }
    }
    // 6. Staging and discarding changes nothing.
    let before = (saved, e.head.clone(), e.refs.clone(), e.base);
    let names: Vec<String> = s.index.names().map(|n| n.to_string()).collect();
    let mut tx = s.begin();
    for name in &names {
        let _ = tx.part_mut(name);
    }
    tx.discard();
    let mut tx = s.begin();
    if let Some(first) = layout.stories.first().and_then(|st| st.blocks.first()) {
        let tree = tx
            .part_mut(first.key.part())
            .map_err(|e| format!("stage the body: {e}"))?;
        let parent = tree
            .node_ref(first.key.node())
            .parent()
            .map(|p| p.id())
            .ok_or("the first block has no parent")?;
        let mut edit = tree.edit();
        let p = edit.create_element(W, "p").map_err(|e| format!("{e}"))?;
        edit.insert_before(parent, Some(first.key.node()), p)
            .map_err(|e| format!("{e}"))?;
        tx.touch(&first.key);
    }
    tx.discard();
    let after = s
        .export()
        .map_err(|e| format!("export after discards: {e}"))?;
    let saved = s
        .to_package()
        .and_then(|p| p.write())
        .map_err(|e| format!("save after discards: {e}"))?;
    if (saved, after.head, after.refs, after.base) != before {
        return Err("a discarded transaction changed the state".into());
    }
    Ok(())
}

#[test]
fn the_laws_hold_for_every_document_of_the_corpus() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut failures = Vec::new();
    let mut checked = 0;
    for entry in &corpus.entries {
        let bytes = entry.bytes();
        match entry.expect {
            Expect::Ok if NOT_WORD.contains(&entry.id.as_str()) => {
                // The container and the XML are fine, and the document is not a Word document.
                let e = State::open(bytes, &Limits::default())
                    .err()
                    .map(|e| e.code());
                if e != Some(ErrorCode::MalformedInput) {
                    failures.push(format!("{}: expected malformed_input, got {e:?}", entry.id));
                }
            }
            Expect::Ok => {
                checked += 1;
                if let Err(why) = laws(&bytes) {
                    failures.push(format!("{}: {why}", entry.id));
                }
            }
            Expect::Refused {
                code,
                stage: Stage::Open | Stage::Document,
            } => match State::open(bytes, &Limits::default()) {
                Err(e) if e.code().as_str() == code => {}
                Err(e) => failures.push(format!(
                    "{}: refused with {}, not {code}",
                    entry.id,
                    e.code().as_str()
                )),
                Ok(_) => failures.push(format!("{}: opened, and the index says {code}", entry.id)),
            },
            Expect::Refused { .. } => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} documents broke a law:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(checked > 0, "the corpus has no document that must open");
}
