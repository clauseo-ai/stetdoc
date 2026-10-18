//! `Tx::commit`: audit, size check, lineage, and the next `State`.
//!
//! Everything that can fail happens before the next `State` exists: the scans, the declarations
//! and their composition, the table cap, the size of every edited part and the history cap. The step
//! that installs the result only moves values and cannot fail. So commit is atomic by construction:
//! on any error nothing was published, and the base `State` was never written to.

use super::State;
use super::audit::{self, Audited};
use super::derived::{Dep, Derived, Generation};
use super::errors::{internal, limit};
use super::failpoint;
use super::parts::{PartId, PartSlot};
use super::tx::{Added, Fresh, Tx};
use crate::api::error::StetError;
use crate::refs::{BaseToken, BlockKey, BlockRef, Hasher, Reason, sha256};
use std::cell::RefCell;
use std::collections::BTreeSet;

/// What a commit did to the refs, for the receipt.
#[derive(Clone, Debug)]
pub(crate) struct Committed {
    /// The token of the new state.
    pub(crate) base: BaseToken,
    /// The refs minted, in the order they were minted: story order, then document order.
    pub(crate) minted: Vec<(BlockKey, BlockRef)>,
    /// The refs retired because their element left the document.
    pub(crate) retired: Vec<(BlockRef, Reason)>,
    /// The refs that now name another element, with that element.
    pub(crate) rebound: Vec<(BlockRef, BlockKey)>,
    /// The merges: survivor, then absorbed.
    pub(crate) merged: Vec<(BlockRef, BlockRef)>,
    /// The refs that the merges retired, each with the reason the table gives it at the end of the commit: merged into the
    /// survivor (the last one, when a later merge of the commit absorbed that survivor too), or the survivor's own reason when
    /// the commit removed the survivor as well. `retired` does not hold them.
    pub(crate) absorbed: Vec<(BlockRef, Reason)>,
}

impl Committed {
    /// The report of a commit that changed nothing: the version did not move.
    fn unchanged(base: BaseToken) -> Committed {
        Committed {
            base,
            minted: Vec::new(),
            retired: Vec::new(),
            rebound: Vec::new(),
            merged: Vec::new(),
            absorbed: Vec::new(),
        }
    }
}

/// The SHA-256 of the bytes of every part the commit changed, in part order: the edited trees as
/// they would be saved, and the parts `add_part` added or rewrote. Each edited part is serialized
/// once and its size checked against `Limits::max_entry_bytes`, so that `save` cannot fail after a
/// successful edit.
fn hash_changed(tx: &Tx<'_>, edited: &[PartId]) -> Result<Vec<(PartId, [u8; 32])>, StetError> {
    failpoint::here("commit:size")?;
    let max = tx.base().limits.max_entry_bytes;
    let mut ids: BTreeSet<PartId> = edited.iter().copied().collect();
    if let Some(added) = &tx.added {
        ids.extend(added.fresh.keys().copied());
    }
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let name = tx
            .index
            .name(id)
            .ok_or_else(|| internal("a changed part has no name"))?;
        let digest = match tx.staged.get(&id).filter(|s| s.changed()) {
            Some(staged) => {
                let bytes = staged.tree.serialize();
                let len = bytes.len() as u64;
                if len > max {
                    return Err(limit(format!(
                        "{name} would be {len} bytes after this edit, over the limit of {max}"
                    )));
                }
                failpoint::here("commit:hash")?;
                sha256(&bytes)
            }
            None => {
                let bytes = tx.parts().package().part(name)?.ok_or_else(|| {
                    internal(format!("{name} was added and is not in the package"))
                })?;
                failpoint::here("commit:hash")?;
                sha256(&bytes)
            }
        };
        out.push((id, digest));
    }
    Ok(out)
}

/// The digest of a commit, which the next link of the lineage hashes together with the one before:
/// what changed in the bytes, and the canonical text of what happened to the refs.
fn digest(tx: &Tx<'_>, at: u32, hashes: &[(PartId, [u8; 32])], events: &str) -> [u8; 32] {
    let mut h = Hasher::new("stetdoc/commit");
    h.number(u64::from(at));
    for (id, hash) in hashes {
        let name = tx.index.name(*id).map_or("", |n| n);
        h.field(name.as_bytes()).field(hash);
    }
    h.field(events.as_bytes());
    h.finish()
}

