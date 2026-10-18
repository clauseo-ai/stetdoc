//! The ref table: which block each ref names, and what became of the refs that are gone.
//!
//! A ref follows its block's element, and a number is never reused, so the table is a dense list of
//! slots per kind. Each ref is `Live` while its element is in the document, and `Gone` for good
//! after. Every live slot records the version its block last changed in, and every gone slot the
//! version that retired it, which is what lets a batch written for an older version be refused for
//! a block that has changed since. The table also keeps the lineage: a hash chain with one link per
//! committed version.
//!
//! Only `state` calls the methods that change the table, at commit. Everyone else resolves refs.

use super::base::{BaseToken, Claim, Lineage, NamespaceId, NamespaceOrigin};
use super::errors::{internal, limit, not_found, stale};
use super::layout::Layout;
use super::token::{BlockRef, RefKind};
use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::wordml::{BlockKey, BlockKind, NodeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Why a ref is gone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Reason {
    /// Its element left the document: an untracked deletion, or an accepted tracked one.
    Deleted,
    /// It was merged into another block, which kept the ref.
    MergedInto(BlockRef),
    /// It was an insertion that was rejected.
    Rejected,
}

/// What became of one ref number.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Slot {
    /// The ref names an element that is in the document. `changed` is the version the block last
    /// changed in, which is the version that minted it until something touches it.
    Live {
        part: u16,
        node: NodeId,
        changed: u32,
    },
    /// The element left the document in version `at`.
    Gone { reason: Reason, at: u32 },
}

/// The refs of one document, and the history of its versions.
#[derive(Clone, Debug)]
pub(crate) struct RefTable {
    namespace: NamespaceId,
    origin: NamespaceOrigin,
    /// The lineage of versions 1 to the current one. Version 0 is the namespace, so the current
    /// version is the length of this list.
    history: Vec<Lineage>,
    /// The part names, in first-use order.
    parts: Vec<Arc<str>>,
    part_ids: HashMap<Arc<str>, u16>,
    /// By kind: the slot of ref number `n` is at index `n - 1`.
    slots: [Vec<Slot>; 3],
    by_key: HashMap<(u16, NodeId), BlockRef>,
    max_refs: u32,
}

/// The refs that the merges of one commit retired. The audit keeps one for the commit it is
/// building and gives it to `merge` and `tombstone`, so that a ref absorbed in turn can retarget
/// them to the final survivor, and a survivor that is removed in the same commit can give them its
/// reason. It belongs to the commit and not to the table: the table does not keep it, copy it or
/// save it.
#[derive(Debug, Default)]
pub(crate) struct Merged(Vec<BlockRef>);

impl RefTable {
    pub(super) fn empty(
        namespace: NamespaceId,
        origin: NamespaceOrigin,
        max_refs: u32,
    ) -> RefTable {
        RefTable {
            namespace,
            origin,
            history: Vec::new(),
            parts: Vec::new(),
            part_ids: HashMap::new(),
            slots: [Vec::new(), Vec::new(), Vec::new()],
            by_key: HashMap::new(),
            max_refs,
        }
    }

    /// Assigns refs to every block of a freshly opened document, story by story and in document
    /// order, so the same bytes always give the same numbers. The table starts at version 0.
    pub(crate) fn open(
        namespace: NamespaceId,
        origin: NamespaceOrigin,
        layout: &Layout,
        limits: &Limits,
    ) -> Result<RefTable, StetError> {
        let mut table = RefTable::empty(namespace, origin, limits.max_refs);
        for story in &layout.stories {
            table.intern(&story.part)?;
            for block in &story.blocks {
                table.allocate(&block.key, block.kind, 0)?;
            }
        }
        Ok(table)
    }

    // ---- reading ------------------------------------------------------------------------

    /// The namespace.
    pub(crate) fn namespace(&self) -> NamespaceId {
        self.namespace
    }

    /// How the namespace came to be.
    pub(crate) fn origin(&self) -> &NamespaceOrigin {
        &self.origin
    }

