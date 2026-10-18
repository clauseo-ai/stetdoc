//! Origins: the keys a unit may keep because its content came from somewhere that had them.
//!
//! The origin of a unit is the union of the keys of the same node in the base, of every block the
//! edit declared the unit was copied or built from, validated in the base, of every unit of the base
//! that the unit absorbed (one that is no longer in the document and whose elements now stand in this
//! unit: the second paragraph of a join), and, for a block from another package, of the key set that
//! came with it (`docs/design/check.md` 2.3). A unit with no
//! origin has no keys to keep, so every finding in it is new, which is the safe direction: a
//! compiler that forgets to declare one is refused on a document that already had a problem, and
//! passes on a clean one.

use super::document::{SchemaPart, census_of, is_story, listing};
use super::examine::examine;
use super::keys::KeySet;
use super::units::{Unit, UnitKind, kind_of, layout_of};
use crate::api::error::{ErrorCode, StetError};
use crate::schema::unit_root;
use crate::state::{Dep, PartId, PartsView};
use crate::wordml::BlockKey;
use crate::xml::{NodeId, SpanTree};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Where the content of a block came from, as the compiler declared it. The block is the unit that
/// received content, or a row or a cell, which stands for the table that holds it. `from` are the
/// blocks the content was copied or built from, which are in the base or are declared themselves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Origin {
    /// The block that received content.
    pub(crate) block: BlockKey,
    /// The blocks the content came from.
    pub(crate) from: Vec<BlockKey>,
}

/// What an edit declares about where its new content came from. A declaration only makes `check`
/// less strict about problems that were already there: what is not declared is checked as new.
#[derive(Clone, Debug, Default)]
pub(crate) struct Declared {
    /// The origins, from the effects of the compiler's calls, in order.
    pub(crate) origins: Vec<Origin>,
    /// For `compare`: blocks whose origin is in another package, with the keys they had there
    /// (`unit_keys` computes them). A block may be an element of a part that is not a story, a style
    /// or a numbering definition, whose unit is the element itself.
    pub(crate) foreign: Vec<(BlockKey, KeySet)>,
    /// For `compare`: parts that were copied whole from another package (a notes part, a style sheet
    /// the redline lacked), with the keys the part had there (`part_keys` computes them). A finding of
    /// such a part with one of these keys was already there, and is carried.
    pub(crate) parts: Vec<(Arc<str>, KeySet)>,
}

/// The keys of a unit that a cache holds.
pub(super) struct UnitKeys(pub(super) KeySet);

/// What tells the cached keys of one unit from another's: its root and what it is.
pub(super) fn variant(unit: Unit) -> u64 {
    let kind: u8 = match unit.kind {
        UnitKind::Paragraph => 0,
        UnitKind::Table => 1,
        UnitKind::Note => 2,
        UnitKind::TopLevel => 3,
        UnitKind::Frame => 4,
    };
    let mut hasher = DefaultHasher::new();
    (kind, unit.root).hash(&mut hasher);
    hasher.finish()
}

/// Whether the keys of a unit are worth keeping: a frame or a table can be as big as a part, and
/// a paragraph is cheaper to read again than to look up.
fn is_cached(unit: Unit) -> bool {
    matches!(unit.kind, UnitKind::Frame | UnitKind::Table)
}

/// The keys of a unit, from the cache of `view` when the part has not changed since they were
/// computed. A frame or a table is validated once for each state of the part it passes through,
/// and not twice for each batch: the keys that a batch computes for the units it changed are
/// stored in the transaction's cache (`remember`), which commit promotes, so the next batch finds
/// them as the base's.
pub(super) fn keys_of(
    view: &PartsView<'_>,
    part: &SchemaPart,
    tree: &SpanTree,
    unit: Unit,
) -> Result<KeySet, StetError> {
    if !is_cached(unit) {
        return Ok(examine(part.kind, tree, unit).keys);
    }
    let build = |_: Option<&UnitKeys>| Ok(UnitKeys(examine(part.kind, tree, unit).keys));
    Ok(view
        .derived(variant(unit), &[Dep::Part(part.id)], &build)?
        .0
        .clone())
}

