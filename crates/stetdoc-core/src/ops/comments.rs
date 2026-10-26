//! The comment operations and `delete-note`: `comment`, `reply`, `resolve`, `edit-comment`, `delete-comment` (design
//! `document-and-ops.md` 2.3, and `comments-and-notes.md` 4 and 6).
//!
//! Each resolves what the payload names and calls one function of `comments`, which writes through the compiler. A comment is
//! named `c3`, its `w:id`. A note is named `fn3` or `en2`, its kind and its ordinal as the view shows it, which `document` turns
//! into the note's `w:id` before the batch runs (`Context::notes`), because the ordinals are `projection`'s.
//!
//! For the scope guard each operation names the paragraphs it reaches: the paragraph a new comment is anchored to, the anchor of
//! the thread a reply joins, and the anchor of a comment that `resolve`, `edit-comment` and `delete-comment` change. `resolve` and
//! `edit-comment` touch no document block, so the guard reads their anchor from these targets. An unanchored comment names none
//! and is in any scope. The paragraphs of a deleted note are owned by the operation, which names the paragraphs of its
//! references.

use super::context::{Context, Done, SideEffect};
use super::target::{self, Within, warning};
use crate::api::error::{ErrorCode, StetError};
use crate::api::op::{CommentOp, DeleteCommentOp, DeleteNoteOp, EditCommentOp, ReplyOp, ResolveOp};
use crate::api::view::{Warning, WarningCode};
use crate::comments::{self, CommentAction, CommentEffect, CommentWarning};
use crate::refs::BaseToken;
use crate::revisions::compiler::Compiler;
use crate::revisions::outcome::Effect;
use crate::state::Tx;
use crate::wordml::{BlockKey, StoryKind};

fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The `w:id` of a comment named `c3`.
pub(super) fn comment_id(text: &str) -> Result<i64, StetError> {
    text.trim()
        .strip_prefix('c')
        .and_then(|n| n.parse::<i64>().ok())
        .filter(|n| *n >= 0)
        .ok_or_else(|| {
            invalid(format!(
                "`{text}` is not a comment. A comment is named as the view shows it, such as c3."
            ))
        })
}

/// The kind and the ordinal of a note named `fn3` or `en2`: true for an endnote.
pub(super) fn note_name(text: &str) -> Result<(bool, u32), StetError> {
    let t = text.trim();
    let (endnote, rest) = match (t.strip_prefix("fn"), t.strip_prefix("en")) {
        (Some(n), _) => (false, n),
        (_, Some(n)) => (true, n),
        _ => (false, ""),
    };
    rest.parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .map(|n| (endnote, n))
        .ok_or_else(|| {
            invalid(format!(
                "`{text}` is not a note. A note is named as the view shows it, such as fn3 or en2."
            ))
        })
}

/// A warning of `comments`, as the receipt writes it.
pub(crate) fn comment_warning(w: &CommentWarning) -> Warning {
    let code = match w.code {
        "already_resolved" => WarningCode::AlreadyResolved,
        "reply_to_resolved" => WarningCode::ReplyToResolved,
        "anchor_emptied" => WarningCode::AnchorEmptied,
        "removed_with_text" => WarningCode::CommentRemoved,
        _ => WarningCode::ReadAnomaly,
    };
    Warning {
        code,
        block: None,
        story: None,
        message: w.message.clone(),
        next: None,
    }
}

