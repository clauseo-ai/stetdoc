//! Atomicity and determinism: an error at any step leaves the state as it was, and the same input
//! gives the same output on every run and in every thread.

use super::failpoint;
use super::tests::*;
use super::tests_edit::{refs, snapshot};
use super::*;
use crate::api::error::ErrorCode;
use crate::package::NewPart;
use crate::wordml::IdKind;
use std::collections::BTreeSet;

fn comments_part() -> NewPart<'static> {
    NewPart {
        name: "word/comments.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        bytes: format!(
            "<w:comments xmlns:w=\"{W}\"><w:comment w:id=\"0\" w:author=\"a\"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:comment></w:comments>"
        )
        .into_bytes(),
        related_from: Some((
            MAIN,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
        )),
    }
}

/// An edit that does a bit of everything: it stages two parts, inserts, merges, deletes, adds a part,
/// mints an id and commits. Every call that can fail uses `?`, so an injected error comes out.
fn script(s: &State) -> Result<(State, Committed, u32), StetError> {
    let keys = body_keys(s);
    let mut tx = s.begin();
    let (new, parent) = {
        let tree = tx.part_mut(MAIN)?;
        let parent = tree.node_ref(keys[0].node()).parent().map(|p| p.id());
        let mut e = tree.edit();
        let p = e.create_element(W, "p")?;
        (p, parent)
    };
    let parent =
        parent.ok_or_else(|| StetError::new(ErrorCode::Internal, "internal: no parent"))?;
    tx.part_mut(MAIN)?
        .edit()
        .insert_before(parent, Some(keys[0].node()), new)?;
    // Merge the first two original paragraphs, keeping the second, and delete the third.
    tx.part_mut(MAIN)?.edit().detach(keys[0].node())?;
    tx.merge(&keys[0], &keys[1], keys[1].clone())?;
    tx.part_mut(MAIN)?.edit().detach(keys[2].node())?;
    tx.touch(&keys[1]);
    tx.add_part(comments_part())?;
    let id = tx.mint_id(IdKind::ParaId, MAIN)?;
    let (next, committed) = tx.commit()?;
    Ok((next, committed, id))
}

fn doc() -> State {
    state(&paragraphs(&["a", "b", "c", "d"]))
}

/// Runs `script` once for every step that can fail, failing that step, and checks each time that
/// the state it started from is as it was. Returns the steps that were reached.
fn every_step_fails_cleanly() -> BTreeSet<&'static str> {
    let s = doc();
    let before = snapshot(&s);
    let tokens = (s.base_token(), refs(&s));
    let mut reached = BTreeSet::new();
    let mut steps = 0;
    loop {
        failpoint::arm(steps);
        let outcome = script(&s);
        let seen = failpoint::disarm();
        match outcome {
            Err(e) => {
                assert_eq!(e.code(), ErrorCode::Internal, "{e}");
                assert!(
                    e.message().contains("injected failure"),
                    "step {steps}: {e}"
                );
                reached.extend(seen.last().copied());
                assert_eq!(
                    snapshot(&s),
                    before,
                    "after a failure at step {steps} ({:?})",
                    seen.last()
                );
                assert_eq!((s.base_token(), refs(&s)), tokens, "step {steps}");
            }
            Ok((next, _, _)) => {
                // Every step has been reached. The edit committed, and still did not touch the base.
                assert_eq!(snapshot(&s), before);
                assert_eq!(next.base_token().version(), 1);
                break;
            }
        }
        steps += 1;
        assert!(steps < 200, "the script never finishes");
    }
    assert!(steps >= 12, "only {steps} steps were reached");
    reached
}

#[test]
fn an_error_at_any_step_of_an_edit_leaves_the_state_as_it_was() {
    let reached = every_step_fails_cleanly();
    for step in [
        "tx:part_mut",
        "tx:add_part",
        "commit:begin",
        "audit:scan",
        "audit:declarations",
        "audit:check",
        "audit:tombstone",
        "audit:mint",
        "audit:stamp",
        "commit:size",
        "commit:hash",
        "commit:advance",
    ] {
        assert!(
            reached.contains(step),
            "the step {step} was never reached: {reached:?}"
        );
    }
}

