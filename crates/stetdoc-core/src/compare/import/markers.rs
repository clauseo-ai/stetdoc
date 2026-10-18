//! Bookmarks and drawing ids of imported content (design `compare.md`, 7.5).

use super::{Cx, Importer};
use crate::api::error::StetError;
use crate::revisions::import_writer::ImportWriter;
use std::collections::{HashMap, HashSet};

/// What the importer remembers about bookmarks and drawings.
#[derive(Default)]
pub(super) struct MarkState {
    /// The names that bookmarks of the revised document have, and those that import has added.
    names: Option<HashSet<String>>,
    /// Original bookmark id (by source part) to the id it was given.
    ids: HashMap<(String, String), u32>,
    /// The next drawing id, found by one walk of every story.
    drawing: Option<u32>,
    /// The next VML shape number (`_x0000_s<n>`), found by one walk of every story.
    shape: Option<u32>,
    /// The shape ids of the block being copied, old to new, so that a shape's `id` and its `o:spid` stay one.
    pub(super) shapes: HashMap<String, String>,
    /// Bookmark ids that have a start and an end in the block being copied.
    pub(super) pairs: HashSet<String>,
    /// The part being copied from.
    pub(super) source: String,
}

/// Every story tree of the revised document, by part name.
fn story_parts(tx: &ImportWriter<'_, '_>) -> Result<Vec<std::sync::Arc<str>>, StetError> {
    let view = tx.parts();
    Ok(view
        .story_specs()?
        .0
        .iter()
        .map(|s| std::sync::Arc::clone(&s.part))
        .collect())
}

/// The bookmark `name`: a new id in the revised document, or `None` when it is dropped (the name is taken, it is Word's own
/// `_GoBack`, or its end is not in the block being copied).
pub(super) fn start(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    cx: Cx<'_>,
    id: &str,
    name: &str,
) -> Result<Option<u32>, StetError> {
    if name == "_GoBack" || !imp.marks.pairs.contains(id) {
        return Ok(None);
    }
    if imp.marks.names.is_none() {
        let mut names = HashSet::new();
        for part in story_parts(tx)? {
            let view = tx.parts();
            let Some(pid) = view.id(&part) else { continue };
            let tree = view.tree(pid)?;
            let mut stack = vec![tree.root()];
            while let Some(n) = stack.pop() {
                for c in n.child_elements() {
                    if c.local_name() == "bookmarkStart"
                        && let Some(v) = c.attrs().find(|a| a.local_name() == "name")
                    {
                        names.insert(v.value().into_owned());
                    }
                    stack.push(c);
                }
            }
        }
        imp.marks.names = Some(names);
    }
    let taken = imp.marks.names.as_ref().is_some_and(|n| n.contains(name));
    if taken {
        imp.stats.bookmarks_dropped += 1;
        return Ok(None);
    }
    if let Some(n) = imp.marks.names.as_mut() {
        n.insert(name.to_owned());
    }
    let new = tx.next_annotation_id()?;
    imp.marks
        .ids
        .insert((cx.src.to_string(), id.to_owned()), new);
    Ok(Some(new))
}

/// The id that the end of bookmark `id` takes, if its start was kept.
pub(super) fn end(imp: &mut Importer<'_>, id: &str) -> Option<u32> {
    let key = (imp.marks.source.clone(), id.to_owned());
    imp.marks.ids.get(&key).copied()
}

/// A drawing id that no drawing of the revised document has.
pub(super) fn fresh_drawing_id(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
) -> Result<u32, StetError> {
    let next = match imp.marks.drawing {
        Some(n) => n,
        None => {
            let mut max = 0u32;
            for part in story_parts(tx)? {
                let view = tx.parts();
                let Some(pid) = view.id(&part) else { continue };
                let tree = view.tree(pid)?;
                let mut stack = vec![tree.root()];
                while let Some(n) = stack.pop() {
                    for c in n.child_elements() {
                        if c.local_name() == "docPr"
                            && let Some(v) = c
                                .attrs()
                                .find(|a| a.local_name() == "id")
                                .and_then(|a| a.value().parse::<u32>().ok())
                        {
                            max = max.max(v);
                        }
                        stack.push(c);
                    }
                }
            }
            max.saturating_add(1)
        }
    };
    imp.marks.drawing = Some(next.saturating_add(1));
    Ok(next)
}

/// The number of a VML shape id, `_x0000_s1026` is 1026.
pub(super) fn shape_number(id: &str) -> Option<u32> {
    id.strip_prefix("_x0000_s")?.parse().ok()
}

/// A VML shape id that no shape of the revised document has, the same for the same `old` within one copy: a shape's `id` and
/// its `o:spid` name one shape, and Word repairs a document in which two shapes share one.
pub(super) fn fresh_shape_id(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    old: &str,
) -> Result<String, StetError> {
    if let Some(done) = imp.marks.shapes.get(old) {
        return Ok(done.clone());
    }
    let next = match imp.marks.shape {
        Some(n) => n,
        None => {
            let mut max = 1024u32;
            for part in story_parts(tx)? {
                let view = tx.parts();
                let Some(pid) = view.id(&part) else { continue };
                let tree = view.tree(pid)?;
                let mut stack = vec![tree.root()];
                while let Some(n) = stack.pop() {
                    for c in n.child_elements() {
                        for a in c.attrs() {
                            if matches!(a.local_name(), "id" | "spid")
                                && let Some(v) = shape_number(&a.value())
                            {
                                max = max.max(v);
                            }
                        }
                        stack.push(c);
                    }
                }
            }
            max.saturating_add(1)
        }
    };
    imp.marks.shape = Some(next.saturating_add(1));
    let new = format!("_x0000_s{next}");
    imp.marks.shapes.insert(old.to_owned(), new.clone());
    Ok(new)
}
