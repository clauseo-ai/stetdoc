//! The executor's own crash test: a write stopped after every step, then any command, leaves exactly the old working copy
//! or exactly the new one, and loses no hand edit.

use super::Workdir;
use std::fs;
use std::path::Path;
use stetdoc_core::{
    Batch, Document, Edit, Limits, OpenOptions, Stamp, WorkingCopy, WriteOptions, WriteStep,
};
use stetdoc_testkit::fixtures::contract;

fn stamp() -> Stamp {
    Stamp::new("Clauseo", "2026-10-06T09:30:00")
}

/// Unpacks the testkit's contract into `dir`.
fn unpack(dir: &Path) {
    let doc = Document::open(contract::package(), &OpenOptions::default()).unwrap();
    let copy = doc.export().unwrap();
    let workdir = Workdir::create(dir).unwrap();
    workdir
        .write(&copy, None, &WriteOptions::default())
        .unwrap();
}

/// A working copy with its entries in name order, so two can be compared whatever order they were listed in.
fn sorted(mut copy: WorkingCopy) -> WorkingCopy {
    copy.files.sort();
    copy.complement.entries.sort();
    copy
}

/// Reopens the directory (which recovers), and reads it.
fn reopen(dir: &Path) -> WorkingCopy {
    let (workdir, _) = Workdir::open(dir).unwrap();
    sorted(workdir.read().unwrap())
}

/// The old working copy, the new one and the plan between them, for an edit made by `change`.
fn plan(
    dir: &Path,
    change: impl Fn(&mut Document, &WorkingCopy) -> WriteOptions,
) -> (WorkingCopy, WorkingCopy, Vec<WriteStep>) {
    let (workdir, _) = Workdir::open(dir).unwrap();
    let (mut doc, disk) = workdir.resume().unwrap();
    let options = change(&mut doc, &disk);
    let next = doc.export().unwrap();
    let steps = next.write_plan(Some(&disk), &options).unwrap().steps;
    (sorted(disk), sorted(next), steps)
}

fn edit_ops(doc: &mut Document, _: &WorkingCopy) -> WriteOptions {
    let text = format!(
        "base: {}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n",
        doc.base()
    );
    let batch = Batch::parse(&text, &Limits::default()).unwrap();
    doc.edit(Edit::ops(batch, stamp())).unwrap();
    WriteOptions::default()
}

fn edit_files(doc: &mut Document, disk: &WorkingCopy) -> WriteOptions {
    let pending = disk.pending_files().unwrap().unwrap();
    let consumed = pending.files.iter().map(|(n, _)| n.clone()).collect();
    doc.edit(Edit::files(pending, stamp())).unwrap();
    WriteOptions {
        consumed,
        refresh: false,
    }
}

fn hand_edit(dir: &Path) {
    let path = dir.join("document.md");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("thirty"));
    fs::write(&path, text.replace("thirty", "forty-five")).unwrap();
}

/// Stops the write of `change` after each step in turn, and checks what a later command finds.
fn crash_everywhere(
    prepare: impl Fn(&Path),
    change: impl Fn(&mut Document, &WorkingCopy) -> WriteOptions + Copy,
) {
    let steps = {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("c.stet");
        prepare(&dir);
        plan(&dir, change).2.len()
    };
    assert!(steps > 2, "a write has a marker, a commit and a removal");
    for k in 0..=steps {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("c.stet");
        prepare(&dir);
        let (old, new, plan) = plan(&dir, change);
        {
            let (workdir, _) = Workdir::open(&dir).unwrap();
            workdir.execute(&plan, k).unwrap();
        }
        let found = reopen(&dir);
        assert!(
            found == old || found == new,
            "stopped after {k} of {steps} steps, the working copy is neither the old one nor the new one"
        );
        if k == steps {
            assert_eq!(found, new, "a complete write gives the new working copy");
        }
        assert!(
            !dir.join(".stet/writing").exists(),
            "recovery removes the marker"
        );
        let (doc, copy) = Workdir::open(&dir).unwrap().0.resume().unwrap();
        assert_eq!(sorted(doc.export().unwrap()).complement, copy.complement);
    }
}

#[test]
fn an_unpacked_working_copy_resumes_to_the_same_export() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("c.stet");
    unpack(&dir);
    let (workdir, notes) = Workdir::open(&dir).unwrap();
    assert!(notes.is_empty());
    let (doc, copy) = workdir.resume().unwrap();
    assert_eq!(sorted(doc.export().unwrap()), sorted(copy));
    assert!(dir.join("document.md").is_file());
    assert!(dir.join(".stet/lock").is_file());
}

