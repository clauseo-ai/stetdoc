//! L2 Locality by block, for the random batches of `laws::random`: a top-level block of the body that the batch does not name,
//! and that no merge, split or insert touches, keeps its XML byte for byte. One change is allowed, to the paragraph before or
//! after a block the batch touches: it may gain the range markers and comment reference runs that a removal hoists into it,
//! when each one's id is that of a marker or a reference that a touched block (or the markup between blocks next to it) had,
//! or of a comment that the old body did not have at all, one the batch made. Markup between blocks next to a touched block
//! may move into such a paragraph.
//!
//! The body part is read from the package (`locality::part`) and cut into its top-level elements with a small scanner. Each
//! `w:p` and `w:tbl` is one block of the view, and a block-level container (`w:sdt`, `w:customXml`) holds the blocks inside it,
//! so the elements line up with the refs of the view's top-level blocks (`Facts::top`) in order. A document where the counts
//! differ (an element the view shows another way) is not checked by block, and the entry check of L2 still holds for it.
//!
//! What a batch touches: the blocks its operations name (`ref`, `to`, `second`, and every block from `ref` through `to`), the
//! block after a merge's paragraph, and the table that holds a named paragraph or row. A merge's paragraph after is the next
//! one the operations before it left: a deleted paragraph, an earlier merge's second and, untracked, a table whose last row a
//! `row` operation deleted are passed over. An `accept` or `reject` touches every block with revision markup, an operation
//! on an existing comment every block with that comment's markers, a `delete-note` every block with a note reference, and a
//! section's properties the elements that hold a `w:sectPr`.

use super::facts::Facts;
use super::generate::Generated;
use super::hoist::{Before, kept_or_hoisted};
use super::locality::part;
use serde_json::Value;
use std::collections::BTreeSet;

/// One element of an XML text: its local name, where it starts and ends, and where its content is.
pub(super) struct El {
    pub(super) name: String,
    pub(super) span: (usize, usize),
    pub(super) inner: (usize, usize),
}

/// The child elements of the content `from..to` of an element, or `None` when the text is not one this scanner reads.
pub(super) fn children(xml: &[u8], from: usize, to: usize) -> Option<Vec<El>> {
    let find = |at: usize, what: &[u8]| {
        xml.get(at..to)?
            .windows(what.len())
            .position(|w| w == what)
            .map(|p| at + p)
    };
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut open: Option<(String, usize, usize)> = None;
    let mut i = from;
    while let Some(lt) = find(i, b"<") {
        let rest = xml.get(lt..to)?;
        if rest.starts_with(b"<!--") {
            i = find(lt, b"-->")? + 3;
        } else if rest.starts_with(b"<![CDATA[") {
            i = find(lt, b"]]>")? + 3;
        } else if rest.starts_with(b"<?") {
            i = find(lt, b"?>")? + 2;
        } else if rest.starts_with(b"<!") {
            i = find(lt, b">")? + 1;
        } else if rest.starts_with(b"</") {
            let gt = find(lt, b">")?;
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                let (name, start, inner) = open.take()?;
                out.push(El {
                    name,
                    span: (start, gt + 1),
                    inner: (inner, lt),
                });
            }
            i = gt + 1;
        } else {
            // A start tag: its end is the first `>` outside a quoted value.
            let mut quote = None;
            let mut gt = None;
            for (k, &b) in rest.iter().enumerate() {
                match (quote, b) {
                    (None, b'"' | b'\'') => quote = Some(b),
                    (Some(q), _) if b == q => quote = None,
                    (None, b'>') => {
                        gt = Some(lt + k);
                        break;
                    }
                    _ => {}
                }
            }
            let gt = gt?;
            let tag = xml.get(lt + 1..gt)?;
            let name_end = tag
                .iter()
                .position(|b| b.is_ascii_whitespace() || *b == b'/')
                .unwrap_or(tag.len());
            let qname = String::from_utf8_lossy(tag.get(..name_end)?).into_owned();
            let local = qname.rsplit(':').next().unwrap_or_default().to_string();
            let empty = tag.last() == Some(&b'/');
            if depth == 0 {
                if empty {
                    out.push(El {
                        name: local,
                        span: (lt, gt + 1),
                        inner: (gt + 1, gt + 1),
                    });
                } else {
                    open = Some((local, lt, gt + 1));
                }
            }
            if !empty {
                depth += 1;
            }
            i = gt + 1;
        }
    }
    (depth == 0).then_some(out)
}