/// Stores the keys that were computed for a unit of the transaction, if the unit is worth keeping.
pub(super) fn remember(view: &PartsView<'_>, part: &SchemaPart, unit: Unit, keys: &KeySet) {
    if is_cached(unit) {
        let build = |_: Option<&UnitKeys>| Ok(UnitKeys(keys.clone()));
        // a cache that cannot be written is only a cache that does not help
        let _ = view.derived(variant(unit), &[Dep::Part(part.id)], &build);
    }
}

/// The keys of the unit that holds a block, in the state `parts` reads: what `compare` brings
/// from the other package as the origin of a block it imports.
pub(crate) fn unit_keys(parts: &PartsView<'_>, block: &BlockKey) -> Result<KeySet, StetError> {
    let list = listing(parts)?;
    let Some(part) = list.parts.iter().find(|p| *p.name == **block.part()) else {
        return Err(internal(format!(
            "{} is not a part that has a schema",
            block.part()
        )));
    };
    let tree = parts.tree(part.id)?;
    if !tree.node_ref(block.node()).is_element() {
        return Err(internal(format!(
            "{} has no element {:?}",
            block.part(),
            block.node()
        )));
    }
    let root = unit_root(tree, block.node());
    let unit = Unit {
        root,
        kind: kind_of(tree, root, layout_of(part.kind)),
    };
    Ok(examine(part.kind, tree, unit).keys)
}

/// The keys of a whole part, in the state `parts` reads: what `compare` brings from the other
/// package as the origin of a part it copies. They are the schema's keys of the part, and for a story
/// the Word rules of its units and the findings the part makes by itself (`check.md` 2.3).
pub(crate) fn part_keys(parts: &PartsView<'_>, name: &str) -> Result<KeySet, StetError> {
    let list = listing(parts)?;
    let Some(part) = list.parts.iter().find(|p| *p.name == *name) else {
        return Err(internal(format!("{name} is not a part that has a schema")));
    };
    let tree = parts.tree(part.id)?;
    let report = crate::schema::validate_part(part.kind, tree);
    let mut keys = KeySet::of_schema(&report.set);
    if is_story(part.kind) {
        for found in super::rules::scan_part(tree) {
            keys.insert(found.key);
        }
        for found in census_of(parts, part.id, tree)?.part_findings(&part.name) {
            keys.insert(found.key);
        }
    }
    Ok(keys)
}

/// Resolves the origin of each unit of one transaction.
pub(super) struct Origins<'a> {
    before: PartsView<'a>,
    after: PartsView<'a>,
    parts: &'a [SchemaPart],
    declared: &'a Declared,
    /// For a unit of the staged trees, the declared origins that name it.
    named: HashMap<(PartId, NodeId), Vec<usize>>,
    foreign: HashMap<(PartId, NodeId), KeySet>,
    /// The keys of units of the base, by part and root, as they were asked for.
    base: HashMap<(PartId, NodeId), KeySet>,
}

fn internal(message: String) -> StetError {
    StetError::new(ErrorCode::Internal, format!("internal: {message}"))
}

