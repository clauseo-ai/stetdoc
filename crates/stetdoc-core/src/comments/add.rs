//! Adding a comment, and replying to one.
//!
//! The markers go into the story first, through the compiler, because that can be refused (a range that
//! cuts into a field, a deleted block). Then the parts: the comment, its paragraphs and its rows.

use super::body::{self, NewComment, ParaSpec};
use super::errors::{internal, invalid, unsupported};
use super::lookup::{find, locate};
use super::parts::{ensure_comments, ensure_extended, names};
use super::{CommentAction, CommentEffect, CommentWarning, rows, styles};
use crate::api::error::{ErrorCode, StetError};
use crate::revisions::compiler::Compiler;
use crate::revisions::handle::PartWriter;
use crate::revisions::stamp::RevStamp;
use crate::revisions::types::{MarkerSpec, PropElement};
use crate::threads::{NS_W14, Threads};
use crate::wordml::{BlockKey, BlockKind};
use std::ops::Range;
use std::sync::Arc;

fn pe(ns: &str, local: &str, attrs: &[(&str, &str)], children: Vec<PropElement>) -> PropElement {
    PropElement {
        ns: ns.to_owned(),
        local: local.to_owned(),
        attrs: attrs
            .iter()
            .map(|(l, v)| (ns.to_owned(), (*l).to_owned(), (*v).to_owned()))
            .collect(),
        children,
    }
}

/// The markers of comment `id`: the start, the end and the run that holds the reference, which carries the
/// reference style when the document has one.
pub(super) fn marker_spec(w: &str, id: i64, reference_style: Option<&str>) -> MarkerSpec {
    let id = id.to_string();
    let mut run = Vec::new();
    if let Some(style) = reference_style {
        run.push(pe(
            w,
            "rPr",
            &[],
            vec![pe(w, "rStyle", &[("val", style)], vec![])],
        ));
    }
    run.push(pe(w, "commentReference", &[("id", &id)], vec![]));
    MarkerSpec {
        start: pe(w, "commentRangeStart", &[("id", &id)], vec![]),
        end: pe(w, "commentRangeEnd", &[("id", &id)], vec![]),
        reference: Some(pe(w, "r", &[], run)),
    }
}

/// The comment text as paragraphs, or `invalid_payload` if there is no text.
fn check_text(text: &str) -> Result<Vec<&str>, StetError> {
    if text.trim().is_empty() {
        return Err(invalid("A comment needs text."));
    }
    Ok(body::lines(text))
}

/// What a new comment is, apart from where it is anchored.
pub(super) struct New<'a> {
    pub(super) id: i64,
    pub(super) text: &'a str,
    pub(super) initials: Option<&'a str>,
    /// The last `paraId` of the comment this one answers, for a reply.
    pub(super) parent_para: Option<u32>,
    /// The `w15:done` of the new row.
    pub(super) done: bool,
}

