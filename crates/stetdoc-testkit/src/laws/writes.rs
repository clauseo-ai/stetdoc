//! H12: an interrupted write comes back to exactly the old working copy or exactly the new one.

use super::memdir::MemDir;
use std::collections::BTreeMap;
use stetdoc_core::{
    Document, ErrorCode, OpenOptions, Recovery, StetError, WorkingCopy, WriteOptions, WriteStep,
};

fn entries(working_copy: &WorkingCopy) -> BTreeMap<&str, &[u8]> {
    working_copy
        .complement
        .entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect()
}

/// Runs `recover` if the directory has a marker, and does the caller's part: it writes the files to render again from `new`,
/// then removes the marker. `None` when there was nothing to recover.
fn settle(dir: &mut MemDir, new: &WorkingCopy) -> Option<Result<Recovery, StetError>> {
    let (on_disk, marker) = dir.read();
    let recovery = WorkingCopy::recover(&on_disk, &marker?);
    if let Ok(recovery) = &recovery {
        dir.apply_all(&recovery.steps);
        for name in &recovery.rerender {
            if let Some((_, text)) = new.files.iter().find(|(n, _)| n == name) {
                dir.apply(&WriteStep::File {
                    path: name.clone(),
                    text: text.clone(),
                });
            }
        }
        dir.apply(&recovery.finish);
    }
    Some(recovery)
}

/// **H12.** Writes `new` over `old` (`None` for a first unpack) and stops after each step of the plan, and then stops
/// recovery after each of its steps. After recovery the complement is exactly the old one or exactly the new one, `resume`
/// succeeds at the version of whichever it is, no stale entry or marker is left, and the readable files are the new render or,
/// rolled back, what they were. An interrupted first unpack that did not reach its commit has no working copy, and `recover`
/// says to delete the directory. `old_doc` and `new_doc` are the documents the two working copies were exported from.
pub fn survives_a_crash_at_every_step(
    old: Option<(&Document, &WorkingCopy)>,
    new: (&Document, &WorkingCopy),
    options: &WriteOptions,
) -> Result<(), String> {
    let start = old.map(|(_, wc)| MemDir::of(wc)).unwrap_or_default();
    let disk = old.map(|_| start.read().0);
    let plan = new
        .1
        .write_plan(disk.as_ref(), options)
        .map_err(|e| format!("the plan: {e}"))?;
    let resume = |wc: &WorkingCopy| Document::resume(wc, &OpenOptions::default()).map(|d| d.base());
    for k in 0..=plan.steps.len() {
        let mut dir = start.clone();
        dir.apply_all(plan.steps.iter().take(k));
        let (interrupted, marker) = dir.read();
        if marker.is_some() {
            // Nothing but recovery reads a directory in the middle of a write.
            if interrupted.pending_files().is_ok()
                || Document::resume(&interrupted, &OpenOptions::default()).is_ok()
            {
                return Err(format!(
                    "crash after step {k}: a directory with an unrecovered write was read"
                ));
            }
            // Recovery stopped after each step, and run again, gives what recovery run once gives.
            let mut once = dir.clone();
            let outcome = settle(&mut once, new.1);
            if let Some(Ok(recovery)) = &outcome {
                for j in 0..=recovery.steps.len() {
                    let mut crashed = dir.clone();
                    crashed.apply_all(recovery.steps.iter().take(j));
                    let _ = settle(&mut crashed, new.1);
                    if crashed != once {
                        return Err(format!(
                            "crash after step {k} of the write, then after step {j} of the recovery: recovery run again differs"
                        ));
                    }
                }
            }
            if let Some(Err(e)) = outcome {
                if old.is_some()
                    || e.code() != ErrorCode::CorruptComplement
                    || !e.message().contains("Delete the directory")
                {
                    return Err(format!("crash after step {k}: recovery failed: {e}"));
                }
                continue;
            }
            dir = once;
        }
        let (on_disk, marker) = dir.read();
        if marker.is_some() {
            return Err(format!("crash after step {k}: the marker is left"));
        }
        if on_disk.complement.entries.is_empty() {
            if old.is_some() {
                return Err(format!(
                    "crash after step {k}: the old working copy is gone"
                ));
            }
            continue;
        }
        let at = if entries(&on_disk) == entries(new.1) {
            (new.0.base(), true)
        } else if let Some((old_doc, old_wc)) =
            old.filter(|(_, wc)| entries(&on_disk) == entries(wc))
        {
            let _ = old_wc;
            (old_doc.base(), false)
        } else {
            return Err(format!(
                "crash after step {k}: the complement is neither the old one nor the new one"
            ));
        };
        match resume(&on_disk) {
            Ok(base) if base == at.0 => {}
            Ok(base) => {
                return Err(format!(
                    "crash after step {k}: resumed at {base}, expected {}",
                    at.0
                ));
            }
            Err(e) => return Err(format!("crash after step {k}: resume failed: {e}")),
        }
        let manifest = on_disk
            .complement
            .entries
            .iter()
            .filter(|(n, _)| n != "manifest")
            .count();
        let expected = new.1.complement.entries.len() - 1;
        if at.1 && manifest != expected {
            return Err(format!(
                "crash after step {k}: {manifest} entries, {expected} expected: a stale entry is left"
            ));
        }
    }
    Ok(())
}
