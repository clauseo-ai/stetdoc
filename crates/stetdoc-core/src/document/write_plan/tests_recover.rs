//! `recover` and the refusals around it: what becomes of an edit made after a crash, a directory that is not recovered yet,
//! and an entry name that stands for other bytes (law H12, the second half).

use super::harness::*;
use super::*;
use crate::api::options::OpenOptions;
use crate::api::view::FormatId;
use crate::document::Document;
use crate::document::tests::{opened, without};
use std::collections::HashMap;

/// The directory after a crash right after the commit: the new manifest is on disk and no readable file has been written yet.
fn crashed_after_commit(old: &WorkingCopy, new: &WorkingCopy) -> Dir {
    let plan = new.write_plan(Some(old), &WriteOptions::default()).unwrap();
    let commit = plan
        .steps
        .iter()
        .position(|s| matches!(s, WriteStep::Commit { .. }))
        .unwrap();
    let mut dir = Dir::of(old);
    dir.apply_all(&plan.steps[..=commit]);
    dir
}

fn recover_of(dir: &Dir) -> Recovery {
    let (wc, marker) = dir.read();
    WorkingCopy::recover(&wc, &marker.unwrap()).unwrap()
}

/// What the caller does with a recovery: the steps, the files to render again, and the marker last.
fn finish(dir: &mut Dir, recovery: &Recovery, new: &WorkingCopy) {
    dir.apply_all(&recovery.steps);
    rewrite(dir, recovery, new);
    dir.apply(&recovery.finish);
}

fn saved_as(name: &str, text: &str) -> String {
    format!("{name}.{}.stale", &hex(&sha256(text.as_bytes()))[..8])
}

/// Version 0 and version 1 of a document, each with the readable files given.
fn pair(
    old_files: &[(&str, &str)],
    new_files: &[(&str, &str)],
) -> (Document, Document, WorkingCopy, WorkingCopy) {
    let v0 = opened();
    let v1 = without(&v0, "p1");
    let old = v0.export_with(files(old_files)).unwrap();
    let new = v1.export_with(files(new_files)).unwrap();
    (v0, v1, old, new)
}

const AGENT: &str = "# one\n\nfirst render, edited by the agent\n";

#[test]
fn an_edit_made_after_the_crash_on_a_render_the_commit_replaced_is_saved_aside() {
    // The blocker of the review: left pending, this edit would be read against the new render, and the committed change
    // would look like the agent's own and be undone.
    let (_, _, old, new) = pair(
        &[("document.md", DOC_V0), ("notes.md", NOTES)],
        &[("document.md", DOC_V1), ("notes.md", NOTES)],
    );
    let mut dir = crashed_after_commit(&old, &new);
    dir.set("document.md", AGENT);
    let recovery = recover_of(&dir);
    assert!(recovery.kept.is_empty(), "{:?}", recovery.kept);
    assert_eq!(recovery.rerender, ["document.md"]);
    let saved = saved_as("document.md", AGENT);
    assert!(recovery.steps.contains(&WriteStep::File {
        path: saved.clone(),
        text: AGENT.to_string()
    }));
    assert_eq!(recovery.warnings.len(), 1);
    let warning = &recovery.warnings[0];
    assert!(
        warning.contains("`document.md`")
            && warning.contains(&saved)
            && warning.contains("Redo the edit"),
        "{warning}"
    );
    finish(&mut dir, &recovery, &new);
    assert_eq!(dir.paths[&saved], AGENT.as_bytes());
    let (settled, marker) = dir.read();
    assert_eq!(marker, None);
    // The file is the committed render, nothing is pending, and the saved file is not a readable file.
    assert_eq!(files_of(&settled), files_of(&new));
    assert_eq!(settled.pending_files().unwrap(), None);
    assert!(settled.files.iter().all(|(n, _)| !n.ends_with(".stale")));
}

