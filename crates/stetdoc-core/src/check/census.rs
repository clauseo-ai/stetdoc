//! The rules that need a whole part: ids that must be unique (W3) and ranges that must be paired
//! (W7).
//!
//! One walk over a story part collects the ids into sorted vectors, four bytes an id, and the
//! rules are comparisons of them. The walk is `schema::walk_unit` without a stop, so it reads what
//! Word reads: the branch of an `mc:AlternateContent` that was taken, and no ignorable element. The
//! old properties inside a `*PrChange` are not read either, because nothing in them is a revision.
//! A census is a function of one part, so `check` keeps one per part generation (`PartsView::derived`)
//! and the census that an edit builds for the part it changed is the one the next edit starts from.
//!
//! Revision ids are unique across the story parts of a document. Bookmark ids, and the pairs of
//! ranges, are read within one part, because Word repeats a bookmark id in another part (the
//! corpus has a document that does).

use super::found::Found;
use super::rules::{Ancestors, Tag, tag_of};
use crate::api::check::FindingCode;
use crate::schema::{is_word_namespace, walk_unit};
use crate::xml::{NodeRef, SpanTree};

/// The ids and names of one part.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Census {
    /// The `w:id` of every tracked change that has one of its own: a revision, a paragraph mark's
    /// or a row's, a property change, and the start of a move or of a custom XML revision.
    revisions: Vec<u32>,
    bookmark_starts: Vec<u32>,
    bookmark_ends: Vec<u32>,
    from_starts: Vec<u32>,
    from_ends: Vec<u32>,
    to_starts: Vec<u32>,
    to_ends: Vec<u32>,
    from_names: Vec<String>,
    to_names: Vec<String>,
    /// How many `w:fldChar` there are, which says whether W8 has anything to read.
    field_chars: usize,
    /// How many comment range markers and references there are, which says whether the audit has
    /// anything to read in a story that has no comments part.
    comment_markers: usize,
}

/// Whether a Word element has a revision id of its own.
fn is_revision(local: &str) -> bool {
    matches!(
        local,
        "ins"
            | "del"
            | "moveFrom"
            | "moveTo"
            | "rPrChange"
            | "pPrChange"
            | "sectPrChange"
            | "tblPrChange"
            | "tblPrExChange"
            | "tblGridChange"
            | "trPrChange"
            | "tcPrChange"
            | "numberingChange"
            | "cellIns"
            | "cellDel"
            | "cellMerge"
            | "moveFromRangeStart"
            | "moveToRangeStart"
            | "customXmlInsRangeStart"
            | "customXmlDelRangeStart"
            | "customXmlMoveFromRangeStart"
            | "customXmlMoveToRangeStart"
    )
}

/// The `w:id` of an element, if it is a whole number that fits.
fn id_of(element: &NodeRef<'_>) -> Option<u32> {
    element.attr(element.namespace(), "id")?.trim().parse().ok()
}

fn name_of(element: &NodeRef<'_>) -> Option<String> {
    element
        .attr(element.namespace(), "name")
        .map(|n| n.into_owned())
}

impl Census {
    /// The census of one part: one walk over everything Word reads in it.
    pub(crate) fn of(tree: &SpanTree) -> Census {
        let mut census = Census::default();
        let mut up = Ancestors::default();
        walk_unit(tree, tree.root().id(), false, &mut |node, depth| {
            up.leave_to(depth);
            let element = tree.node_ref(node);
            let tag = if is_word_namespace(element.namespace()) {
                let local = element.local_name();
                if !up.contains(Tag::Snapshot) {
                    census.record(&element, local);
                }
                tag_of(local)
            } else {
                Tag::Other
            };
            up.push(tag);
        });
        census.revisions.sort_unstable();
        for ids in [
            &mut census.bookmark_starts,
            &mut census.bookmark_ends,
            &mut census.from_starts,
            &mut census.from_ends,
            &mut census.to_starts,
            &mut census.to_ends,
        ] {
            ids.sort_unstable();
        }
        census.from_names.sort_unstable();
        census.to_names.sort_unstable();
        census
    }