#[test]
fn a_transaction_dropped_after_any_staged_edits_changes_nothing() {
    let s = doc();
    let before = snapshot(&s);
    let keys = body_keys(&s);
    for stop in 0..6 {
        let mut tx = s.begin();
        if stop > 0 {
            detach(&mut tx, &keys[0]);
        }
        if stop > 1 {
            tx.merge(&keys[0], &keys[1], keys[1].clone()).unwrap();
        }
        if stop > 2 {
            let n = new_paragraph(&mut tx, "x");
            put_after(&mut tx, n, &keys[3]);
        }
        if stop > 3 {
            tx.add_part(comments_part()).unwrap();
        }
        if stop > 4 {
            tx.mint_id(IdKind::TextId, MAIN).unwrap();
            tx.parts().story(id(&s, MAIN)).unwrap();
        }
        tx.discard();
        assert_eq!(snapshot(&s), before, "after stopping at {stop}");
    }
}

/// What a run produces, as bytes and text: everything the determinism rules cover.
fn fingerprint_of_run() -> Vec<String> {
    let s = doc();
    let (next, committed, id) = script(&s).unwrap();
    let saved = next.to_package().unwrap().write().unwrap();
    let e = next.export().unwrap();
    vec![
        format!("{:x?}", crate::refs::sha256(&saved)),
        format!("{:x?}", crate::refs::sha256(&e.head)),
        String::from_utf8(e.refs).unwrap(),
        e.base.to_string(),
        format!(
            "{:?}",
            committed
                .minted
                .iter()
                .map(|(_, r)| r.to_string())
                .collect::<Vec<_>>()
        ),
        format!("{:?}", committed.merged),
        id.to_string(),
    ]
}

#[test]
fn the_same_input_gives_the_same_output_twenty_times_and_in_two_threads() {
    let reference = fingerprint_of_run();
    for _ in 0..20 {
        assert_eq!(fingerprint_of_run(), reference);
    }
    // Each thread's hash maps have their own seeds, so an iteration order that leaked into an
    // output would show up here.
    let handles: Vec<_> = (0..2)
        .map(|_| std::thread::spawn(|| (0..10).map(|_| fingerprint_of_run()).collect::<Vec<_>>()))
        .collect();
    for h in handles {
        for run in h.join().unwrap() {
            assert_eq!(run, reference);
        }
    }
}

#[test]
fn opening_the_bytes_of_an_unedited_save_gives_the_same_namespace_and_an_edited_save_a_new_one() {
    let bytes = package_bytes(&paragraphs(&["a", "b"]));
    let s = state_from(bytes.clone(), &Limits::default());
    let saved = s.to_package().unwrap().write().unwrap();
    assert_eq!(saved, bytes);
    assert_eq!(
        state_from(saved, &Limits::default()).base_token(),
        s.base_token()
    );
    let mut tx = s.begin();
    detach(&mut tx, &body_keys(&s)[0]);
    let (edited, _) = tx.commit().unwrap();
    let reopened = state_from(
        edited.to_package().unwrap().write().unwrap(),
        &Limits::default(),
    );
    assert_ne!(
        reopened.base_token().namespace(),
        s.base_token().namespace()
    );
    // Every old ref is stale in the new namespace.
    let old = s.base_token();
    assert_eq!(
        reopened
            .begin()
            .resolve(crate::refs::Claim::Base(&old), "p1")
            .unwrap_err()
            .code(),
        ErrorCode::StaleRef
    );
}

#[test]
fn a_state_is_not_shared_between_threads_but_can_move_to_one() {
    let s = doc();
    let token = std::thread::spawn(move || {
        let (next, _, _) = script(&s).unwrap();
        next.base_token().to_string()
    })
    .join()
    .unwrap();
    assert!(token.ends_with(&format!(":1:{}", token.rsplit(':').next().unwrap())));
}