impl<'s> Tx<'s> {
    /// Builds the next `State` from the staged trees, the added parts and the audited ref table, and
    /// reports what happened to the refs. A transaction that changed nothing returns the base and a
    /// report with the version unmoved, so readable files and base tokens stay current.
    ///
    /// On an error the transaction is dropped and the base is as it was.
    pub(crate) fn commit(self) -> Result<(State, Committed), StetError> {
        failpoint::here("commit:begin")?;
        let base = self.base();
        let edited: Vec<PartId> = self
            .staged
            .iter()
            .filter(|(_, s)| s.changed())
            .map(|(id, _)| *id)
            .collect();
        let nothing = edited.is_empty()
            && self.added.is_none()
            && self.intents.is_empty()
            && self.touched.is_empty();
        if nothing {
            return Ok((base.clone(), Committed::unchanged(base.refs.base_token())));
        }
        let at = base
            .refs
            .version()
            .checked_add(1)
            .ok_or_else(|| limit("this document has had too many commits"))?;

        // Everything that can fail comes first.
        let Audited {
            mut table,
            scans,
            minted,
            retired,
            rebound,
            merged,
            events,
        } = audit::run(&self, at)?;
        let absorbed: Vec<(BlockRef, Reason)> = merged
            .iter()
            .filter_map(|(_, gone)| table.gone_reason(*gone).map(|why| (*gone, why)))
            .collect();
        for scan in &scans {
            self.parts()
                .seed_story(scan.part, scan.spec.kind, scan.story.clone());
        }
        let hashes = hash_changed(&self, &edited)?;
        let changes = digest(&self, at, &hashes, &events);
        failpoint::here("commit:advance")?;
        let token = table.advance(&changes)?;

        // From here nothing fails: values are moved into the next state.
        let Tx {
            staged,
            derived,
            index,
            added,
            ..
        } = self;
        let package_epoch = base.package_epoch + added.as_ref().map_or(0, |a| a.calls);
        let mut slots = base.slots.clone();
        for name in index.names().skip(slots.len()) {
            slots.push(PartSlot::unparsed(name.clone()));
        }
        let package = match added {
            Some(Added { package, fresh, .. }) => {
                for (id, Fresh { cell, call }) in fresh {
                    let epoch = base.slot(id).map_or(0, |s| s.generation.epoch) + call;
                    if let Some(slot) = slots.get_mut(id.index()) {
                        let generation = Generation { epoch, edits: 0 };
                        *slot = match cell.into_inner() {
                            Some(Ok(tree)) => {
                                PartSlot::with_tree(slot.name.clone(), tree, generation)
                            }
                            _ => PartSlot::fresh(slot.name.clone(), epoch),
                        };
                    }
                }
                package
            }
            None => base.package.clone(),
        };
        for (id, s) in staged {
            if !s.changed() {
                continue;
            }
            if let Some(slot) = slots.get_mut(id.index()) {
                let generation = Generation {
                    epoch: slot.generation.epoch,
                    edits: s.edits(),
                };
                *slot = PartSlot::with_tree(slot.name.clone(), s.tree, generation);
            }
        }
        let current = |dep: Dep| match dep {
            Dep::Package => Generation {
                epoch: package_epoch,
                edits: 0,
            },
            Dep::Part(id) => slots
                .get(id.index())
                .map_or_else(Generation::default, |s| s.generation),
        };
        let mut store = Derived::new();
        store.promote(&base.derived.borrow(), &current);
        store.promote(&derived.into_inner(), &current);
        let state = State {
            package,
            limits: base.limits.clone(),
            index,
            slots,
            package_epoch,
            main: base.main,
            refs: table,
            derived: RefCell::new(store),
        };
        let committed = Committed {
            base: token,
            minted,
            retired,
            rebound,
            merged,
            absorbed,
        };
        Ok((state, committed))
    }
}