/// How many blocks of the view an element of the body is.
pub(super) fn blocks_in(xml: &[u8], el: &El) -> Option<usize> {
    match el.name.as_str() {
        "p" | "tbl" => Some(1),
        "sdt" | "sdtContent" | "customXml" => children(xml, el.inner.0, el.inner.1)?
            .iter()
            .map(|c| blocks_in(xml, c))
            .sum(),
        _ => Some(0),
    }
}

/// The top-level elements of the body of a document part.
pub(super) fn body(xml: &[u8]) -> Option<Vec<El>> {
    let root = children(xml, 0, xml.len())?.into_iter().next()?;
    let body = children(xml, root.inner.0, root.inner.1)?
        .into_iter()
        .find(|e| e.name == "body")?;
    children(xml, body.inner.0, body.inner.1)
}

/// A range marker that a removal hoists into a neighbour: the start and end of a comment's range, a bookmark, a permission
/// range or a move range.
pub(super) fn is_marker(local: &str) -> bool {
    matches!(
        local,
        "commentRangeStart"
            | "commentRangeEnd"
            | "bookmarkStart"
            | "bookmarkEnd"
            | "permStart"
            | "permEnd"
            | "moveFromRangeStart"
            | "moveFromRangeEnd"
            | "moveToRangeStart"
            | "moveToRangeEnd"
    )
}

/// Whether `needle` occurs in `hay`.
pub(super) fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// The bytes of an element.
pub(super) fn span<'a>(xml: &'a [u8], el: &El) -> &'a [u8] {
    xml.get(el.span.0..el.span.1).unwrap_or_default()
}

/// What a batch touches, by the refs of top-level blocks and by kinds of markup.
#[derive(Default)]
struct Touched {
    tops: BTreeSet<String>,
    revisions: bool,
    comments: Vec<String>,
    notes: bool,
    section: bool,
}