#[test]
fn recovery_run_again_after_the_edit_was_saved_aside_gives_the_same_directory() {
    let (_, _, old, new) = pair(&[("document.md", DOC_V0)], &[("document.md", DOC_V1)]);
    let mut dir = crashed_after_commit(&old, &new);
    dir.set("document.md", AGENT);
    let first = recover_of(&dir);
    let mut once = dir.clone();
    finish(&mut once, &first, &new);
    // A crash after the copy: the file still holds the agent's text, and the same copy is made again.
    let mut copied = dir.clone();
    copied.apply_all(&first.steps);
    let second = recover_of(&copied);
    assert_eq!(second.warnings, first.warnings);
    assert_eq!(second.rerender, first.rerender);
    assert!(second.steps.contains(&WriteStep::File {
        path: saved_as("document.md", AGENT),
        text: AGENT.to_string()
    }));
    finish(&mut copied, &second, &new);
    assert_eq!(copied, once);
    // A crash after the file was moved away, as a rename would leave it: it is rendered again, and the saved text stays.
    let mut renamed = dir.clone();
    renamed.apply_all(&first.steps);
    renamed.paths.remove("document.md");
    let third = recover_of(&renamed);
    assert_eq!(third.rerender, ["document.md"]);
    assert!(third.warnings.is_empty());
    finish(&mut renamed, &third, &new);
    assert_eq!(renamed, once);
    // A crash after the file was rendered again and before the marker went.
    let mut written = dir.clone();
    written.apply_all(&first.steps);
    rewrite(&mut written, &first, &new);
    let fourth = recover_of(&written);
    assert!(fourth.rerender.is_empty() && fourth.kept.is_empty());
    finish(&mut written, &fourth, &new);
    assert_eq!(written, once);
    // Two different edits are saved under two names, and one edit twice under one.
    let other_edit = "# one\n\nanother edit\n";
    assert_ne!(
        saved_as("document.md", AGENT),
        saved_as("document.md", other_edit)
    );
}

#[test]
fn an_edit_made_after_the_crash_on_a_file_the_write_did_not_change_stays_pending() {
    // `notes.md` has the same text in the old render and the new one, so the edit is against the render that the new manifest
    // records, and it is exactly what `pending_files` reports.
    let (_, v1, old, new) = pair(
        &[("document.md", DOC_V0), ("notes.md", NOTES)],
        &[("document.md", DOC_V1), ("notes.md", NOTES)],
    );
    let mut dir = crashed_after_commit(&old, &new);
    let note = "[^1]: a note, edited\n";
    dir.set("notes.md", note);
    let recovery = recover_of(&dir);
    assert_eq!(recovery.kept, ["notes.md"]);
    assert_eq!(recovery.rerender, ["document.md"]);
    assert!(recovery.warnings.is_empty());
    finish(&mut dir, &recovery, &new);
    let (settled, _) = dir.read();
    let pending = settled.pending_files().unwrap().unwrap();
    assert_eq!(pending.files, files(&[("notes.md", note)]));
    assert_eq!(pending.base, v1.base());
    assert_eq!(
        pending.rendered,
        files(&[("notes.md", &hex(&sha256(NOTES.as_bytes())))])
    );
    assert!(dir.paths.keys().all(|p| !p.ends_with(".stale")));
}

#[test]
fn a_file_the_new_render_adds_is_saved_aside_if_the_agent_wrote_it_first() {
    let (_, _, old, new) = pair(
        &[("document.md", DOC_V0)],
        &[("document.md", DOC_V1), ("comments.md", "c\n")],
    );
    let mut dir = crashed_after_commit(&old, &new);
    dir.set("comments.md", "my own text\n");
    let recovery = recover_of(&dir);
    assert!(recovery.kept.is_empty());
    assert_eq!(recovery.rerender, ["comments.md", "document.md"]);
    assert_eq!(recovery.warnings.len(), 1);
    finish(&mut dir, &recovery, &new);
    assert!(dir.saved("comments.md", "my own text\n").is_some());
    let (settled, _) = dir.read();
    assert_eq!(files_of(&settled), files_of(&new));
    assert_eq!(settled.pending_files().unwrap(), None);
}