    /// The state version: the number of commits.
    pub(crate) fn version(&self) -> u32 {
        // The history is capped by `max_refs`, which is a `u32`.
        u32::try_from(self.history.len()).unwrap_or(u32::MAX)
    }

    /// The lineage at a version: the namespace at 0, and the chain after.
    pub(crate) fn lineage_at(&self, version: u32) -> Option<Lineage> {
        match version {
            0 => Some(Lineage::root(&self.namespace)),
            v => self
                .history
                .get(usize::try_from(v).ok()?.checked_sub(1)?)
                .copied(),
        }
    }

    /// The lineage now.
    pub(crate) fn lineage(&self) -> Lineage {
        self.history
            .last()
            .copied()
            .unwrap_or_else(|| Lineage::root(&self.namespace))
    }

    /// The base token of the current state.
    pub(crate) fn base_token(&self) -> BaseToken {
        BaseToken::new(self.namespace, self.version(), self.lineage())
    }

    /// The lineage of every version after 0, oldest first.
    pub(super) fn history(&self) -> &[Lineage] {
        &self.history
    }

    /// How many refs of a kind have been minted, live and gone.
    pub(crate) fn count(&self, kind: RefKind) -> u32 {
        self.slots
            .get(kind.index())
            .map_or(0, |s| u32::try_from(s.len()).unwrap_or(u32::MAX))
    }

    /// How many refs have been minted in all.
    pub(super) fn total(&self) -> u64 {
        self.slots.iter().map(|s| s.len() as u64).sum()
    }

    pub(super) fn slot(&self, r: BlockRef) -> Option<&Slot> {
        let index = usize::try_from(r.number()).ok()?.checked_sub(1)?;
        self.slots.get(r.kind().index())?.get(index)
    }

    fn slot_mut(&mut self, r: BlockRef) -> Option<&mut Slot> {
        let index = usize::try_from(r.number()).ok()?.checked_sub(1)?;
        self.slots.get_mut(r.kind().index())?.get_mut(index)
    }

    pub(super) fn part_name(&self, part: u16) -> Option<&Arc<str>> {
        self.parts.get(usize::from(part))
    }

    /// The element a live ref names, or `None` if the ref is gone or was never minted.
    pub(crate) fn key_of(&self, r: BlockRef) -> Option<BlockKey> {
        match self.slot(r)? {
            Slot::Live { part, node, .. } => {
                Some(BlockKey::new(Arc::clone(self.part_name(*part)?), *node))
            }
            Slot::Gone { .. } => None,
        }
    }

    /// Why a ref is gone, or `None` if it is live or was never minted.
    pub(crate) fn gone_reason(&self, r: BlockRef) -> Option<Reason> {
        match self.slot(r)? {
            Slot::Gone { reason, .. } => Some(*reason),
            Slot::Live { .. } => None,
        }
    }

    /// The ref of a block, if it has a live one.
    pub(crate) fn ref_of(&self, key: &BlockKey) -> Option<BlockRef> {
        let part = *self.part_ids.get(key.part())?;
        self.by_key.get(&(part, key.node())).copied()
    }

    /// The live refs of one part, in slot order (paragraphs, then tables, then rows, each by
    /// number), with the element each names.
    pub(crate) fn live_in(&self, part: &str) -> Vec<(BlockRef, NodeId)> {
        let Some(&id) = self.part_ids.get(part) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for kind in RefKind::ALL {
            let slots = self.slots.get(kind.index()).map_or(&[][..], Vec::as_slice);
            for (i, slot) in slots.iter().enumerate() {
                if let Slot::Live { part, node, .. } = slot
                    && *part == id
                    && let Some(r) = u32::try_from(i + 1)
                        .ok()
                        .and_then(|n| BlockRef::new(kind, n))
                {
                    out.push((r, *node));
                }
            }
        }
        out
    }

    // ---- resolution ----------------------------------------------------------------------

