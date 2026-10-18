//! What an operation tells the transaction about identity, and `Tx::resolve`, the edit path of
//! resolution.
//!
//! The trees say which elements are gone and which are new. They cannot say which identity survives
//! a split or a merge, or why a block went, so the operations declare that here. A declaration takes
//! effect for `Tx::resolve` at once, through a small overlay of ref to element, and for the ref table
//! only at commit, when the audit applies the declarations to a clone of the table.

use super::errors::{internal, invalid, stale};
use super::tx::Tx;
use crate::api::error::StetError;
use crate::refs::{BlockKey, BlockRef, Claim, Reason};
use crate::wordml::NodeId;
use std::collections::HashMap;

/// Why a block's ref is retired by a declaration. A plain deletion declares nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Retired {
    /// The block was deleted. This is the default the audit records for an element that left the
    /// tree.
    Deleted,
    /// The block was an insertion that was rejected.
    Rejected,
}

impl From<Retired> for Reason {
    fn from(r: Retired) -> Reason {
        match r {
            Retired::Deleted => Reason::Deleted,
            Retired::Rejected => Reason::Rejected,
        }
    }
}

/// One declaration, in the order it was made.
#[derive(Clone, Debug)]
pub(crate) enum Intent {
    /// The ref of this block is retired, for this reason.
    Retire {
        /// The block.
        key: BlockKey,
        /// Why.
        reason: Retired,
    },
    /// The ref of `from` names the element `to`. The old element, if it is still in the tree, is
    /// minted as a new block.
    Rebind {
        /// The block the ref named.
        from: BlockKey,
        /// The element it names now.
        to: BlockKey,
    },
    /// Two blocks became one.
    Merge {
        /// The block whose ref is kept, named by its current element.
        survivor: BlockKey,
        /// The block whose ref becomes `Gone(MergedInto(survivor))`.
        absorbed: BlockKey,
        /// The element that is the merged block: either of the two, or a new one with no ref.
        into: BlockKey,
    },
}

/// What the declarations made so far mean for resolution inside the transaction.
#[derive(Default)]
pub(super) struct Overlay {
    /// Refs a declaration retired or merged away, with the reason shown to later operations.
    gone: HashMap<BlockRef, Reason>,
    /// Refs that now name another element: after a rebind, or as a merge survivor.
    moved: HashMap<BlockRef, BlockKey>,
    /// The ref of an element that a declaration moved a ref to.
    owner: HashMap<BlockKey, BlockRef>,
}

/// How a retirement reads to the operation that comes after it.
fn earlier(r: BlockRef, reason: Reason) -> String {
    match reason {
        Reason::Deleted => format!("{r} was deleted earlier in this batch."),
        Reason::Rejected => {
            format!("{r} was removed earlier in this batch, when its insertion was rejected.")
        }
        Reason::MergedInto(into) => format!("{r} was merged into {into} earlier in this batch."),
    }
}