    fn record(&mut self, element: &NodeRef<'_>, local: &str) {
        let id = id_of(element);
        if let Some(id) = id.filter(|_| is_revision(local)) {
            self.revisions.push(id);
        }
        match local {
            "fldChar" => self.field_chars += 1,
            "commentRangeStart" | "commentRangeEnd" | "commentReference" => {
                self.comment_markers += 1;
            }
            _ => {}
        }
        let (ids, names) = match local {
            "bookmarkStart" => (&mut self.bookmark_starts, None),
            "bookmarkEnd" => (&mut self.bookmark_ends, None),
            "moveFromRangeStart" => (&mut self.from_starts, Some(&mut self.from_names)),
            "moveFromRangeEnd" => (&mut self.from_ends, None),
            "moveToRangeStart" => (&mut self.to_starts, Some(&mut self.to_names)),
            "moveToRangeEnd" => (&mut self.to_ends, None),
            _ => return,
        };
        ids.extend(id);
        if let (Some(names), Some(name)) = (names, name_of(element)) {
            names.push(name);
        }
    }

    /// Whether the part has a field.
    pub(crate) fn has_fields(&self) -> bool {
        self.field_chars > 0
    }

    /// Whether the part has a comment range marker or a comment reference.
    pub(crate) fn has_comment_markers(&self) -> bool {
        self.comment_markers > 0
    }

    /// The sorted ids of the tracked changes of the part.
    pub(crate) fn revisions(&self) -> &[u32] {
        &self.revisions
    }

    /// The ids of the bookmarks that have a start and no end, or an end and no start, in order.
    pub(crate) fn unpaired_bookmarks(&self) -> Vec<u32> {
        unpaired(&self.bookmark_starts, &self.bookmark_ends)
    }

    /// The findings that one part makes by itself: bookmark ids used twice, and starts and ends
    /// that do not pair, in the part.
    pub(crate) fn part_findings(&self, part: &str) -> Vec<Found> {
        let mut out = Vec::new();
        let mut push = |code: FindingCode, thing: String| {
            out.push(Found::rule(code, &format!("{part}#{thing}"), None));
        };
        for id in duplicates(&self.bookmark_starts) {
            push(FindingCode::BookmarkIdDuplicate, id.to_string());
        }
        for id in self.unpaired_bookmarks() {
            push(FindingCode::BookmarkUnpaired, id.to_string());
        }
        for id in unpaired(&self.from_starts, &self.from_ends) {
            push(FindingCode::MoveRangeUnpaired, format!("from {id}"));
        }
        for id in unpaired(&self.to_starts, &self.to_ends) {
            push(FindingCode::MoveRangeUnpaired, format!("to {id}"));
        }
        let missing = |a: &[String], b: &[String]| -> Vec<String> {
            let mut out: Vec<String> = a
                .iter()
                .filter(|n| b.binary_search(n).is_err())
                .cloned()
                .collect();
            out.dedup();
            out
        };
        for name in missing(&self.from_names, &self.to_names)
            .into_iter()
            .chain(missing(&self.to_names, &self.from_names))
        {
            push(FindingCode::MoveNameUnpaired, name);
        }
        out
    }
}

/// The findings that the censuses of all the story parts of a document make together: a revision
/// id used twice, in one part or in two.
pub(crate) fn revision_findings(censuses: &[&Census]) -> Vec<Found> {
    let mut all: Vec<u32> = censuses
        .iter()
        .flat_map(|c| c.revisions.iter().copied())
        .collect();
    all.sort_unstable();
    duplicates(&all)
        .into_iter()
        .map(|id| Found::rule(FindingCode::RevisionIdDuplicate, &id.to_string(), None))
        .collect()
}

/// The runs of equal values of a sorted slice, as `(value, how many)`.
fn runs(sorted: &[u32]) -> Vec<(u32, usize)> {
    let mut out: Vec<(u32, usize)> = Vec::new();
    for &v in sorted {
        match out.last_mut() {
            Some((last, n)) if *last == v => *n += 1,
            _ => out.push((v, 1)),
        }
    }
    out
}

/// The values that occur more than once, each once.
fn duplicates(sorted: &[u32]) -> Vec<u32> {
    runs(sorted)
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .map(|(v, _)| v)
        .collect()
}

/// The values that occur a different number of times in the two sorted slices.
fn unpaired(starts: &[u32], ends: &[u32]) -> Vec<u32> {
    let (a, b) = (runs(starts), runs(ends));
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    loop {
        match (a.get(i), b.get(j)) {
            (Some(&(x, n)), Some(&(y, m))) => {
                if x == y {
                    if n != m {
                        out.push(x);
                    }
                    i += 1;
                    j += 1;
                } else if x < y {
                    out.push(x);
                    i += 1;
                } else {
                    out.push(y);
                    j += 1;
                }
            }
            (Some(&(x, _)), None) => {
                out.push(x);
                i += 1;
            }
            (None, Some(&(y, _))) => {
                out.push(y);
                j += 1;
            }
            (None, None) => return out,
        }
    }
}