    /// Checks that a batch's base token is for this document and this history.
    fn check_base(&self, base: &BaseToken) -> Result<(), StetError> {
        if base.namespace() != self.namespace {
            return Err(stale(format!(
                "This batch was written for another document (namespace {}, not {}). Refs do \
                 not carry over when a file is reopened. View the document again.",
                base.namespace(),
                self.namespace
            )));
        }
        if base.version() > self.version() {
            return Err(stale(format!(
                "This batch was written for version {} of a document that is at version {}, so it \
                 comes from a different copy. View the document again.",
                base.version(),
                self.version()
            )));
        }
        if self.lineage_at(base.version()) != Some(base.lineage()) {
            return Err(stale(
                "This batch was written for a different history of this document: another \
                 working copy, or another session opened from the same file. Its refs may name \
                 other blocks here. View the document again.",
            ));
        }
        Ok(())
    }

    /// The block a ref names, if the batch's claim holds. The checks run in this order, and the
    /// first failure is the answer: the claim's namespace, version and lineage (`stale_ref`), the
    /// token (`invalid_payload`), whether the number was ever minted (`not_found`), whether the ref
    /// is gone (`stale_ref`), and whether the block changed after the claim's version (`stale_ref`).
    pub(crate) fn resolve(&self, claim: Claim<'_>, token: &str) -> Result<BlockKey, StetError> {
        if let Claim::Base(base) = claim {
            self.check_base(base)?;
        }
        let r = BlockRef::parse(token)?;
        match self.slot(r) {
            None => Err(self.never_minted(r)),
            Some(Slot::Gone { reason, at }) => Err(self.gone(r, *reason, *at)),
            Some(Slot::Live {
                part,
                node,
                changed,
            }) => {
                if let Claim::Base(base) = claim
                    && *changed > base.version()
                {
                    return Err(stale(format!(
                        "{r} was changed in version {changed}, after version {}, which this batch \
                         was written for. View it again.",
                        base.version()
                    ))
                    .with_next(
                        "View the block again, then rewrite the operation from what it shows.",
                    ));
                }
                let name = self
                    .part_name(*part)
                    .ok_or_else(|| internal("a live ref names a part the table does not know"))?;
                Ok(BlockKey::new(Arc::clone(name), *node))
            }
        }
    }

    fn never_minted(&self, r: BlockRef) -> StetError {
        let count = self.count(r.kind());
        let kind = r.kind();
        let range = if count == 0 {
            format!("This document has no {}.", kind.noun())
        } else {
            let first = BlockRef::new(kind, 1)
                .map(|f| f.to_string())
                .unwrap_or_default();
            let last = BlockRef::new(kind, count)
                .map(|l| l.to_string())
                .unwrap_or_default();
            format!(
                "This document's {} run from {first} to {last}.",
                kind.noun()
            )
        };
        not_found(format!("{r} does not exist. {range}"))
    }

    fn gone(&self, r: BlockRef, reason: Reason, at: u32) -> StetError {
        let message = match reason {
            Reason::Deleted => format!("{r} was deleted in version {at}."),
            Reason::Rejected => {
                format!("{r} was removed in version {at}, when its insertion was rejected.")
            }
            Reason::MergedInto(target) => {
                let removed = matches!(self.slot(target), Some(Slot::Gone { .. }));
                if removed {
                    format!(
                        "{r} was merged into {target} in version {at}, and {target} has since \
                         been removed."
                    )
                } else {
                    format!("{r} was merged into {target} in version {at}.")
                }
            }
        };
        stale(message)
    }

    // ---- changing the table, at commit ------------------------------------------------------

    fn intern(&mut self, name: &Arc<str>) -> Result<u16, StetError> {
        if let Some(&id) = self.part_ids.get(name) {
            return Ok(id);
        }
        let id = u16::try_from(self.parts.len())
            .map_err(|_| limit("the ref table names too many parts"))?;
        self.parts.push(Arc::clone(name));
        self.part_ids.insert(Arc::clone(name), id);
        Ok(id)
    }