fn touched(facts: &Facts, g: &Generated) -> Touched {
    let top_of = |r: &str| -> Option<String> {
        facts
            .paras
            .iter()
            .find(|p| p.r == r && p.story == "body")
            .map(|p| p.top.clone())
            .or_else(|| {
                facts
                    .tables
                    .iter()
                    .find(|t| t.r == r || t.rows.iter().any(|(row, _)| row == r))
                    .map(|t| t.top.clone())
            })
    };
    let index = |r: &str| facts.top.iter().position(|t| t == r);
    // A ref that names a top-level block itself (not a paragraph of a table's cell).
    let is_top = |r: Option<&str>| r.is_some_and(|r| facts.top.iter().any(|t| t == r));
    let mut gone: BTreeSet<String> = BTreeSet::new();
    // The rows that `row` operations deleted so far, for the table that loses its last row.
    let mut rows_gone: BTreeSet<String> = BTreeSet::new();
    let mut t = Touched::default();
    for op in &g.ops {
        let Ok(v) = serde_json::from_str::<Value>(&op.json) else {
            continue;
        };
        let field = |k: &str| v.get(k).and_then(Value::as_str).and_then(top_of);
        let (first, to, second) = (field("ref"), field("to"), field("second"));
        if let (Some(a), Some(b)) = (
            first.as_deref().and_then(index),
            to.as_deref().and_then(index),
        ) {
            for r in facts.top.get(a.min(b)..=a.max(b)).into_iter().flatten() {
                t.tops.insert(r.clone());
            }
        }
        // A merge with no `second` takes the next paragraph of the document as the batch has left it at that operation (an
        // operation sees what the operations before it did, ops/mod.rs): the first block after it that no earlier operation
        // removed (a deleted paragraph or range, the second of an earlier merge) and that shows Final text (a deleted block does
        // not). That block is the merge's actual second, and the batch names it.
        if op.name == "merge"
            && second.is_none()
            && let Some(at) = first.as_deref().and_then(index)
        {
            let empty = |r: &str| facts.paras.iter().any(|p| p.r == r && p.text.is_empty());
            for next in facts.top.iter().skip(at + 1) {
                t.tops.insert(next.clone());
                if !gone.contains(next) && !empty(next) {
                    gone.insert(next.clone());
                    break;
                }
            }
        }
        // What this operation removes, for the merges after it.
        match op.name {
            "delete" => {
                let whole = ["find", "start", "end"].iter().all(|k| v.get(*k).is_none());
                match (
                    first.as_deref().and_then(index),
                    to.as_deref().and_then(index),
                ) {
                    (Some(a), Some(b)) => {
                        for r in facts.top.get(a.min(b)..=a.max(b)).into_iter().flatten() {
                            gone.insert(r.clone());
                        }
                    }
                    (Some(_), None) if whole && is_top(v.get("ref").and_then(Value::as_str)) => {
                        gone.extend(first.clone());
                    }
                    _ => {}
                }
            }
            "merge" => gone.extend(second.clone()),
            // Untracked, a table that loses its last row is removed (revisions.md 3.2.3: a merge then joins the paragraphs on
            // either side of where it was).
            "row" if !g.tracked && v.get("action").and_then(Value::as_str) == Some("delete") => {
                if let Some(row) = v.get("ref").and_then(Value::as_str) {
                    rows_gone.insert(row.to_string());
                    let emptied = facts.tables.iter().find(|t| {
                        t.r == t.top
                            && t.rows.iter().any(|(r, _)| r == row)
                            && t.rows.iter().all(|(r, _)| rows_gone.contains(r))
                    });
                    gone.extend(emptied.map(|t| t.r.clone()));
                }
            }
            _ => {}
        }
        t.tops.extend([first, to, second].into_iter().flatten());
        match op.name {
            "accept" | "reject" => t.revisions = true,
            "reply" | "resolve" | "edit-comment" | "delete-comment" => {
                if let Some(c) = v.get("comment").and_then(Value::as_str) {
                    t.comments.push(c.trim_start_matches('c').to_string());
                }
            }
            "delete-note" => t.notes = true,
            "set"
                if v.get("section").is_some()
                    || v.get("target").and_then(Value::as_str) == Some("section") =>
            {
                t.section = true;
            }
            _ => {}
        }
    }
    // An `accept` or a `reject` resolves what the operations before it wrote too, and a paragraph mark it resolves joins the
    // next block: the block after each one the batch names is touched.
    if t.revisions {
        let after: Vec<String> = t
            .tops
            .iter()
            .filter_map(|r| index(r).and_then(|i| facts.top.get(i + 1)).cloned())
            .collect();
        t.tops.extend(after);
    }
    t
}

/// Whether an element holds markup that a batch touches by kind rather than by ref.
fn holds(text: &str, t: &Touched) -> bool {
    let revision = [
        ":ins ",
        ":ins>",
        ":del ",
        ":del>",
        "<ins",
        "<del ",
        "<del>",
        "Change",
        "moveFrom",
        "moveTo",
        "cellIns",
        "cellDel",
        "cellMerge",
        "customXmlDel",
        "customXmlIns",
        "customXmlMove",
    ];
    (t.revisions && revision.iter().any(|m| text.contains(m)))
        || (t.notes && text.contains("noteReference"))
        || (t.section && text.contains("sectPr"))
        || t.comments
            .iter()
            .any(|id| text.contains("comment") && text.contains(&format!("\"{id}\"")))
}

/// The top-level elements of a document's body, each with the count of view blocks it holds, when they line up with the view's
/// top-level blocks.
fn lined_up(bytes: &[u8], facts: &Facts) -> Option<(Vec<u8>, Vec<El>, Vec<usize>)> {
    let xml = part(bytes, facts.body_part.as_deref()?)?;
    let els = body(&xml)?;
    let counts: Vec<usize> = els
        .iter()
        .map(|e| blocks_in(&xml, e))
        .collect::<Option<_>>()?;
    (counts.iter().sum::<usize>() == facts.top.len()).then_some((xml, els, counts))
}

/// Whether the per-block check can run on a document: its body's top-level elements line up with the view's blocks.
pub fn lines_up(bytes: &[u8], facts: &Facts) -> bool {
    lined_up(bytes, facts).is_some()
}