/// The parts of the comments story, which no scope can name: `comments` creates and rewrites their paragraphs.
pub(super) fn comment_parts(tx: &Tx<'_>) -> Vec<std::sync::Arc<str>> {
    tx.parts()
        .story_specs()
        .map(|s| {
            s.0.iter()
                .filter(|spec| spec.kind == StoryKind::Comments)
                .map(|spec| spec.part.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// The two parts that adding a part changes: `[Content_Types].xml` and the relationships of the main part.
fn added_part_entries(tx: &Tx<'_>) -> Vec<(String, String)> {
    let main = tx
        .parts()
        .story_specs()
        .ok()
        .and_then(|s| {
            s.0.iter()
                .find(|spec| spec.kind == StoryKind::Body)
                .map(|spec| spec.part.to_string())
        })
        .unwrap_or_else(|| "word/document.xml".to_string());
    let rels = match main.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{main}.rels"),
    };
    vec![
        (
            "[Content_Types].xml".to_string(),
            "the content type of a comment part that was added".to_string(),
        ),
        (
            rels,
            "the relationship to a comment part that was added".to_string(),
        ),
    ]
}

/// The name of the styles part, and the ids of the styles it defines.
pub(super) fn style_ids(tx: &Tx<'_>) -> Option<(String, Vec<String>)> {
    let view = tx.parts();
    let specs = view.story_specs().ok()?;
    let body = specs.0.iter().find(|s| s.kind == StoryKind::Body)?;
    let main = view.id(&body.part)?;
    let id = view
        .related_part(main, &|t| t.ends_with("/relationships/styles"))
        .ok()??;
    let name = view.name(id)?.to_string();
    let tree = view.tree(id).ok()?;
    let ids = tree
        .root()
        .child_elements()
        .filter(|c| c.local_name() == "style")
        .filter_map(|c| {
            c.attrs()
                .find(|a| a.local_name() == "styleId")
                .map(|a| a.value().into_owned())
        })
        .collect();
    Some((name, ids))
}

/// What a comment operation hands back: its anchor as targets, the parts it wrote as side effects, and the effect itself. The
/// styles part is declared by the styles the operation appended, `before` being the ids it had before the call.
fn done_of(
    tx: &Tx<'_>,
    effects: Vec<CommentEffect>,
    extra: &[BlockKey],
    before: &Option<(String, Vec<String>)>,
) -> Done {
    let after = style_ids(tx);
    let appended = |part: &str| -> Option<Vec<String>> {
        let (name, now) = after.as_ref()?;
        if name != part {
            return None;
        }
        let was: &[String] = before.as_ref().map_or(&[], |(_, ids)| ids.as_slice());
        Some(now.iter().filter(|id| !was.contains(id)).cloned().collect())
    };
    let stories: Vec<std::sync::Arc<str>> = tx
        .parts()
        .story_specs()
        .map(|s| s.0.iter().map(|spec| spec.part.clone()).collect())
        .unwrap_or_default();
    let mut done = Done::naming(&extra.iter().collect::<Vec<_>>());
    for effect in &effects {
        for key in &effect.blocks {
            if !done.targets.contains(key) {
                done.targets.push(key.clone());
            }
        }
        for part in &effect.parts {
            if !stories.contains(part) {
                done.side_effects.push(SideEffect {
                    part: part.to_string(),
                    why: format!("the comment c{} was written there", effect.id),
                    styles: appended(part),
                });
            }
            // A part that `comments` added has a content type and a relationship from the main part.
            if tx.base().parts().id(part).is_none() {
                for (name, why) in added_part_entries(tx) {
                    if !done.side_effects.iter().any(|s| s.part == name) {
                        done.side_effects.push(SideEffect {
                            part: name,
                            why,
                            styles: None,
                        });
                    }
                }
            }
        }
        done.warnings
            .extend(effect.warnings.iter().map(comment_warning));
    }
    // A resolve of a thread that is already as asked changes nothing, and says so.
    done.changed = effects
        .iter()
        .any(|e| !e.warnings.iter().any(|w| w.code == "already_resolved"));
    // The paragraphs whose markers were written or removed are what the operation changed in the document. `resolve` and
    // `edit-comment` change none of them.
    let touched: Vec<BlockKey> = effects
        .iter()
        .filter(|e| {
            matches!(
                e.action,
                CommentAction::Added | CommentAction::Replied | CommentAction::Deleted
            )
        })
        .flat_map(|e| e.blocks.iter().cloned())
        .collect();
    if !touched.is_empty() {
        done.effects.push(Effect {
            touched,
            ..Effect::default()
        });
    }
    done.comments = effects;
    done
}

/// `comment`: a new comment on a range of a paragraph, or on the whole paragraph.
pub(super) fn comment(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &CommentOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    let para = target::paragraph(c.tx(), &key, &op.r#ref)?;
    let within = Within {
        find: op.find.as_deref(),
        occurrence: op.occurrence,
        start: op.start,
        end: op.end,
    };
    let found = target::range(c.tx(), &para, &op.r#ref, &within, cx.unit)?;
    let loose = found.loose;
    let before = style_ids(c.tx());
    let effect = comments::add::add(
        c,
        cx.stamp,
        &key,
        found.range,
        &op.text,
        op.initials.as_deref(),
    )?;
    let mut done = done_of(c.tx(), vec![effect], std::slice::from_ref(&key), &before);
    if loose {
        done.warnings.push(warning(
            WarningCode::FindMatchedLoosely,
            c.tx(),
            Some(&key),
            "`find` matched only after curly and straight quotes were treated as equal.",
        ));
    }
    Ok(done)
}

/// `reply`: a reply to the thread of any of its comments.
pub(super) fn reply(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    op: &ReplyOp,
) -> Result<Done, StetError> {
    let id = comment_id(&op.comment)?;
    let before = style_ids(c.tx());
    let effect = comments::add::reply(c, cx.stamp, id, &op.text, None)?;
    Ok(done_of(c.tx(), vec![effect], &[], &before))
}

/// `resolve`: resolves a thread, or reopens it.
pub(super) fn resolve(c: &mut Compiler<'_, '_>, op: &ResolveOp) -> Result<Done, StetError> {
    let id = comment_id(&op.comment)?;
    let before = style_ids(c.tx());
    let effect = comments::resolve::set_resolved(c, id, op.reopen != Some(true))?;
    Ok(done_of(c.tx(), vec![effect], &[], &before))
}

/// `edit-comment`: the text of one of the author's own comments.
pub(super) fn edit_comment(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    op: &EditCommentOp,
) -> Result<Done, StetError> {
    let id = comment_id(&op.comment)?;
    let before = style_ids(c.tx());
    let effect = comments::edit::edit(c, cx.stamp, id, &op.text)?;
    Ok(done_of(c.tx(), vec![effect], &[], &before))
}

/// `delete-comment`: one of the author's own comments, with the comments below it.
pub(super) fn delete_comment(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    op: &DeleteCommentOp,
) -> Result<Done, StetError> {
    let id = comment_id(&op.comment)?;
    let before = style_ids(c.tx());
    let effects = comments::delete::delete(c, cx.stamp, id)?;
    Ok(done_of(c.tx(), effects, &[], &before))
}

/// `delete-note`: a footnote or endnote and its references. The paragraphs of the references are its targets, and the note's own
/// paragraphs are owned: no scope can name them.
pub(super) fn delete_note(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    op: &DeleteNoteOp,
) -> Result<Done, StetError> {
    let (endnote, ordinal) = note_name(&op.note)?;
    let id = cx
        .notes
        .iter()
        .find(|(e, n, _)| *e == endnote && *n == ordinal)
        .map(|(_, _, id)| *id)
        .ok_or_else(|| {
            StetError::new(
                ErrorCode::NotFound,
                format!(
                    "the document has no {} {}: the body refers to fewer.",
                    if endnote { "endnote" } else { "footnote" },
                    op.note.trim()
                ),
            )
            .with_next("Name a note as the view shows it, such as {fn3}.")
        })?;
    let before = c.tx().minted_annotation_ids();
    let deleted = comments::notes::delete_note(c, endnote, id)?;
    // The calls `comments` made are one effect here: the blocks it changed and the revision ids it took. Before the first id of
    // the transaction the range is `0..0`, and its end is not where this operation's ids start: the first id is the highest
    // `w:id` of the document plus one, which can be past two billion, and every id of a range is counted later.
    let now = c.tx().minted_annotation_ids();
    let minted = if before.is_empty() {
        now
    } else {
        before.end..now.end
    };
    let notes_parts: Vec<std::sync::Arc<str>> = c
        .tx()
        .parts()
        .story_specs()
        .map(|s| {
            s.0.iter()
                .filter(|spec| matches!(spec.kind, StoryKind::Footnotes | StoryKind::Endnotes))
                .map(|spec| spec.part.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut done = Done::default();
    done.effects.push(Effect {
        minted,
        touched: deleted.blocks.clone(),
        ..Effect::default()
    });
    for key in deleted.blocks {
        if notes_parts.contains(key.part()) {
            done.owned.push(key);
        } else if !done.targets.contains(&key) {
            done.targets.push(key);
        }
    }
    done.warnings
        .extend(deleted.warnings.iter().map(comment_warning));
    done.changed = true;
    Ok(done)
}