    /// Mints the next ref of `kind` for a block, in version `at`. A block has at most one live ref,
    /// and the table holds at most `Limits::max_refs` refs.
    pub(crate) fn allocate(
        &mut self,
        key: &BlockKey,
        kind: BlockKind,
        at: u32,
    ) -> Result<BlockRef, StetError> {
        if self.total() >= u64::from(self.max_refs) {
            return Err(limit(format!(
                "this document has had {} refs, the most a ref namespace may hold",
                self.max_refs
            )));
        }
        let part = self.intern(key.part())?;
        if self.by_key.contains_key(&(part, key.node())) {
            return Err(internal("a block was given a second ref"));
        }
        let kind = RefKind::of(kind);
        let slots = self
            .slots
            .get_mut(kind.index())
            .ok_or_else(|| internal("a kind of ref has no slots"))?;
        let number = u32::try_from(slots.len() + 1).map_err(|_| limit("too many refs"))?;
        let r = BlockRef::new(kind, number).ok_or_else(|| internal("ref number 0"))?;
        slots.push(Slot::Live {
            part,
            node: key.node(),
            changed: at,
        });
        self.by_key.insert((part, key.node()), r);
        Ok(r)
    }

    /// Retires a live ref in version `at`. Its number is never used again. The refs that merges of
    /// this commit (`merged`) retired into it are gone for the same reason.
    pub(crate) fn tombstone(
        &mut self,
        r: BlockRef,
        reason: Reason,
        at: u32,
        merged: &Merged,
    ) -> Result<(), StetError> {
        let Some(Slot::Live { part, node, .. }) = self.slot(r).copied() else {
            return Err(internal(format!("{r} was retired but is not a live ref")));
        };
        self.by_key.remove(&(part, node));
        if let Some(slot) = self.slot_mut(r) {
            *slot = Slot::Gone { reason, at };
        }
        // Refs that merges of this commit retired into `r` are gone for the same reason, since the
        // block they were merged into was removed in the same commit. So no tombstone of one version
        // points at another of the same version.
        for earlier in &merged.0 {
            if let Some(Slot::Gone {
                reason: rr,
                at: when,
            }) = self.slot_mut(*earlier)
                && *when == at
                && *rr == Reason::MergedInto(r)
            {
                *rr = reason;
            }
        }
        Ok(())
    }

    /// Makes a live ref name another element, which must have no ref. The element it named, if it
    /// is still in the document, has none now, and the audit mints one for it.
    pub(crate) fn rebind(&mut self, r: BlockRef, to: &BlockKey) -> Result<(), StetError> {
        let Some(Slot::Live { part, node, .. }) = self.slot(r).copied() else {
            return Err(internal(format!("{r} was rebound but is not a live ref")));
        };
        let target = self.intern(to.part())?;
        if self.by_key.contains_key(&(target, to.node())) {
            return Err(internal(format!(
                "{r} was rebound to an element that already has a ref"
            )));
        }
        self.by_key.remove(&(part, node));
        self.by_key.insert((target, to.node()), r);
        if let Some(Slot::Live { part, node, .. }) = self.slot_mut(r) {
            *part = target;
            *node = to.node();
        }
        Ok(())
    }