#[test]
fn a_file_the_new_render_drops_goes_and_its_edit_after_the_crash_is_saved_aside() {
    let (_, _, old, new) = pair(
        &[("document.md", DOC_V0), ("notes.md", NOTES)],
        &[("document.md", DOC_V1)],
    );
    // Unedited: it goes, and there is nothing to say.
    let mut dir = crashed_after_commit(&old, &new);
    let recovery = recover_of(&dir);
    assert!(recovery.warnings.is_empty());
    assert!(recovery.steps.contains(&WriteStep::Remove {
        path: "notes.md".to_string()
    }));
    finish(&mut dir, &recovery, &new);
    assert!(!dir.paths.contains_key("notes.md"));
    // Edited after the crash: the copy comes before the removal.
    let mut dir = crashed_after_commit(&old, &new);
    let note = "[^1]: a note, edited\n";
    dir.set("notes.md", note);
    let recovery = recover_of(&dir);
    let copy = recovery
        .steps
        .iter()
        .position(
            |s| matches!(s, WriteStep::File { path, .. } if *path == saved_as("notes.md", note)),
        )
        .unwrap();
    let removal = recovery
        .steps
        .iter()
        .position(|s| matches!(s, WriteStep::Remove { path } if path == "notes.md"))
        .unwrap();
    assert!(copy < removal);
    assert_eq!(recovery.warnings.len(), 1);
    assert!(
        recovery.warnings[0].contains("no longer part"),
        "{:?}",
        recovery.warnings
    );
    finish(&mut dir, &recovery, &new);
    assert!(!dir.paths.contains_key("notes.md"));
    assert!(dir.saved("notes.md", note).is_some());
    let (settled, _) = dir.read();
    assert_eq!(settled.pending_files().unwrap(), None);
}

#[test]
fn an_edit_made_after_a_crash_at_any_step_is_never_lost() {
    let v0 = opened();
    let v1 = without(&v0, "p1");
    let agent = "# one\n\nthe agent's edit\n";
    let both = [("document.md", DOC_V0), ("notes.md", NOTES)];
    let old = v0.export_with(files(&both)).unwrap();
    for new_files in [
        &[("document.md", DOC_V1), ("notes.md", NOTES)][..],
        &[("document.md", DOC_V1)][..],
        &[
            ("document.md", DOC_V1),
            ("notes.md", "[^1]: a changed note\n"),
            ("comments.md", "c\n"),
        ][..],
    ] {
        let new = v1.export_with(files(new_files)).unwrap();
        for name in ["document.md", "notes.md", "comments.md"] {
            // An edit of a file that neither working copy has is not an edit of the working copy.
            if !both.iter().chain(new_files.iter()).any(|(n, _)| *n == name) {
                continue;
            }
            Scenario {
                old: Some((&v0, &old)),
                hand: &[],
                new: (&v1, &new),
                options: WriteOptions::default(),
            }
            .run_with_edit_after_the_crash(name, agent);
        }
    }
    // After a consumed hand edit, and after a first unpack.
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
    .run_with_edit_after_the_crash("document.md", agent);
    Scenario {
        old: None,
        hand: &[],
        new: (&v1, &new),
        options: WriteOptions::default(),
    }
    .run_with_edit_after_the_crash("document.md", agent);
}

