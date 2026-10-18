//! `replace_text`: the one primitive for text (design `revisions.md`, 2.2). An insertion is a range
//! `at..at`, and a deletion replaces with the empty string.
//!
//! The call builds one text map for the paragraph, diffs the old text of the range against the new,
//! checks every edit of the difference against the document before it changes anything, and then
//! applies the edits from the last to the first, so that what is still to come keeps its offsets: a cut
//! keeps the original node as the left part, and an insertion goes after what was just deleted.

use super::compiler::{Compiler, ParaView};
use super::diff::{self, Cleanup};
use super::errors::{invalid, readonly, unsupported};
use super::insert_run::check_new_text;
use super::names::is_w;
use super::outcome::Effect;
use crate::api::error::StetError;
use crate::wordml::BlockKey;
use crate::wordml::{
    ItemKind, Located, Mode, NO_FIELD, TextMap, WrapperKind, control, enclosing_controls,
};
use std::ops::Range;

/// The offset in `Final` at which each item starts.
pub(super) fn final_starts(map: &TextMap) -> Vec<usize> {
    let mut out = Vec::with_capacity(map.items().len());
    let mut at = 0usize;
    for item in map.items() {
        out.push(at);
        if item.vis.in_mode(Mode::Final) {
            at += item.len as usize;
        }
    }
    out
}

/// The character of a note's own number (`w:footnoteRef`, `w:endnoteRef`) in the text model.
const NOTE_MARK: char = '\u{3}';

/// The new text of a range with every note number of the old text kept: a text edit never deletes a note's own
/// number, as Word keeps it (a note goes only when its reference in the body goes). A number the new text lacks is
/// put back where it was, before the new text when it opened the range and after it otherwise, so that the
/// difference keeps it and writes no deletion of it.
fn keep_note_marks(old: &str, with: &str) -> String {
    let marks = old.chars().filter(|c| *c == NOTE_MARK).count();
    let have = with.chars().filter(|c| *c == NOTE_MARK).count();
    if marks <= have {
        return with.to_string();
    }
    let leading = old.chars().take_while(|c| *c == NOTE_MARK).count();
    let lead = leading.min(marks - have);
    let trail = marks - have - lead;
    let mut out = String::with_capacity(with.len() + marks);
    out.extend(std::iter::repeat_n(NOTE_MARK, lead));
    out.push_str(with);
    out.extend(std::iter::repeat_n(NOTE_MARK, trail));
    out
}

/// One edit of the difference, in offsets of the paragraph's `Final` text.
pub(super) struct Piece {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) with: String,
}

