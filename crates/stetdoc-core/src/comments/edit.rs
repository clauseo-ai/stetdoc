//! Editing the text of one's own comment.
//!
//! The paragraphs are rebuilt from the new text. The comment keeps its id, author, date and place: only the
//! text changes, and Word does not change the date of a comment whose text is edited. The last new
//! paragraph keeps the old last paragraph's `paraId`, so the rows keep their key. The change is not
//! tracked: a comment is not document text, and Word has no revision for it.

use super::body::{self, ParaSpec, Styles};
use super::errors::{internal, invalid, not_yours, unsupported};
use super::lookup::{find, locate};
use super::{CommentAction, CommentEffect, xmlw};
use crate::api::error::StetError;
use crate::revisions::compiler::Compiler;
use crate::revisions::stamp::RevStamp;
use crate::threads::Threads;
use crate::wordml::NodeId;
use crate::xml::SpanTree;
use std::sync::Arc;

/// The styles the comment's first paragraph already has, so a rebuilt comment looks as it did and no
/// style is added to the document for it.
fn styles_of(tree: &SpanTree, first: Option<NodeId>) -> Styles {
    let Some(p) = first.map(|n| tree.node_ref(n)) else {
        return Styles::default();
    };
    let text = p
        .child_elements()
        .find(|c| c.local_name() == "pPr")
        .and_then(|ppr| ppr.child_elements().find(|c| c.local_name() == "pStyle"))
        .and_then(|s| s.attr(s.namespace(), "val").map(|v| v.into_owned()));
    let reference = p
        .child_elements()
        .find(|c| c.local_name() == "r")
        .and_then(|r| r.child_elements().find(|c| c.local_name() == "rPr"))
        .and_then(|rpr| rpr.child_elements().find(|c| c.local_name() == "rStyle"))
        .and_then(|s| s.attr(s.namespace(), "val").map(|v| v.into_owned()));
    Styles { reference, text }
}

/// Rewrites the text of comment `comment`, which must be the stamp's author's.
pub(crate) fn edit(
    c: &mut Compiler<'_, '_>,
    stamp: &RevStamp,
    comment: i64,
    text: &str,
) -> Result<CommentEffect, StetError> {
    if text.trim().is_empty() {
        return Err(invalid("A comment needs text."));
    }
    let mut writer = c.parts_mut();
    let tx = &mut writer;
    let (part, element, old, para_id, thread, blocks, styles, w) = {
        let view = tx.parts();
        let threads = Threads::of(&view)?;
        let comment = find(&threads, comment)?;
        if comment.author.as_deref() != Some(stamp.author()) {
            return Err(not_yours(
                comment.id,
                comment.author.as_deref().unwrap_or("nobody"),
                stamp.author(),
            ));
        }
        let at = locate(&view, &threads, comment)?;
        let id = view
            .id(&at.part)
            .ok_or_else(|| internal("the comments part has no number"))?;
        let tree = view.tree(id)?;
        // A comment that holds a table or anything but paragraphs would lose it: its paragraphs are rebuilt.
        if tree
            .node_ref(at.element)
            .child_elements()
            .any(|c| c.local_name() != "p")
        {
            return Err(unsupported(format!(
                "Comment c{} holds more than paragraphs, a table for example, and editing its text would lose that. Delete it and add a new comment.",
                comment.id
            )));
        }
        let thread = threads.thread_of(comment.id).map_or(comment.id, |t| t.root);
        (
            at.part,
            at.element,
            at.paragraphs.clone(),
            comment.para_id,
            thread,
            comment.anchor.blocks.clone(),
            styles_of(tree, at.paragraphs.first().copied()),
            tree.root().namespace().to_owned(),
        )
    };
    let lines = body::lines(text);
    // A comment that had no `paraId` has no rows to keep in step, so its paragraphs get none.
    let ids: Vec<Option<u32>> = if para_id.is_some() {
        body::mint(tx, &part, lines.len(), para_id)?
            .into_iter()
            .map(Some)
            .collect()
    } else {
        vec![None; lines.len()]
    };
    let specs: Vec<ParaSpec> = lines
        .iter()
        .zip(ids)
        .map(|(t, p)| ParaSpec {
            text: (*t).to_owned(),
            para_id: p,
        })
        .collect();
    let tree = tx.part_mut(&part)?;
    for node in old {
        xmlw::detach(tree, node)?;
    }
    for paragraph in body::paragraphs(tree, &w, &specs, &styles)? {
        xmlw::append(tree, element, paragraph)?;
    }
    Ok(CommentEffect {
        action: CommentAction::Edited,
        id: comment,
        thread,
        author: stamp.author().to_owned(),
        blocks,
        parts: vec![Arc::clone(&part)],
        warnings: Vec::new(),
    })
}