#[test]
fn a_stray_markdown_file_is_left_alone_by_a_write_and_by_its_recovery() {
    // The marker records every readable file on disk, so that recovery can tell an edit made after the crash. It must not
    // read that as a list of files to remove: a stray `.md` file is in neither manifest, the plan keeps it, and so does
    // recovery. Only the files the plan said it would remove go.
    let (v0, v1, old, new) = pair(
        &[("document.md", DOC_V0), ("notes.md", NOTES)],
        &[("document.md", DOC_V1)],
    );
    let stray = "the agent's own file\n";
    let mut dir = crashed_after_commit(&old, &new);
    dir.set("appendix.md", stray);
    let recovery = recover_of(&dir);
    assert!(
        !recovery
            .steps
            .iter()
            .any(|s| matches!(s, WriteStep::Remove { path } if path == "appendix.md")),
        "{:?}",
        recovery.steps
    );
    assert!(recovery.steps.contains(&WriteStep::Remove {
        path: "notes.md".to_string()
    }));
    assert!(recovery.warnings.is_empty() && recovery.kept.is_empty());
    finish(&mut dir, &recovery, &new);
    assert_eq!(dir.paths["appendix.md"], stray.as_bytes());
    assert!(!dir.paths.contains_key("notes.md"));
    // At every crash point, with an edit after it or without, and for a first unpack as well.
    for hand in [&[("appendix.md", stray)][..], &[][..]] {
        for edit in [None, Some(("document.md", "# one\n\nthe agent's edit\n"))] {
            let scenario = Scenario {
                old: Some((&v0, &old)),
                hand,
                new: (&v1, &new),
                options: WriteOptions::default(),
            };
            match edit {
                None => scenario.run(),
                Some((name, text)) => scenario.run_with_edit_after_the_crash(name, text),
            }
        }
    }
    Scenario {
        old: None,
        hand: &[("appendix.md", stray)],
        new: (&v1, &new),
        options: WriteOptions::default(),
    }
    .run();
}

#[test]
fn a_directory_with_an_interrupted_write_is_refused_until_it_is_recovered() {
    let (_, _, old, new) = pair(&[("document.md", DOC_V0)], &[("document.md", DOC_V1)]);
    let plan = new
        .write_plan(Some(&old), &WriteOptions::default())
        .unwrap();
    for k in 1..plan.steps.len() {
        let mut dir = Dir::of(&old);
        dir.apply_all(&plan.steps[..k]);
        let (interrupted, marker) = dir.read();
        let marker = marker.unwrap();
        assert!(interrupted.complement.entry(MARKER).unwrap().is_some());
        let refusals = [
            interrupted.pending_files().err(),
            interrupted.ensure_clean().err(),
            Document::resume(&interrupted, &OpenOptions::default()).err(),
            new.write_plan(Some(&interrupted), &WriteOptions::default())
                .err(),
        ];
        for refusal in refusals {
            let e = refusal.unwrap_or_else(|| {
                panic!("step {k}: a directory in the middle of a write was accepted")
            });
            assert_eq!(e.code(), ErrorCode::CorruptComplement, "step {k}");
            assert!(
                e.message().contains("interrupted") && e.message().contains("Recover"),
                "{e}"
            );
        }
        // Recovery is the one reader that takes it, and after it nothing refuses.
        let recovery = WorkingCopy::recover(&interrupted, &marker).unwrap();
        finish(&mut dir, &recovery, &new);
        let (clean, marker) = dir.read();
        assert_eq!(marker, None);
        clean.ensure_clean().unwrap();
        Document::resume(&clean, &OpenOptions::default()).unwrap();
        new.write_plan(Some(&clean), &WriteOptions::default())
            .unwrap();
    }
}

/// Two byte strings whose SHA-256 digests start with the same four bytes: the first eight hex digits of a name. Thirty-two
/// bits collide after about 2^16 tries.
fn colliding_pair() -> Option<(Vec<u8>, Vec<u8>)> {
    let mut seen: HashMap<[u8; 4], u32> = HashMap::new();
    for i in 0..=u32::MAX {
        let d = sha256(&i.to_le_bytes());
        if let Some(j) = seen.insert([d[0], d[1], d[2], d[3]], i) {
            return Some((j.to_le_bytes().to_vec(), i.to_le_bytes().to_vec()));
        }
    }
    None
}

