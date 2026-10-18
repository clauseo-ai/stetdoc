//! The entries that write inside paragraphs: a paired paragraph, a split and a merge (design `compare.md`, 6.1 and 5.5).

use super::build::Builder;
use super::import::Cx;
use super::plan::StoryPlan;
use super::plan_para::{MergePlan, ParaEdits, SplitPlan};
use crate::api::error::{ErrorCode, StetError};
use crate::revisions::redline_types::{ParaRedline, RedlineEdit, RunSnapshot, Snapshot};
use crate::wordml::{BlockKey, NodeId};
use std::sync::Arc;

fn key(part: &Arc<str>, n: NodeId) -> BlockKey {
    BlockKey::new(Arc::clone(part), n)
}

impl Builder<'_, '_, '_> {
    /// The plan of one paragraph as the compiler takes it: the deleted text and the old properties are copied from the original.
    fn para_redline(&mut self, sp: &StoryPlan, pe: &ParaEdits) -> Result<ParaRedline, StetError> {
        let cx = Cx {
            src: &sp.part_a,
            dst: &sp.part_b,
        };
        let a = pe.a.ok_or_else(|| {
            StetError::new(ErrorCode::Internal, "a paragraph plan has no original")
        })?;
        let mut edits = Vec::with_capacity(pe.edits.len());
        for e in &pe.edits {
            let deleted = match &e.deleted {
                Some(r) => self
                    .imp
                    .fragment(&mut self.c.import_writer(), cx, a, r.clone())?,
                None => Vec::new(),
            };
            edits.push(RedlineEdit {
                insert: e.insert.clone(),
                deleted,
            });
        }
        let mut fmt = Vec::with_capacity(pe.fmt.len());
        for f in &pe.fmt {
            fmt.push(RunSnapshot {
                range: f.b.clone(),
                snapshot: self
                    .imp
                    .run_snapshot(&mut self.c.import_writer(), cx, a, f.a.start)?,
            });
        }
        let snapshot = if pe.ppr || pe.mark {
            let (own, mark) = self
                .imp
                .paragraph_snapshot(&mut self.c.import_writer(), cx, a)?;
            Some(Snapshot {
                ppr: pe.ppr.then_some(own),
                mark: pe.mark.then_some(mark),
            })
        } else {
            None
        };
        Ok(ParaRedline {
            edits,
            fmt,
            snapshot,
            mark_inserted: pe.mark_inserted,
            mark_deleted: false,
            source: Some(key(&sp.part_a, a)),
        })
    }

    /// A paired paragraph that differs.
    pub(super) fn edit(&mut self, sp: &StoryPlan, pe: &ParaEdits) -> Result<(), StetError> {
        let (Some(a), Some(b)) = (pe.a, pe.b) else {
            return Ok(());
        };
        let nothing =
            pe.edits.is_empty() && pe.fmt.is_empty() && !pe.ppr && !pe.mark && !pe.mark_inserted;
        if nothing {
            return Ok(());
        }
        let k = key(&sp.part_b, b);
        if self.holds_nested_paragraph(&k) {
            self.exclude(
                vec![k.clone()],
                vec![key(&sp.part_a, a)],
                "not_restored",
                "the paragraph holds another paragraph, which no tracked change can carry",
            );
        }
        let plan = match self.para_redline(sp, pe) {
            Ok(p) => p,
            Err(e) => {
                self.replace_whole(sp, &[a], &[b], &e.to_string());
                return Ok(());
            }
        };
        if let Err(e) = self.c.precheck_redline(&k, &plan) {
            self.replace_whole(sp, &[a], &[b], &e.to_string());
            return Ok(());
        }
        match self.c.redline_paragraph(&k, &plan) {
            Ok(fx) => {
                self.record(&fx);
                self.count_edit();
            }
            Err(e) => self.broken(k, &e),
        }
        Ok(())
    }

    /// One original paragraph that became several: the first parts have their marks inserted, and each part has its own edits.
    pub(super) fn split(&mut self, sp: &StoryPlan, s: &SplitPlan) -> Result<(), StetError> {
        let bs: Vec<NodeId> = s.parts.iter().filter_map(|p| p.b).collect();
        let keys: Vec<BlockKey> = bs.iter().map(|&n| key(&sp.part_b, n)).collect();
        let mut plans = Vec::with_capacity(s.parts.len());
        for part in &s.parts {
            match self.para_redline(sp, part) {
                Ok(p) => plans.push(p),
                Err(e) => {
                    self.replace_whole(sp, &[s.a], &bs, &e.to_string());
                    return Ok(());
                }
            }
        }
        for (k, p) in keys.iter().zip(&plans) {
            if let Err(e) = self.c.precheck_redline(k, p) {
                self.replace_whole(sp, &[s.a], &bs, &e.to_string());
                return Ok(());
            }
        }
        for (k, p) in keys.iter().zip(&plans) {
            match self.c.redline_paragraph(k, p) {
                Ok(fx) => self.record(&fx),
                Err(e) => self.broken(k.clone(), &e),
            }
        }
        self.count_split();
        Ok(())
    }

    /// Several original paragraphs that became one: the revised paragraph is cut where they met, untracked, and the marks between
    /// the parts are deleted, tracked, so that rejecting gives the paragraphs back.
    pub(super) fn merge(&mut self, sp: &StoryPlan, m: &MergePlan) -> Result<(), StetError> {
        let originals: Vec<NodeId> = m.parts.iter().filter_map(|p| p.a).collect();
        let b = key(&sp.part_b, m.b);
        let mut plans = Vec::with_capacity(m.parts.len());
        if m.parts.len() != m.cuts.len() + 1 {
            self.replace_whole(
                sp,
                &originals,
                &[m.b],
                "a merge whose parts and cuts do not agree",
            );
            return Ok(());
        }
        for part in &m.parts {
            match self.para_redline(sp, part) {
                Ok(p) => plans.push(p),
                Err(e) => {
                    self.replace_whole(sp, &originals, &[m.b], &e.to_string());
                    return Ok(());
                }
            }
        }
        let mut starts = vec![0usize];
        starts.extend(m.cuts.iter().copied());
        let mut check = self.c.precheck_split(&b, &m.cuts);
        for (p, &start) in plans.iter().zip(&starts) {
            if check.is_ok() {
                check = self.c.precheck_redline(&b, &p.shifted(start));
            }
        }
        if let Err(e) = check {
            self.replace_whole(sp, &originals, &[m.b], &e.to_string());
            return Ok(());
        }
        let mut keys = vec![b.clone()];
        let mut cur = b.clone();
        let mut prev = 0usize;
        for &cut in &m.cuts {
            let done = self.c.split_block_untracked(&cur, cut.saturating_sub(prev));
            match done {
                Ok(fx) => {
                    let Some(new) = fx.new_blocks.first().cloned() else {
                        self.broken(
                            b,
                            &StetError::new(ErrorCode::Internal, "a split made no block"),
                        );
                        return Ok(());
                    };
                    self.record(&fx);
                    keys.push(new.clone());
                    cur = new;
                    prev = cut;
                }
                Err(e) => {
                    self.broken(b, &e);
                    return Ok(());
                }
            }
        }
        for w in keys.windows(2) {
            if let [x, y] = w {
                match self.c.merge_blocks(x, y) {
                    Ok(fx) => self.record(&fx),
                    Err(e) => self.broken(x.clone(), &e),
                }
            }
        }
        for (k, p) in keys.iter().zip(&plans) {
            match self.c.redline_paragraph(k, p) {
                Ok(fx) => self.record(&fx),
                Err(e) => self.broken(k.clone(), &e),
            }
        }
        self.count_merge();
        Ok(())
    }
}
