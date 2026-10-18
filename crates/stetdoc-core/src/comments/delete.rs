//! Deleting comments: one's own, with everything that hangs below them, and any that another operation
//! left without a reference run.

use super::errors::{internal, not_yours};
use super::lookup::{find, locate};
use super::parts::names;
use super::{CommentAction, CommentEffect, rows, xmlw};
use crate::api::error::{ErrorCode, StetError};
use crate::revisions::compiler::Compiler;
use crate::revisions::stamp::RevStamp;
use crate::threads::Threads;
use crate::wordml::{BlockKey, NodeId};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// What was known of a comment that is gone, for its effect.
pub(super) struct Gone {
    pub(super) id: i64,
    pub(super) thread: i64,
    pub(super) author: String,
    pub(super) blocks: Vec<BlockKey>,
    pub(super) parts: Vec<Arc<str>>,
}

/// Removes the comments `ids`: their range markers and reference runs from the stories, their `w:comment`
/// elements, and their rows in every extension part. A comment that survives and named a removed one as
/// its parent is moved to the nearest surviving ancestor, or becomes a root. No author is asked: the
/// callers decide who may do this.
pub(super) fn remove_comments(
    c: &mut Compiler<'_, '_>,
    ids: &[i64],
) -> Result<Vec<Gone>, StetError> {
    struct Plan {
        gone: Vec<Gone>,
        by_story: Vec<(Arc<str>, Vec<i64>)>,
        elements: Vec<NodeId>,
        comments_part: Arc<str>,
        paras: HashSet<u32>,
        durables: HashSet<u32>,
        reparent: Vec<(u32, Option<u32>)>,
        names: super::parts::Names,
    }
    let plan = {
        let view = c.tx().parts();
        let threads = Threads::of(&view)?;
        let wanted: HashSet<i64> = ids.iter().copied().collect();
        let mut gone = Vec::new();
        let mut by_story: Vec<(Arc<str>, Vec<i64>)> = Vec::new();
        let mut elements = Vec::new();
        let (mut paras, mut durables) = (HashSet::new(), HashSet::new());
        let mut comments_part = None;
        for id in ids {
            let comment = find(&threads, *id)?;
            let at = locate(&view, &threads, comment)?;
            comments_part = Some(Arc::clone(&at.part));
            elements.push(at.element);
            paras.extend(comment.para_id);
            durables.extend(comment.durable_id);
            let thread = threads.thread_of(*id).map_or(*id, |t| t.root);
            if let Some(story) = &comment.anchor.story {
                match by_story.iter_mut().find(|(s, _)| s == story) {
                    Some((_, v)) => v.push(*id),
                    None => by_story.push((Arc::clone(story), vec![*id])),
                }
            }
            gone.push(Gone {
                id: *id,
                thread,
                author: comment.author.clone().unwrap_or_default(),
                blocks: comment.anchor.blocks.clone(),
                parts: Vec::new(),
            });
        }
        // The survivors that named a removed comment: their nearest ancestor that stays.
        let parent_of: HashMap<i64, Option<i64>> = threads
            .comments()
            .iter()
            .map(|c| (c.id, c.parent))
            .collect();
        let mut reparent = Vec::new();
        for s in threads
            .comments()
            .iter()
            .filter(|c| !wanted.contains(&c.id))
        {
            let Some(p) = s.parent.filter(|p| wanted.contains(p)) else {
                continue;
            };
            let mut up = Some(p);
            let mut steps = 0;
            while let Some(a) = up.filter(|a| wanted.contains(a)) {
                up = parent_of.get(&a).copied().flatten();
                steps += 1;
                if steps > parent_of.len() {
                    up = None;
                    break;
                }
            }
            let new_parent = up.and_then(|a| threads.comment(a)).and_then(|a| a.para_id);
            if let Some(para) = s.para_id {
                reparent.push((para, new_parent));
            }
        }
        Plan {
            gone,
            by_story,
            elements,
            comments_part: comments_part.ok_or_else(|| internal("no comment to remove"))?,
            paras,
            durables,
            reparent,
            names: names(&view)?,
        }
    };
    let mut plan = plan;
    for (story, story_ids) in &plan.by_story {
        c.remove_markers_in_part(story, story_ids)?;
    }
    let mut writer = c.parts_mut();
    let tx = &mut writer;
    let tree = tx.part_mut(&plan.comments_part)?;
    for node in &plan.elements {
        xmlw::detach(tree, *node)?;
    }
    if let Some(part) = &plan.names.extended {
        rows::set_parents(tx, part, &plan.reparent)?;
        rows::remove(tx, part, "commentEx", &plan.paras, &HashSet::new())?;
    }
    if let Some(part) = &plan.names.ids {
        rows::remove(tx, part, "commentId", &plan.paras, &plan.durables)?;
    }
    if let Some(part) = &plan.names.extensible {
        rows::remove(
            tx,
            part,
            "commentExtensible",
            &HashSet::new(),
            &plan.durables,
        )?;
    }
    let mut parts = vec![Arc::clone(&plan.comments_part)];
    parts.extend(plan.names.extended.clone());
    parts.extend(plan.names.ids.clone());
    parts.extend(plan.names.extensible.clone());
    for g in &mut plan.gone {
        g.parts = parts.clone();
        for (story, story_ids) in &plan.by_story {
            if story_ids.contains(&g.id) {
                g.parts.push(Arc::clone(story));
            }
        }
    }
    Ok(plan.gone)
}

/// Deletes comment `comment` and every comment below it: its replies, and theirs. All of them must be the
/// stamp's author's; one that is not is `readonly_region`, and nothing changes. The first effect is the
/// comment that was named, and the others are what hung below it.
pub(crate) fn delete(
    c: &mut Compiler<'_, '_>,
    stamp: &RevStamp,
    comment: i64,
) -> Result<Vec<CommentEffect>, StetError> {
    let ids = {
        let view = c.tx().parts();
        let threads = Threads::of(&view)?;
        let target = find(&threads, comment)?;
        let mut all = vec![target];
        all.extend(threads.descendants_of(target.id));
        if let Some(foreign) = all
            .iter()
            .find(|c| c.author.as_deref() != Some(stamp.author()))
        {
            let author = foreign.author.as_deref().unwrap_or("nobody");
            return Err(if foreign.id == target.id {
                not_yours(target.id, author, stamp.author())
            } else {
                StetError::new(
                    ErrorCode::ReadonlyRegion,
                    format!(
                        "Comment c{} has a reply, c{}, by {author}, and deleting it would delete the reply. Reply to c{} instead, or ask {author}.",
                        target.id, foreign.id, target.id
                    ),
                )
            });
        }
        for c in &all {
            find(&threads, c.id)?;
        }
        all.iter().map(|c| c.id).collect::<Vec<_>>()
    };
    let gone = remove_comments(c, &ids)?;
    Ok(gone
        .into_iter()
        .map(|g| CommentEffect {
            action: CommentAction::Deleted,
            id: g.id,
            thread: g.thread,
            author: g.author,
            blocks: g.blocks,
            parts: g.parts,
            warnings: Vec::new(),
        })
        .collect())
}