impl<'a> Origins<'a> {
    /// Prepares the origins of a transaction: `before` is the base's view, `after` the
    /// transaction's.
    pub(super) fn new(
        before: PartsView<'a>,
        after: PartsView<'a>,
        parts: &'a [SchemaPart],
        declared: &'a Declared,
    ) -> Result<Origins<'a>, StetError> {
        // The unit that a declared block stands for in the staged trees. A block that is no
        // longer there (a later operation removed it) stands for nothing.
        let target = |key: &BlockKey| -> Result<Option<(PartId, NodeId)>, StetError> {
            let Some(part) = parts.iter().find(|p| *p.name == **key.part()) else {
                return Err(internal(format!(
                    "{} is not a part that has a schema",
                    key.part()
                )));
            };
            let tree = after.tree(part.id)?;
            Ok(tree
                .node_ref(key.node())
                .is_element()
                .then(|| (part.id, unit_root(tree, key.node()))))
        };
        let mut named: HashMap<(PartId, NodeId), Vec<usize>> = HashMap::new();
        for (i, origin) in declared.origins.iter().enumerate() {
            if let Some(at) = target(&origin.block)? {
                named.entry(at).or_default().push(i);
            }
        }
        let mut foreign: HashMap<(PartId, NodeId), KeySet> = HashMap::new();
        for (block, keys) in &declared.foreign {
            if let Some(at) = target(block)? {
                foreign.entry(at).or_default().extend(keys);
            }
        }
        Ok(Origins {
            before,
            after,
            parts,
            declared,
            named,
            foreign,
            base: HashMap::new(),
        })
    }

    /// The keys of the unit with this root in the base: `None` when the base has no such element.
    fn base_keys(&mut self, part: &SchemaPart, root: NodeId) -> Result<Option<KeySet>, StetError> {
        if let Some(keys) = self.base.get(&(part.id, root)) {
            return Ok(Some(keys.clone()));
        }
        let tree = self.before.tree(part.id)?;
        if !tree.node_ref(root).is_element() {
            return Ok(None);
        }
        let unit = Unit {
            root,
            kind: kind_of(tree, root, layout_of(part.kind)),
        };
        let keys = keys_of(&self.before, part, tree, unit)?;
        self.base.insert((part.id, root), keys.clone());
        Ok(Some(keys))
    }

    /// The keys of a block that something was copied from: its unit in the base, or, for a block
    /// that an earlier operation of the same edit made, what that block was made from.
    fn source_keys(
        &mut self,
        from: &BlockKey,
        seen: &mut HashSet<BlockKey>,
    ) -> Result<KeySet, StetError> {
        let Some(part) = self.parts.iter().find(|p| *p.name == **from.part()) else {
            return Err(internal(format!(
                "{} is not a part that has a schema",
                from.part()
            )));
        };
        let tree = self.before.tree(part.id)?;
        if tree.node_ref(from.node()).is_element() {
            let root = unit_root(tree, from.node());
            return Ok(self.base_keys(part, root)?.unwrap_or_default());
        }
        let declared = self.declared;
        let mut keys = KeySet::default();
        let mut resolved = false;
        for origin in declared.origins.iter().filter(|o| o.block == *from) {
            resolved = true;
            for source in &origin.from {
                if seen.insert(source.clone()) {
                    keys.extend(&self.source_keys(source, seen)?);
                }
            }
        }
        if resolved {
            Ok(keys)
        } else {
            Err(internal(format!(
                "a declared origin names a block that is not in the base and was made from nothing: {} node {:?}",
                from.part(),
                from.node()
            )))
        }
    }

    /// The roots of the units of the base, in the part, that the edit declared the unit was made
    /// from, following the declarations that name blocks the base does not have.
    pub(super) fn sources(
        &self,
        part: &SchemaPart,
        root: NodeId,
    ) -> Result<Vec<NodeId>, StetError> {
        let tree = self.before.tree(part.id)?;
        let mut stack: Vec<BlockKey> = self
            .named
            .get(&(part.id, root))
            .into_iter()
            .flatten()
            .filter_map(|i| self.declared.origins.get(*i))
            .flat_map(|o| o.from.iter().cloned())
            .collect();
        let (mut seen, mut out) = (HashSet::new(), Vec::new());
        while let Some(from) = stack.pop() {
            if *from.part() != part.name || !seen.insert(from.clone()) {
                continue;
            }
            if tree.node_ref(from.node()).is_element() {
                out.push(unit_root(tree, from.node()));
            } else {
                for origin in self.declared.origins.iter().filter(|o| o.block == from) {
                    stack.extend(origin.from.iter().cloned());
                }
            }
        }
        Ok(out)
    }

    /// The keys that a part the edit added may keep: those of the part it was copied from, and those of the elements of it
    /// that were declared from another package.
    pub(super) fn whole_part(&self, part: &SchemaPart) -> KeySet {
        let mut keys = KeySet::default();
        for (name, k) in &self.declared.parts {
            if **name == *part.name {
                keys.extend(k);
            }
        }
        for (block, k) in &self.declared.foreign {
            if **block.part() == *part.name {
                keys.extend(k);
            }
        }
        keys
    }

    /// The roots of the units of the base that this unit absorbed: a unit that is no longer in the document, and one of whose
    /// elements now stands in this unit, as the second paragraph's runs do when a join (an accepted paragraph-mark deletion, a
    /// merge) moves them into the first. The survivor of a join carries the keys of both blocks, whatever the compiler declared.
    /// A block that is still there (the first half of a split) gives nothing this way: its keys come by declaration only. A
    /// unit inside the unit (a table inside a paragraph) is not walked into: it gives the unit whose root held it.
    fn moved_in(&self, part: &SchemaPart, unit: Unit) -> Result<Vec<NodeId>, StetError> {
        let (before, after) = (self.before.tree(part.id)?, self.after.tree(part.id)?);
        let document = after.document().id();
        let gone = |root: NodeId| {
            let mut cur = after.node_ref(root);
            while let Some(parent) = cur.parent() {
                cur = parent;
            }
            cur.id() != document
        };
        let mut out: Vec<NodeId> = Vec::new();
        let mut stack: Vec<NodeId> = after
            .node_ref(unit.root)
            .child_elements()
            .map(|c| c.id())
            .collect();
        while let Some(id) = stack.pop() {
            let was = before.node_ref(id);
            if was.is_element() {
                let root = unit_root(before, id);
                if root == id {
                    // A unit of its own, such as a table inside a paragraph, is checked as itself, and it came from the unit
                    // whose root held it: when that unit is gone, its keys come along (a rejected split that had left the
                    // table alone in the second paragraph joins only the table back).
                    if let Some(holder) = was.parent().map(|p| p.id())
                        && unit_root(before, holder) == holder
                        && holder != unit.root
                        && before.node_ref(holder).parent().is_some()
                        && gone(holder)
                        && !out.contains(&holder)
                    {
                        out.push(holder);
                    }
                    continue;
                }
                let attached = before.node_ref(root).parent().is_some();
                if root != unit.root && attached && gone(root) {
                    // It came from another unit, with everything under it.
                    if !out.contains(&root) {
                        out.push(root);
                    }
                    continue;
                }
                if root == unit.root {
                    // Where it was: what is under it may still have come from elsewhere.
                    stack.extend(after.node_ref(id).child_elements().map(|c| c.id()));
                    continue;
                }
            }
            // A node the edit made, or a unit of its own: look inside a node the edit made only.
            if !was.is_element() && !crate::schema::is_unit_boundary(after, id) {
                stack.extend(after.node_ref(id).child_elements().map(|c| c.id()));
            }
        }
        Ok(out)
    }

    /// The keys that the unit may keep.
    pub(super) fn of(&mut self, part: &SchemaPart, unit: Unit) -> Result<KeySet, StetError> {
        let mut keys = self.base_keys(part, unit.root)?.unwrap_or_default();
        for root in self.moved_in(part, unit)? {
            keys.extend(&self.base_keys(part, root)?.unwrap_or_default());
        }
        let key = (part.id, unit.root);
        let sources: Vec<BlockKey> = self
            .named
            .get(&key)
            .into_iter()
            .flatten()
            .filter_map(|i| self.declared.origins.get(*i))
            .flat_map(|o| o.from.iter().cloned())
            .collect();
        let mut seen: HashSet<BlockKey> = sources.iter().cloned().collect();
        for from in &sources {
            keys.extend(&self.source_keys(from, &mut seen)?);
        }
        if let Some(foreign) = self.foreign.get(&key) {
            keys.extend(foreign);
        }
        Ok(keys)
    }
}