impl Tx<'_> {
    /// The live ref of a block as this transaction's declarations leave it, or `None`.
    pub(super) fn ref_of(&self, key: &BlockKey) -> Option<BlockRef> {
        if let Some(r) = self.overlay.owner.get(key) {
            return Some(*r);
        }
        let r = self.base().refs.ref_of(key)?;
        let declared = self.overlay.gone.contains_key(&r) || self.overlay.moved.contains_key(&r);
        (!declared).then_some(r)
    }

    fn live_ref(&self, key: &BlockKey, what: &str) -> Result<BlockRef, StetError> {
        self.ref_of(key).ok_or_else(|| {
            internal(format!(
                "{what} was declared for a block in {} that has no live ref",
                key.part()
            ))
        })
    }

    /// Declares that a block's ref is retired for a reason other than the audit's default. A plain
    /// deletion declares nothing. The block's element must have left the tree by commit.
    pub(crate) fn retire(&mut self, key: &BlockKey, reason: Retired) -> Result<(), StetError> {
        let r = self.live_ref(key, "retire")?;
        self.overlay.gone.insert(r, reason.into());
        self.intents.push(Intent::Retire {
            key: key.clone(),
            reason,
        });
        Ok(())
    }

    /// Declares that the ref of `from` now names the element `to`, which must have no ref. It is for
    /// a split that keeps the original element for the second part: the old element is minted as a
    /// new block if it is still in the tree.
    pub(crate) fn rebind(&mut self, from: &BlockKey, to: BlockKey) -> Result<(), StetError> {
        let r = self.live_ref(from, "rebind")?;
        if self.ref_of(&to).is_some() {
            return Err(internal(format!(
                "{r} was rebound to an element that already has a ref"
            )));
        }
        self.overlay.owner.remove(from);
        self.overlay.moved.insert(r, to.clone());
        self.overlay.owner.insert(to.clone(), r);
        self.intents.push(Intent::Rebind {
            from: from.clone(),
            to,
        });
        Ok(())
    }

    /// Declares that two blocks became one, and which element is the result: the survivor's, the
    /// absorbed block's, or a new one. The survivor keeps its ref and names `into`, and the absorbed
    /// ref becomes `Gone(MergedInto(survivor))`. The element that is not `into` must be gone from the
    /// tree by commit.
    ///
    /// A merge of three blocks is two declarations, and the survivor is named by its current element
    /// each time, which `Tx::resolve` returns. If the absorbed ref was the survivor of an earlier
    /// merge in this batch, the refs that merge retired are now merged into this survivor.
    pub(crate) fn merge(
        &mut self,
        survivor: &BlockKey,
        absorbed: &BlockKey,
        into: BlockKey,
    ) -> Result<(), StetError> {
        let rs = self.live_ref(survivor, "merge")?;
        let ra = self.live_ref(absorbed, "merge")?;
        if rs == ra {
            return Err(internal(format!("{rs} was merged into itself")));
        }
        match self.ref_of(&into) {
            None => {}
            Some(r) if r == rs || r == ra => {}
            Some(other) => {
                return Err(internal(format!(
                    "{ra} was merged into {rs}, as an element that is {other}'s"
                )));
            }
        }
        for reason in self.overlay.gone.values_mut() {
            if *reason == Reason::MergedInto(ra) {
                *reason = Reason::MergedInto(rs);
            }
        }
        self.overlay.gone.insert(ra, Reason::MergedInto(rs));
        self.overlay.owner.remove(survivor);
        self.overlay.owner.remove(absorbed);
        self.overlay.moved.insert(rs, into.clone());
        self.overlay.owner.insert(into.clone(), rs);
        self.intents.push(Intent::Merge {
            survivor: survivor.clone(),
            absorbed: absorbed.clone(),
            into,
        });
        Ok(())
    }

    /// Records that an existing block changed in place. The scope guard reads the list, and commit
    /// stamps each live block with the new version. A block's own bytes are what count: all of a
    /// paragraph, and for a table or a row what `Story::own_parts` says.
    pub(crate) fn touch(&mut self, key: &BlockKey) {
        self.touch_log.push(key.clone());
        if self.touched_set.insert(key.clone()) {
            self.touched.push(key.clone());
        }
    }

    /// The blocks touched so far, in call order and without repeats.
    pub(crate) fn touched(&self) -> &[BlockKey] {
        &self.touched
    }

    /// The declarations made so far, in order.
    pub(crate) fn intents(&self) -> &[Intent] {
        &self.intents
    }

    /// The nodes of the blocks in `part` touched by calls after the first `from` calls, with
    /// repeats, and the number of calls so far. A refresh needs only the blocks touched since the
    /// story was last built, and a block touched again after that build is among them.
    pub(super) fn touched_since(&self, part: &str, from: usize) -> (Vec<NodeId>, usize) {
        let nodes = self
            .touch_log
            .get(from..)
            .unwrap_or_default()
            .iter()
            .filter(|k| &**k.part() == part)
            .map(BlockKey::node)
            .collect();
        (nodes, self.touch_log.len())
    }

    /// The nodes of the touched blocks that are in `part`.
    pub(super) fn touched_nodes(&self, part: &str) -> Vec<NodeId> {
        self.touched
            .iter()
            .filter(|k| &**k.part() == part)
            .map(BlockKey::node)
            .collect()
    }

    /// The block a ref names, for an operation of a batch. The batch must name the base token it was
    /// written against: `Claim::Implied` is refused with `invalid_payload`, because an edit that
    /// names no history could land on the wrong block of a forked session.
    ///
    /// The checks of `State::resolve` apply to the state the transaction began from. After them the
    /// transaction answers from its own declarations: a ref that an earlier operation retired or
    /// merged away is `stale_ref` with the reason, and one it rebound or kept as a survivor names its
    /// new element. A block that an earlier operation detached is `stale_ref` too, because a later
    /// operation would otherwise edit a node that is no longer in the document.
    pub(crate) fn resolve(&self, claim: Claim<'_>, token: &str) -> Result<BlockKey, StetError> {
        if matches!(claim, Claim::Implied) {
            return Err(invalid(
                "an operation must name the base token it was written against, and this one names none",
            )
            .with_next("Give the base token that the view showed, and write the batch again."));
        }
        let from_base = self.base().refs.resolve(claim, token)?;
        let r = BlockRef::parse(token)?;
        if let Some(reason) = self.overlay.gone.get(&r) {
            return Err(stale(earlier(r, *reason)));
        }
        let key = self.overlay.moved.get(&r).cloned().unwrap_or(from_base);
        self.check_attached(r, &key)?;
        Ok(key)
    }

    /// Requires that the element is still in the transaction's tree, and in one of the document's stories: a header or a footer
    /// that no section names any more, after an earlier operation of the batch accepted the deletion of the section break
    /// that named it, holds no block an operation can name, and the commit retires its refs.
    fn check_attached(&self, r: BlockRef, key: &BlockKey) -> Result<(), StetError> {
        let view = self.parts();
        if !view.is_attached(key)? {
            return Err(stale(format!("{r} was deleted earlier in this batch.")));
        }
        let in_story = match view.id(key.part()) {
            Some(part) => view.is_story(part)?,
            None => false,
        };
        if !in_story {
            return Err(stale(format!(
                "{r} is in {}, which no section of the document uses after an earlier operation of this batch.",
                key.part()
            )));
        }
        Ok(())
    }
}