/// Writes the comment, its paragraphs and its rows, creating `comments.xml` and `commentsExtended.xml` if
/// the document has none and adding the two styles if it has neither. Returns the parts it wrote.
pub(super) fn write(
    tx: &mut PartWriter<'_, '_>,
    stamp: &RevStamp,
    new: &New<'_>,
    plan: &styles::Plan,
) -> Result<Vec<Arc<str>>, StetError> {
    let lines = check_text(new.text)?;
    let mut written: Vec<Arc<str>> = Vec::new();
    let comments = ensure_comments(tx)?;
    let extended = ensure_extended(tx)?;
    if styles::apply(tx, plan)?
        && let Some(p) = &plan.part
    {
        written.push(Arc::clone(p));
    }
    let n = names(&tx.parts())?;
    // Ids: a `paraId` for each paragraph, and a durable id if a part that keeps them exists.
    let paras = body::mint(tx, &comments, lines.len(), None)?;
    let durable = match n.ids.as_ref().or(n.extensible.as_ref()) {
        Some(part) => Some(tx.mint_id(crate::wordml::IdKind::CommentDurableId, part)?),
        None => None,
    };
    let last = *paras
        .last()
        .ok_or_else(|| internal("a comment has no paragraph"))?;
    let specs: Vec<ParaSpec> = lines
        .iter()
        .zip(&paras)
        .map(|(t, p)| ParaSpec {
            text: (*t).to_owned(),
            para_id: Some(*p),
        })
        .collect();
    let initials = new
        .initials
        .map_or_else(|| body::initials(stamp.author()), str::to_owned);
    let tree = tx.part_mut(&comments)?;
    let w = tree.root().namespace().to_owned();
    let root = tree.root().id();
    let element = body::comment(
        tree,
        &w,
        &NewComment {
            id: new.id,
            author: stamp.author(),
            date: stamp.date(),
            initials: &initials,
        },
        &specs,
        &plan.styles,
    )?;
    super::xmlw::place(tree, crate::schema::PartKind::Comments, root, element)?;
    written.push(Arc::clone(&comments));
    rows::add_extended(tx, &extended, last, new.parent_para, new.done)?;
    written.push(extended);
    if let (Some(part), Some(d)) = (&n.ids, durable) {
        rows::add_id(tx, part, last, d)?;
        written.push(Arc::clone(part));
    }
    if let (Some(part), Some(d)) = (&n.extensible, durable) {
        rows::add_extensible(tx, part, d)?;
        written.push(Arc::clone(part));
    }
    if let Some(part) = &n.people {
        rows::add_person(tx, part, stamp.author())?;
        written.push(Arc::clone(part));
    }
    Ok(written)
}

/// Adds a comment on `range` of the text of the paragraph `block`, which is a range of its `Final` text.
/// The author and date are the stamp's. `initials` default to the first letters of the author's name.
///
/// A target that is not in the body (a header, a footer, a note, a comment) is `unsupported`, and so is a
/// table or a row: name a paragraph in a cell. A range that cuts into a field, and a block that is
/// deleted, are `readonly_region`, from the compiler.
pub(crate) fn add(
    c: &mut Compiler<'_, '_>,
    stamp: &RevStamp,
    block: &BlockKey,
    range: Range<usize>,
    text: &str,
    initials: Option<&str>,
) -> Result<CommentEffect, StetError> {
    check_text(text)?;
    let (w, plan) = {
        let view = c.tx().parts();
        let n = names(&view)?;
        if **block.part() != *n.main {
            return Err(unsupported(
                "A comment can be added in the body. Headers, footers, notes and comments are not supported in this release.",
            ));
        }
        let id = view
            .id(block.part())
            .ok_or_else(|| internal("the block's part is not a part"))?;
        let story = view.story(id)?;
        let kind = story
            .index_of(block.node())
            .and_then(|i| story.blocks().get(i))
            .map(|e| e.kind);
        match kind {
            Some(BlockKind::Paragraph) => {}
            Some(_) => {
                return Err(unsupported(
                    "A comment goes on a paragraph. Name a paragraph in the cell instead of the table or the row.",
                ));
            }
            None => {
                return Err(StetError::new(
                    ErrorCode::StaleRef,
                    "That paragraph is not in the document any more.",
                ));
            }
        }
        (
            view.tree(id)?.root().namespace().to_owned(),
            styles::plan(&view, &n.main)?,
        )
    };
    let id = i64::from(c.parts_mut().next_annotation_id()?);
    let spec = marker_spec(&w, id, plan.styles.reference.as_deref());
    c.place_markers(block, range, &spec)?;
    let mut parts = vec![Arc::clone(block.part())];
    let new = New {
        id,
        text,
        initials,
        parent_para: None,
        done: false,
    };
    parts.extend(write(&mut c.parts_mut(), stamp, &new, &plan)?);
    Ok(CommentEffect {
        action: CommentAction::Added,
        id,
        thread: id,
        author: stamp.author().to_owned(),
        blocks: vec![block.clone()],
        parts,
        warnings: Vec::new(),
    })
}