    /// Two blocks became one, in version `at`. The survivor's ref names `into`, which is the
    /// survivor's element, the absorbed block's element or an element with no ref, and the absorbed
    /// ref is retired as merged into the survivor. Which element is left in the document is the
    /// audit's to check. `merged` is the commit's list of refs that merges retired: a ref that an
    /// earlier merge of the commit retired into `absorbed` is retargeted to the survivor, so that no
    /// chain of merges forms inside one version, and `absorbed` joins the list.
    pub(crate) fn merge(
        &mut self,
        survivor: BlockRef,
        absorbed: BlockRef,
        into: &BlockKey,
        at: u32,
        merged: &mut Merged,
    ) -> Result<(), StetError> {
        let (
            Some(Slot::Live {
                part: sp, node: sn, ..
            }),
            Some(Slot::Live {
                part: ap, node: an, ..
            }),
        ) = (self.slot(survivor).copied(), self.slot(absorbed).copied())
        else {
            return Err(internal(format!(
                "{absorbed} was merged into {survivor}, and one of them is not a live ref"
            )));
        };
        if survivor == absorbed {
            return Err(internal(format!("{survivor} was merged into itself")));
        }
        let target = self.intern(into.part())?;
        match self.by_key.get(&(target, into.node())) {
            None => {}
            Some(r) if *r == survivor || *r == absorbed => {}
            Some(other) => {
                return Err(internal(format!(
                    "{absorbed} was merged into {survivor}, as an element that is {other}'s"
                )));
            }
        }
        self.by_key.remove(&(ap, an));
        // A ref that an earlier merge of this commit retired into `absorbed` is now merged into the
        // survivor, so that no chain of merges forms inside one version.
        for earlier in &merged.0 {
            if let Some(Slot::Gone { reason, at: when }) = self.slot_mut(*earlier)
                && *when == at
                && *reason == Reason::MergedInto(absorbed)
            {
                *reason = Reason::MergedInto(survivor);
            }
        }
        if let Some(slot) = self.slot_mut(absorbed) {
            *slot = Slot::Gone {
                reason: Reason::MergedInto(survivor),
                at,
            };
        }
        merged.0.push(absorbed);
        self.by_key.remove(&(sp, sn));
        self.by_key.insert((target, into.node()), survivor);
        if let Some(Slot::Live {
            part,
            node,
            changed,
        }) = self.slot_mut(survivor)
        {
            *part = target;
            *node = into.node();
            *changed = at;
        }
        Ok(())
    }

    /// Records that a live block changed in version `at`. A ref that is gone is left alone.
    pub(crate) fn touch(&mut self, key: &BlockKey, at: u32) {
        if let Some(r) = self.ref_of(key)
            && let Some(Slot::Live { changed, .. }) = self.slot_mut(r)
        {
            *changed = (*changed).max(at);
        }
    }

    /// Ends a commit: appends the next link of the lineage, from a digest of what the commit
    /// changed, and raises the version. A history that would pass `Limits::max_refs` is
    /// `limit_exceeded`.
    pub(crate) fn advance(&mut self, changes: &[u8; 32]) -> Result<BaseToken, StetError> {
        let next = self.history.len() + 1;
        if next as u64 > u64::from(self.max_refs) {
            return Err(limit(format!(
                "this document has been committed to {} times, the most a ref namespace may hold",
                self.max_refs
            )));
        }
        let version = u32::try_from(next).map_err(|_| limit("too many versions"))?;
        let lineage = self.lineage().next(version, changes);
        self.history.push(lineage);
        Ok(self.base_token())
    }

    // ---- what `restore` builds ---------------------------------------------------------------

    /// Builds a table from parts that `restore` has checked: the slots per kind, the lineage and
    /// the part names. It makes the reverse map and the part numbers.
    pub(super) fn from_parts(
        namespace: NamespaceId,
        origin: NamespaceOrigin,
        history: Vec<Lineage>,
        part_names: &[Arc<str>],
        slots: [Vec<Slot>; 3],
        max_refs: u32,
    ) -> Result<RefTable, StetError> {
        let mut table = RefTable::empty(namespace, origin, max_refs);
        for name in part_names {
            table.intern(name)?;
        }
        table.history = history;
        for kind in RefKind::ALL {
            for (i, slot) in slots
                .get(kind.index())
                .map_or(&[][..], |s| s)
                .iter()
                .enumerate()
            {
                if let Slot::Live { part, node, .. } = slot {
                    let r = u32::try_from(i + 1)
                        .ok()
                        .and_then(|n| BlockRef::new(kind, n))
                        .ok_or_else(|| internal("a ref number is out of range"))?;
                    table.by_key.insert((*part, *node), r);
                }
            }
        }
        table.slots = slots;
        Ok(table)
    }

    /// The number of live refs, which is the size of the reverse map.
    #[cfg(test)]
    pub(super) fn by_key_len(&self) -> usize {
        self.by_key.len()
    }

    /// Every slot, by kind, for `persist`.
    pub(super) fn slots(&self) -> &[Vec<Slot>; 3] {
        &self.slots
    }
}