impl Compiler<'_, '_> {
    /// Replaces the characters `range` of the paragraph's `Final` text with `with`. See the design for
    /// what each kind of text becomes. A refused call has changed nothing.
    pub(crate) fn replace_text(
        &mut self,
        block: &BlockKey,
        range: Range<usize>,
        with: &str,
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.replace_text_inner(block, range, with) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn replace_text_inner(
        &mut self,
        block: &BlockKey,
        range: Range<usize>,
        with: &str,
    ) -> Result<(), StetError> {
        let (p, pieces) = self.plan_text(block, range, with, true)?;
        if pieces.is_empty() {
            return Ok(());
        }
        // From the last edit to the first, on the one map.
        let starts = final_starts(&p.map);
        for piece in pieces.iter().rev() {
            self.apply_piece(&p, &starts, piece)?;
        }
        self.touch(block);
        Ok(())
    }

    /// Every check that can refuse the call, run before anything changes, and the difference as a list of
    /// pieces in offsets of the paragraph's `Final` text. With `keep_marks` a note's own number is never deleted, as a text
    /// edit of a note keeps it; without, it goes with the text, as when the whole note is deleted with its reference (`RP050`).
    pub(super) fn plan_text(
        &mut self,
        block: &BlockKey,
        range: Range<usize>,
        with: &str,
        keep_marks: bool,
    ) -> Result<(ParaView, Vec<Piece>), StetError> {
        let p = self.paragraph(block)?;
        // before the range, which a paragraph that is gone in `Final` has none of
        self.check_not_wrapped_away(&p)?;
        let final_len = p.map.len(Mode::Final);
        if range.start > range.end || range.end > final_len {
            return Err(invalid(format!(
                "the range {}..{} is outside the paragraph, which has {final_len} characters",
                range.start, range.end
            )));
        }
        self.check_editable(&p)?;
        check_carriable(with)?;
        let old: String = p
            .map
            .text(Mode::Final)
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect();
        let with = if keep_marks {
            keep_note_marks(&old, with)
        } else {
            with.to_string()
        };
        let pieces: Vec<Piece> = diff::edits(&old, &with, Cleanup::Readable)
            .into_iter()
            .map(|e| Piece {
                start: range.start + e.range.start,
                end: range.start + e.range.end,
                with: e.with,
            })
            .collect();
        for piece in &pieces {
            check_new_text(&piece.with)?;
            self.preflight(&p, piece)?;
        }
        Ok((p, pieces))
    }

    /// Refuses an edit of a paragraph that is deleted or sits in a locked or placeholder block-level
    /// control.
    pub(super) fn check_editable(&self, p: &ParaView) -> Result<(), StetError> {
        use crate::wordml::{MARK_DELETED, MARK_MOVED_FROM};
        let marked = p.flags & (MARK_DELETED | MARK_MOVED_FROM) != 0;
        if (marked || p.in_deleted_row_or_cell) && p.map.len(Mode::Final) == 0 {
            return Err(readonly(
                "this paragraph is deleted by a tracked change, so it cannot be edited. Reject that change first, or edit another paragraph.",
            ));
        }
        let tree = self.tree_ro(&p.part)?;
        for sdt in enclosing_controls(tree.node_ref(p.node)) {
            let c = control(tree.node_ref(sdt));
            if c.lock.locks_content() || c.data_bound || c.showing_placeholder {
                return Err(readonly(
                    "this paragraph is in a content control that is locked, bound to data or showing its placeholder, so its text cannot be edited",
                ));
            }
        }
        Ok(())
    }

    /// Refuses a text or property edit of a paragraph inside a tracked deletion or move away that holds whole blocks: a
    /// `w:del` or `w:moveFrom` between the paragraph and its container. The paragraph is gone in `Final`, so what an
    /// edit wrote there would not show, as for deleted text. Inside a `w:ins` or `w:moveTo` the paragraph is live.
    pub(super) fn check_not_wrapped_away(&self, p: &ParaView) -> Result<(), StetError> {
        let tree = self.tree_ro(&p.part)?;
        let container = super::blocks::container_of(tree, p.node);
        let mut cur = tree.node_ref(p.node).parent();
        while let Some(n) = cur {
            if Some(n.id()) == container {
                break;
            }
            if is_w(&n, "moveFrom") {
                return Err(readonly(
                    "this paragraph is in a tracked move, on the side it was moved away from, so it cannot be edited. Edit it where it was moved to, or reject that change first.",
                ));
            }
            if is_w(&n, "del") {
                return Err(readonly(
                    "this paragraph is deleted by a tracked change, so it cannot be edited. Reject that change first, or edit another paragraph.",
                ));
            }
            cur = n.parent();
        }
        Ok(())
    }

    /// Every check that can refuse an edit, run before anything changes.
    fn preflight(&self, p: &ParaView, piece: &Piece) -> Result<(), StetError> {
        let map = &p.map;
        if piece.start < piece.end {
            let loc = map
                .locate(Mode::Final, piece.start..piece.end)
                .ok_or_else(|| invalid("a range lies outside the paragraph"))?;
            let loc = cover(p, loc)?;
            self.check_items(p, loc.items.clone())?;
            if self.tracked() {
                self.check_deletable(p, loc.items.clone())?;
            }
            self.refuse_foreign_items(p, loc.items.clone())?;
        }
        if !piece.with.is_empty() || piece.start == piece.end {
            let gap = map
                .gap(Mode::Final, piece.start)
                .ok_or_else(|| invalid("an offset lies outside the paragraph"))?;
            let neighbours = [gap.left, gap.right].into_iter().flatten();
            for i in neighbours {
                self.check_items(p, i..i + 1)?;
            }
            self.refuse_foreign_gap(p, gap.left, gap.right)?;
        }
        Ok(())
    }

    /// Refuses a range of items that are in something this release does not edit: a locked or bound
    /// control, ruby text, alternate content, or a simple field.
    pub(super) fn check_items(&self, p: &ParaView, items: Range<usize>) -> Result<(), StetError> {
        let tree = self.tree_ro(&p.part)?;
        for i in items {
            let Some(item) = p.map.items().get(i) else {
                continue;
            };
            for ctx in p.map.context(item.ctx) {
                match ctx.kind {
                    WrapperKind::Control => {
                        let c = control(tree.node_ref(ctx.node));
                        if c.lock.locks_content() || c.data_bound || c.showing_placeholder {
                            return Err(readonly(
                                "this text is in a content control that is locked, bound to data or showing its placeholder, so it cannot be edited",
                            ));
                        }
                    }
                    WrapperKind::Alternate | WrapperKind::Ruby => {
                        return Err(unsupported(
                            "this text is inside ruby text or alternate content, which this release does not edit",
                        ));
                    }
                    WrapperKind::FieldSimple => {
                        return Err(readonly(
                            "this text is the result of a simple field, which is read-only",
                        ));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

impl Compiler<'_, '_> {
    /// Refuses a tracked deletion of an equation that already holds a tracked deletion (RP013: Word deletes a
    /// math run with a `w:del` inside its `m:r`). An equation is deleted whole, with a `w:del` around it, and
    /// that would put a `w:del` inside a `w:del` (W5); the deletion inside is another revision, which is
    /// never edited.
    fn check_deletable(&self, p: &ParaView, items: Range<usize>) -> Result<(), StetError> {
        let tree = self.tree_ro(&p.part)?;
        for i in items {
            let Some(item) = p.map.items().get(i) else {
                continue;
            };
            if item.kind != crate::wordml::ItemKind::Math {
                continue;
            }
            let mut stack: Vec<_> = tree.node_ref(item.node).child_elements().collect();
            while let Some(n) = stack.pop() {
                if is_w(&n, "del") {
                    return Err(readonly(
                        "this equation holds a tracked deletion, so it cannot be deleted as a tracked change. Accept or reject that deletion first, or edit around the equation.",
                    ));
                }
                stack.extend(n.child_elements());
            }
        }
        Ok(())
    }
}

/// What a range covers, widened to whole fields. A field is one unit: its begin, its code, its separator,
/// its result and its end are covered together or not at all, and a range that covers all the visible
/// characters of a field's result covers the field, though the begin and the end have no width and lie at
/// the range's edges. A range that covers part of a field, or only part of a field's result, is
/// `readonly_region`, as is one that touches a field that spans paragraphs.
pub(super) fn cover(p: &ParaView, mut loc: Located) -> Result<Located, StetError> {
    let items = p.map.items();
    let fields = p.map.fields();
    if fields.is_empty() {
        return Ok(loc);
    }
    let in_final = |i: usize| {
        items
            .get(i)
            .is_some_and(|it| it.len > 0 && it.vis.in_mode(Mode::Final))
    };
    // The fields that a covered character is in, and the fields they are nested in.
    let mut touched: Vec<u32> = Vec::new();
    for s in &loc.spans {
        let Some(item) = items.get(s.item) else {
            continue;
        };
        let mut id = (item.field != NO_FIELD).then_some(item.field);
        while let Some(f) = id {
            if !touched.contains(&f) {
                touched.push(f);
            }
            id = fields.iter().find(|r| r.id == f).and_then(|r| r.parent);
        }
    }
    let position = |node: crate::wordml::NodeId, kind: ItemKind| {
        items.iter().position(|i| i.node == node && i.kind == kind)
    };
    let (mut lo, mut hi) = (loc.items.start, loc.items.end);
    for f in fields {
        let begin = position(f.begin, ItemKind::FieldBegin);
        let end = f.end.and_then(|e| position(e, ItemKind::FieldEnd));
        let begin_in = begin.is_some_and(|b| loc.items.contains(&b));
        let end_in = end.is_some_and(|e| loc.items.contains(&e));
        if !(touched.contains(&f.id) || begin_in || end_in) {
            continue;
        }
        let whole = match (begin, end, f.begins_here) {
            (Some(b), Some(e), true) => {
                (b + 1..e).all(|i| !in_final(i) || loc.spans.iter().any(|s| s.item == i && s.whole))
            }
            _ => false,
        };
        let (Some(b), Some(e), true) = (begin, end, whole) else {
            return Err(readonly(
                "the range covers only part of a field. A field is deleted whole, from its first character to its last, or not at all.",
            ));
        };
        lo = lo.min(b);
        hi = hi.max(e + 1);
    }
    loc.items = lo..hi;
    Ok(loc)
}

/// Refuses text that XML cannot carry, and a carriage return, which is not a character of the text
/// model. Atoms are checked after the difference, since an atom that is kept is not written.
pub(super) fn check_carriable(text: &str) -> Result<(), StetError> {
    for c in text.chars() {
        let atom = matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}');
        let ok = matches!(c, '\t' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}');
        if !atom && !ok {
            return Err(invalid(format!(
                "the new text holds U+{:04X}, which cannot be written in a Word document. Use \\n for a line break and \\t for a tab.",
                u32::from(c)
            )));
        }
    }
    Ok(())
}