/// Replies to the thread that holds the comment `comment`. The reply names the thread's root as its parent,
/// takes its place beside the root's markers, and is as resolved as the root is. A reply to a resolved
/// thread succeeds and says so. A thread whose root has no range cannot be answered: there is nothing to
/// anchor beside.
pub(crate) fn reply(
    c: &mut Compiler<'_, '_>,
    stamp: &RevStamp,
    comment: i64,
    text: &str,
    initials: Option<&str>,
) -> Result<CommentEffect, StetError> {
    check_text(text)?;
    let (root_id, root_para, resolved, anchor_part, anchor_blocks, w, plan) = {
        let view = c.tx().parts();
        let threads = Threads::of(&view)?;
        let asked = find(&threads, comment)?;
        let thread = threads
            .thread_of(asked.id)
            .ok_or_else(|| internal("a comment is in no thread"))?;
        let root = find(&threads, thread.root)?;
        let Some(anchor_part) = root.anchor.story.clone() else {
            return Err(unsupported(format!(
                "Comment c{} has no range in the text, so a reply has nothing to be placed beside.",
                root.id
            )));
        };
        let n = names(&view)?;
        let w = view
            .id(&anchor_part)
            .map(|id| view.tree(id))
            .transpose()?
            .map(|t| t.root().namespace().to_owned())
            .ok_or_else(|| internal("the anchor's part is not a part"))?;
        (
            root.id,
            root.para_id,
            thread.resolved,
            anchor_part,
            root.anchor.blocks.clone(),
            w,
            styles::plan(&view, &n.main)?,
        )
    };
    // The root needs a `paraId` to be named as a parent, and a row to say whether it is done.
    let mut writer = c.parts_mut();
    let tx = &mut writer;
    ensure_comments(tx)?;
    let extended = ensure_extended(tx)?;
    let mut parts: Vec<Arc<str>> = Vec::new();
    let root_para = match root_para {
        Some(p) => p,
        None => {
            let p = give_para_id(tx, root_id)?;
            parts.push(ensure_comments(tx)?);
            p
        }
    };
    if rows::set_done(tx, &extended, root_para, resolved)? {
        parts.push(Arc::clone(&extended));
    }
    let id = i64::from(tx.next_annotation_id()?);
    let spec = marker_spec(&w, id, plan.styles.reference.as_deref());
    c.place_markers_beside(&anchor_part, root_id, &spec)?;
    parts.push(anchor_part);
    let new = New {
        id,
        text,
        initials,
        parent_para: Some(root_para),
        done: resolved,
    };
    parts.extend(write(&mut c.parts_mut(), stamp, &new, &plan)?);
    parts.dedup();
    let mut warnings = Vec::new();
    if resolved {
        warnings.push(CommentWarning {
            code: "reply_to_resolved",
            message: format!(
                "Thread c{root_id} is resolved, and the reply is in it, still resolved. Reopen the thread if it needs attention."
            ),
        });
    }
    Ok(CommentEffect {
        action: CommentAction::Replied,
        id,
        thread: root_id,
        author: stamp.author().to_owned(),
        blocks: anchor_blocks,
        parts,
        warnings,
    })
}

/// Gives the last paragraph of comment `id` a `paraId` and a `textId`, which it lacks, and returns the
/// `paraId`. A parent cannot be named in a row without one.
pub(super) fn give_para_id(tx: &mut PartWriter<'_, '_>, id: i64) -> Result<u32, StetError> {
    let (part, node) = {
        let view = tx.parts();
        let threads = Threads::of(&view)?;
        let comment = find(&threads, id)?;
        let at = locate(&view, &threads, comment)?;
        let last = *at
            .paragraphs
            .last()
            .ok_or_else(|| internal("a comment has no paragraph"))?;
        (at.part, last)
    };
    give_para_id_at(tx, &part, node)
}

/// Gives the paragraph `node` of the comments part a `paraId` and a `textId`, and returns the `paraId`.
pub(super) fn give_para_id_at(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    node: crate::wordml::NodeId,
) -> Result<u32, StetError> {
    let para = tx.mint_id(crate::wordml::IdKind::ParaId, part)?;
    let tree = tx.part_mut(part)?;
    super::xmlw::set(tree, node, NS_W14, "paraId", &super::rows::hex(para))?;
    super::xmlw::set(tree, node, NS_W14, "textId", "77777777")?;
    Ok(para)
}