/// The working copy with another ref map, and a manifest that says so.
fn with_refs(wc: &WorkingCopy, refs: Vec<u8>) -> WorkingCopy {
    let m = wc.manifest().unwrap();
    let head = m.head.as_ref().map_or(m.original, |h| h.sha256);
    let manifest = Manifest::new(
        m.base,
        &FormatId::stet(),
        m.original,
        head,
        sha256(&refs),
        &wc.files,
    )
    .unwrap();
    let mut out = wc.clone();
    out.complement
        .entries
        .retain(|(n, _)| n != MANIFEST && *n != m.refs.name);
    out.complement
        .entries
        .push((manifest.refs.name.clone(), refs));
    out.complement
        .entries
        .push((MANIFEST.to_string(), manifest.render().into_bytes()));
    out
}

#[test]
fn an_entry_is_not_skipped_by_its_name_alone() {
    let (a, b) = colliding_pair().expect("32 bits collide");
    let base = opened().export().unwrap();
    let (one, two) = (with_refs(&base, a.clone()), with_refs(&base, b));
    let (m1, m2) = (one.manifest().unwrap(), two.manifest().unwrap());
    assert_eq!(m1.refs.name, m2.refs.name, "the names collide");
    assert_ne!(m1.refs.sha256, m2.refs.sha256, "and the bytes differ");
    let e = two
        .write_plan(Some(&one), &WriteOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::Internal);
    assert!(
        e.message().contains(&m1.refs.name) && e.message().contains("collision"),
        "{e}"
    );
    // The same bytes under the same name are the ordinary case: the entry is not written again.
    let same = with_refs(&base, a);
    let steps = same
        .write_plan(Some(&one), &WriteOptions::default())
        .unwrap()
        .steps;
    assert!(steps.iter().all(|s| !matches!(s, WriteStep::Entry { .. })));
}

#[test]
fn recover_refuses_a_marker_or_a_directory_that_does_not_fit() {
    let v0 = opened();
    let v1 = without(&v0, "p1");
    let v1b = without(&v0, "p2");
    let (old, new, other) = (
        v0.export().unwrap(),
        v1.export().unwrap(),
        v1b.export().unwrap(),
    );
    let marker = |from: &WorkingCopy, to: &WorkingCopy| {
        let plan = to.write_plan(Some(from), &WriteOptions::default()).unwrap();
        match &plan.steps[0] {
            WriteStep::Marker { bytes, .. } => bytes.clone(),
            other => panic!("{other:?}"),
        }
    };
    // A marker of one write against the directory of another.
    let e = WorkingCopy::recover(&other, &marker(&old, &new))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::CorruptComplement);
    // Garbage, and every cut of a marker.
    let good = marker(&old, &new);
    assert!(WorkingCopy::recover(&new, &good).is_ok());
    for len in 0..good.len() {
        let e = WorkingCopy::recover(&new, &good[..len]).err();
        // A cut that ends on a line boundary may still be a marker of fewer files, which is not an error.
        if let Some(e) = e {
            assert_eq!(e.code(), ErrorCode::CorruptComplement, "cut at {len}");
        }
    }
    for garbage in [
        &b""[..],
        b"hello",
        b"stetdoc-writing 2\n",
        b"stetdoc-writing 1\nfrom x\nto 1\n",
    ] {
        assert_eq!(
            WorkingCopy::recover(&new, garbage).err().unwrap().code(),
            ErrorCode::CorruptComplement
        );
    }
    // No manifest at all: an interrupted first unpack.
    let e = WorkingCopy::recover(&WorkingCopy::default(), &good)
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::CorruptComplement);
    assert!(e.message().contains("Delete the directory"), "{e}");
}

#[test]
fn a_directory_of_another_document_is_refused() {
    let one = opened().export().unwrap();
    let two = crate::document::tests::other().export().unwrap();
    assert_ne!(
        one.manifest().unwrap().original,
        two.manifest().unwrap().original
    );
    let e = two
        .write_plan(Some(&one), &WriteOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    // A working copy that is not a working copy cannot be written.
    let e = WorkingCopy::default()
        .write_plan(None, &WriteOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::CorruptComplement);
}
