//! The write protocol against an in-memory directory, stopped after every step (law H12).
//!
//! The scenarios run with the real documents of `document/tests.rs` and, until the projection renders readable files, with files
//! that the test supplies through `export_with`. After a crash at every step, with and without hand edits, and with a crash at
//! every step of `recover` as well, the directory must come back to exactly the old working copy or exactly the new one.

use super::harness::*;
use super::*;
use crate::document::tests::{opened, without};

#[test]
fn a_first_unpack_survives_a_crash_at_every_step() {
    let doc = opened();
    for with_files in [false, true] {
        let wc = doc
            .export_with(if with_files {
                files(&[("document.md", DOC_V0), ("notes.md", NOTES)])
            } else {
                Vec::new()
            })
            .unwrap();
        Scenario {
            old: None,
            hand: &[],
            new: (&doc, &wc),
            options: WriteOptions::default(),
        }
        .run();
    }
}

#[test]
fn the_plan_has_the_order_that_makes_a_crash_harmless() {
    let doc = opened();
    let wc = doc
        .export_with(files(&[("document.md", DOC_V0), ("notes.md", NOTES)]))
        .unwrap();
    let steps = wc.write_plan(None, &WriteOptions::default()).unwrap().steps;
    let kinds: Vec<&str> = steps
        .iter()
        .map(|s| match s {
            WriteStep::Marker { .. } => "marker",
            WriteStep::Entry { .. } => "entry",
            WriteStep::Commit { .. } => "commit",
            WriteStep::File { .. } => "file",
            WriteStep::Remove { .. } => "remove",
        })
        .collect();
    // The marker, the three entries of an unedited first unpack, the manifest, the two files, the marker's removal.
    assert_eq!(
        kinds,
        [
            "marker", "entry", "entry", "commit", "file", "file", "remove"
        ]
    );
    assert!(matches!(&steps[0], WriteStep::Marker { path, .. } if path == ".stet/writing"));
    assert!(matches!(&steps[3], WriteStep::Commit { path, .. } if path == ".stet/manifest"));
    assert!(matches!(&steps[6], WriteStep::Remove { path } if path == ".stet/writing"));
    // A second write of the same working copy over itself writes no entry and no file, and still ends clean.
    let again = wc
        .write_plan(Some(&wc), &WriteOptions::default())
        .unwrap()
        .steps;
    let kinds: Vec<bool> = again
        .iter()
        .map(|s| matches!(s, WriteStep::Entry { .. } | WriteStep::File { .. }))
        .collect();
    assert!(kinds.iter().all(|k| !k), "{again:?}");
    let mut dir = Dir::of(&wc);
    dir.apply_all(&again);
    assert_eq!(dir, Dir::of(&wc));
}

#[test]
fn an_update_survives_a_crash_at_every_step_with_and_without_files() {
    let v0 = opened();
    let v1 = without(&v0, "p1");
    for (old_files, new_files) in [
        (Vec::new(), Vec::new()),
        (
            files(&[("document.md", DOC_V0), ("notes.md", NOTES)]),
            files(&[("document.md", DOC_V1), ("notes.md", NOTES)]),
        ),
        // A file that the new render no longer has goes with the update.
        (
            files(&[("document.md", DOC_V0), ("notes.md", NOTES)]),
            files(&[("document.md", DOC_V1)]),
        ),
        // A file that the new render adds.
        (
            files(&[("document.md", DOC_V0)]),
            files(&[("document.md", DOC_V1), ("comments.md", "c\n")]),
        ),
    ] {
        let old = v0.export_with(old_files).unwrap();
        let new = v1.export_with(new_files).unwrap();
        Scenario {
            old: Some((&v0, &old)),
            hand: &[],
            new: (&v1, &new),
            options: WriteOptions::default(),
        }
        .run();
    }
}

#[test]
fn a_consumed_hand_edit_is_replaced_and_survives_a_crash_at_every_step() {
    let v0 = opened();
    let v1 = without(&v0, "p1");
    let old = v0
        .export_with(files(&[("document.md", DOC_V0), ("notes.md", NOTES)]))
        .unwrap();
    let new = v1
        .export_with(files(&[("document.md", DOC_V1), ("notes.md", NOTES)]))
        .unwrap();
    Scenario {
        old: Some((&v0, &old)),
        hand: &[("document.md", "# one\n\na hand edit\n")],
        new: (&v1, &new),
        options: WriteOptions {
            consumed: vec!["document.md".to_string()],
            refresh: false,
        },
    }
    .run();
}

#[test]
fn a_refresh_export_at_the_same_generation_never_overwrites_a_committed_entry() {
    // Two real states at generation 1 whose head and ref map differ.
    let v0 = opened();
    let a = without(&v0, "p1");
    let b = without(&v0, "p2");
    let old = a.export_with(files(&[("document.md", DOC_V0)])).unwrap();
    let new = b.export_with(files(&[("document.md", DOC_V1)])).unwrap();
    let (m_old, m_new) = (old.manifest().unwrap(), new.manifest().unwrap());
    assert_eq!(m_old.generation, m_new.generation);
    assert_ne!(m_old.refs.name, m_new.refs.name);
    // The old manifest's entries are never among the writes, and the marker tells the two manifests apart.
    let plan = new
        .write_plan(Some(&old), &WriteOptions::default())
        .unwrap();
    for step in &plan.steps {
        if let WriteStep::Entry { path, .. } = step {
            assert!(
                !m_old.entry_names().iter().any(|n| path_of(n) == *path),
                "{path} would overwrite an entry the old manifest names"
            );
        }
    }
    for hand in [&[][..], &[("document.md", "# a hand edit\n")][..]] {
        Scenario {
            old: Some((&a, &old)),
            hand,
            new: (&b, &new),
            options: WriteOptions {
                consumed: Vec::new(),
                refresh: !hand.is_empty(),
            },
        }
        .run();
    }
}

#[test]
fn a_plan_that_would_overwrite_a_hand_edit_is_refused_before_its_first_step() {
    let v0 = opened();
    let v1 = without(&v0, "p1");
    let old = v0
        .export_with(files(&[("document.md", DOC_V0), ("notes.md", NOTES)]))
        .unwrap();
    let new = v1.export_with(files(&[("document.md", DOC_V1)])).unwrap();
    for hand in [("document.md", "edited"), ("notes.md", "edited")] {
        let hand_edit = [hand];
        let scenario = Scenario {
            old: Some((&v0, &old)),
            hand: &hand_edit,
            new: (&v1, &new),
            options: WriteOptions::default(),
        };
        let e = scenario
            .plan()
            .err()
            .unwrap_or_else(|| panic!("{hand:?} was overwritten"));
        assert_eq!(e.code(), ErrorCode::PendingEdits, "{hand:?}");
        assert!(e.message().contains(hand.0), "{e}");
        // Naming the file as consumed, or asking for a refresh, lets the write go on.
        for options in [
            WriteOptions {
                consumed: vec![hand.0.to_string()],
                refresh: false,
            },
            WriteOptions {
                consumed: Vec::new(),
                refresh: true,
            },
        ] {
            Scenario {
                options,
                ..scenario
            }
            .plan()
            .unwrap();
        }
    }
    // A hand edit that equals the new render is no conflict.
    let scenario = Scenario {
        old: Some((&v0, &old)),
        hand: &[("document.md", DOC_V1)],
        new: (&v1, &new),
        options: WriteOptions::default(),
    };
    scenario.plan().unwrap();
}