#[test]
fn a_crash_at_any_step_of_an_edit_leaves_the_old_or_the_new_working_copy() {
    crash_everywhere(unpack, edit_ops);
}

#[test]
fn a_crash_at_any_step_of_a_pack_keeps_the_hand_edit_or_takes_it_in() {
    crash_everywhere(
        |dir| {
            unpack(dir);
            hand_edit(dir);
        },
        edit_files,
    );
}

#[test]
fn an_edit_made_after_a_crash_is_never_lost() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("c.stet");
    unpack(&dir);
    let (_, _, steps) = plan(&dir, edit_ops);
    let commit = steps
        .iter()
        .position(|s| matches!(s, WriteStep::Commit { .. }))
        .unwrap();
    {
        let (workdir, _) = Workdir::open(&dir).unwrap();
        workdir.execute(&steps, commit + 1).unwrap();
    }
    // The agent edits the old render after the crash, before any command recovers.
    let path = dir.join("document.md");
    let edited = fs::read_to_string(&path)
        .unwrap()
        .replace("governed by", "construed under");
    fs::write(&path, &edited).unwrap();
    let (_, notes) = Workdir::open(&dir).unwrap();
    let saved: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".stale"))
        .map(|e| fs::read_to_string(e.path()).unwrap())
        .collect();
    let kept = fs::read_to_string(&path).unwrap() == edited;
    assert!(
        kept || saved.contains(&edited),
        "the edit is kept or saved aside"
    );
    assert!(!notes.is_empty(), "the agent is told about the recovery");
}

#[test]
fn a_step_that_would_leave_the_directory_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("c.stet");
    unpack(&dir);
    let (workdir, _) = Workdir::open(&dir).unwrap();
    for path in ["../outside.md", "/tmp/outside.md", ""] {
        let step = WriteStep::Remove { path: path.into() };
        assert!(workdir.execute(&[step], 1).is_err(), "{path}");
    }
}

#[test]
fn a_folder_without_a_manifest_is_not_a_working_copy() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(Workdir::open(tmp.path()).is_err());
}

#[test]
fn a_folder_with_only_the_lock_counts_as_empty_and_a_working_copy_does_not() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("c.stet");
    fs::create_dir_all(dir.join(".stet")).unwrap();
    fs::write(dir.join(".stet/lock"), "").unwrap();
    drop(Workdir::create(&dir).unwrap());
    unpack(&tmp.path().join("done.stet"));
    let Err(crate::output::Failure::Core(error)) = Workdir::create(&tmp.path().join("done.stet"))
    else {
        panic!("a working copy is not an empty folder");
    };
    assert!(error.next().contains("--refresh"), "{}", error.next());
}

#[test]
fn a_discarded_unpack_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let fresh = tmp.path().join("fresh.stet");
    Workdir::create(&fresh).unwrap().discard(false);
    assert!(!fresh.exists());
    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    let workdir = Workdir::create(&empty).unwrap();
    fs::write(empty.join("document.md"), "p1 x").unwrap();
    workdir.discard(true);
    assert!(empty.is_dir());
    assert_eq!(fs::read_dir(&empty).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn a_complement_folder_that_is_a_link_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let real = tmp.path().join("real.stet");
    unpack(&real);
    let fake = tmp.path().join("fake.stet");
    fs::create_dir(&fake).unwrap();
    std::os::unix::fs::symlink(real.join(".stet"), fake.join(".stet")).unwrap();
    assert!(Workdir::open(&fake).is_err());
}

#[cfg(unix)]
#[test]
fn a_lock_or_packed_record_that_is_a_link_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().join("outside.txt");
    fs::write(&outside, "mine").unwrap();
    let dir = tmp.path().join("c.stet");
    unpack(&dir);
    let lock = dir.join(".stet/lock");
    let _ = fs::remove_file(&lock);
    std::os::unix::fs::symlink(tmp.path().join("planted"), &lock).unwrap();
    assert!(Workdir::open(&dir).is_err());
    assert!(
        !tmp.path().join("planted").exists(),
        "the lock is not created through the link"
    );
    fs::remove_file(&lock).unwrap();

    std::os::unix::fs::symlink(&outside, dir.join(".stet/packed")).unwrap();
    let (workdir, _) = Workdir::open(&dir).unwrap();
    assert!(workdir.record_packed(&"a".repeat(64)).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "mine");
}

#[test]
fn unpack_refuses_a_folder_that_is_not_empty() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("x.txt"), "x").unwrap();
    assert!(Workdir::create(tmp.path()).is_err());
}