/// L2 by block: `Ok` when every top-level block of the body that the batch does not touch keeps its bytes (or gained only what
/// a removal next to it hoisted), or when the body cannot be lined up with the view.
pub fn per_block(before: &[u8], after: &[u8], facts: &Facts, g: &Generated) -> Result<(), String> {
    let Some((old, els, counts)) = lined_up(before, facts) else {
        return Ok(());
    };
    let Some(new) = facts
        .body_part
        .as_deref()
        .and_then(|name| part(after, name))
    else {
        return Err("L2: the body part cannot be read after the edit".to_string());
    };
    let Some(new_els) = body(&new) else {
        return Err("L2: the body part cannot be cut into blocks after the edit".to_string());
    };
    let t = touched(facts, g);
    let (mut refs, mut hit) = (Vec::new(), Vec::new());
    let mut at = 0;
    // A revision of a paragraph mark that is accepted or rejected joins its paragraph to the next block, so under an `accept` or
    // a `reject` the block after one with revision markup is touched too, and so is markup between blocks (a bookmark, a range).
    let mut after_revision = false;
    for (el, n) in els.iter().zip(&counts) {
        let these = facts.top.get(at..at + n).unwrap_or_default();
        at += n;
        let text = String::from_utf8_lossy(span(&old, el));
        let joined = after_revision;
        // Markup between blocks (a bookmark) passes the join on to the next block.
        after_revision = (t.revisions && holds(&text, &t)) || (joined && *n == 0);
        hit.push(
            these.iter().any(|r| t.tops.contains(r))
                || holds(&text, &t)
                || (t.revisions && (joined || *n == 0)),
        );
        refs.push(these.to_vec());
    }
    let before = Before {
        xml: &old,
        els: &els,
        counts: &counts,
        refs: &refs,
        touched: &hit,
    };
    kept_or_hoisted(&before, &new, &new_els)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::parts::{Doc, entries_doc};
    use crate::fixtures::zip::Zip;
    use crate::laws::edits::run_batch;
    use crate::laws::generate::GenOp;
    use stetdoc_core::{Document, OpenOptions};

    fn generated(ops: &[(&'static str, &str)], tracked: bool) -> Generated {
        let ops = ops
            .iter()
            .map(|(name, json)| GenOp {
                name,
                json: (*json).to_string(),
                refused: None,
                sticks: None,
            })
            .collect();
        Generated { ops, tracked }
    }

    /// The bytes after an untracked batch of `ops`.
    fn edited(bytes: &[u8], ops: &str) -> Vec<u8> {
        run_batch(bytes, &[], ops, false).unwrap().unwrap().saved
    }

    #[test]
    fn a_merge_after_the_last_row_of_a_table_is_deleted_names_the_paragraph_after_the_table() {
        let para = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
        let body = format!(
            r#"{}<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr>{}</w:tc></w:tr></w:tbl>{}{}"#,
            para("Before."),
            para("Cell."),
            para("After."),
            para("Far.")
        );
        let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
        let facts =
            Facts::of(&Document::open(bytes.clone(), &OpenOptions::default()).unwrap()).unwrap();
        assert_eq!(facts.top, ["p1", "t1", "p3", "p4"]);
        let (delete, merge) = (
            r#"{"op":"row","ref":"r1","action":"delete"}"#,
            r#"{"op":"merge","ref":"p1"}"#,
        );
        // Untracked, the row's deletion removes the table, and the merge joins p1 with p3, which the batch names.
        let g = generated(&[("row", delete), ("merge", merge)], false);
        let after = edited(&bytes, &g.json());
        per_block(&bytes, &after, &facts, &g).unwrap();
        // A paragraph that the batch does not name and that changes is still caught.
        let changed = edited(
            &bytes,
            &format!(
                r#"{},{{"op":"replace","ref":"p4","find":"Far","with":"Near"}}"#,
                g.json()
            ),
        );
        let e = per_block(&bytes, &changed, &facts, &g).unwrap_err();
        assert!(
            e.starts_with("L2: p4, which the batch does not name"),
            "{e}"
        );
    }
}
