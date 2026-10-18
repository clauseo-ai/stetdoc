//! A paragraph line: its ref, its tags, and its slots and seams as text and markers.
//!
//! Every non-text thing is a marker in braces. A text that a marker shows is a quoted string, and a
//! name that is not text (a keyword, an id, an author's code, a ref) is bare. The text between markers
//! is a slot, written with the escapes of `escape.rs`, so a marker is found without lookahead: a raw
//! brace is always a marker's.

use super::layout::LineBuf;
use super::refs_syntax::write_ref;
use super::words::{
    auto_word, cell_word, change_word, control_word, note_letters, object_word, signal_word,
    state_word,
};
use crate::projection::draft::DraftBlock;
use crate::projection::keys::{
    ControlKey, FieldKey, Key, LabelKey, ObjectKey, PropKey, RevRef, Span,
};
use crate::projection::model::MarkKind;

/// A bare word for the buffer, with the space in front of it.
pub(super) fn word(buf: &mut LineBuf, w: &str) {
    buf.atomic(&format!(" {w}"));
}

/// A quoted string for the buffer, with the space in front of it.
pub(super) fn arg_quoted(buf: &mut LineBuf, s: &str) {
    buf.atomic(" ");
    buf.quoted(s);
}

/// A bare word if it is one that a reader can tell from the next token, and a quoted string if not.
pub(super) fn word_or_quoted(buf: &mut LineBuf, w: &str, reserved: &[&str]) {
    let bare = !w.is_empty()
        && !reserved.contains(&w)
        && w.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        word(buf, w);
    } else {
        arg_quoted(buf, w);
    }
}

/// The id and the author of a change, and for a move the other end: ` #12 JD`, ` #12 JD move from #13`.
fn rev_words(buf: &mut LineBuf, rev: &RevRef, direction: Option<&str>) {
    buf.atomic(&format!(" #{} {}", rev.id, rev.author));
    if let Some(direction) = direction {
        let peer = rev
            .peer
            .map_or_else(|| "-".to_string(), |id| format!("#{id}"));
        buf.atomic(&format!(" move {direction} {peer}"));
    }
}

fn prop_tag(buf: &mut LineBuf, prop: &PropKey) {
    buf.atomic(" {rev");
    word(buf, change_word(prop.kind));
    rev_words(buf, &prop.rev, None);
    buf.atomic("}");
}

/// The tags of a paragraph and of a table line: each in braces, with a space in front.
pub(super) fn tag(buf: &mut LineBuf, key: &Key) {
    match key {
        Key::Style(name) => {
            buf.atomic(" {style");
            arg_quoted(buf, name);
            buf.atomic("}");
        }
        Key::Label(label) => label_tag(buf, label),
        Key::Mark { kind, rev } => {
            let (sign, direction) = match kind {
                MarkKind::Inserted => ("+", None),
                MarkKind::Deleted => ("-", None),
                MarkKind::MovedTo => ("+", Some("from")),
                MarkKind::MovedFrom => ("-", Some("to")),
            };
            buf.atomic(&format!(" {{¶{sign}"));
            rev_words(buf, rev, direction);
            buf.atomic("}");
        }
        Key::Rev(prop) => prop_tag(buf, prop),
        Key::HiddenPara => buf.atomic(" {hidden-para}"),
        Key::JoinsNext => buf.atomic(" {joins-next}"),
        Key::CellPlus => buf.atomic(" {+}"),
        Key::CellSpan(n) => buf.atomic(&format!(" {{span {n}}}")),
        Key::CellVMerge { restart: true } => buf.atomic(" {vmerge restart}"),
        Key::CellVMerge { restart: false } => buf.atomic(" {vmerge}"),
        Key::CellChange { kind, rev } => {
            buf.atomic(" {cell");
            word(buf, cell_word(*kind));
            rev_words(buf, rev, None);
            buf.atomic("}");
        }
        _ => {}
    }
}

fn label_tag(buf: &mut LineBuf, label: &LabelKey) {
    if label.bullet {
        buf.atomic(" {•");
        if &*label.text != "•" {
            arg_quoted(buf, &label.text);
        }
    } else {
        buf.atomic(" {§");
        buf.quoted(&label.text);
    }
    if label.ghost {
        word(buf, "ghost");
    }
    if label.hidden {
        word(buf, "hidden");
    }
    buf.atomic("}");
}

/// The cell tags of a table line, which is a block of a cell.
pub(super) fn cell_tags(buf: &mut LineBuf, cell: &[Key]) {
    for key in cell {
        tag(buf, key);
    }
}

/// One paragraph line, from the ref to the last marker. Nothing is written after the last tag when the
/// text is empty, and no space at the end of the line. A paragraph with no ref (a block that a dry run created, which no
/// commit has numbered) still shows its tags, first on the line, so that it reads as the real edit's line without the ref.
pub(super) fn paragraph(buf: &mut LineBuf, block: &DraftBlock) {
    if let Some(r) = block.r#ref {
        buf.atomic(&write_ref(r));
        for key in &block.tags {
            tag(buf, key);
        }
    } else {
        let mut tags = LineBuf::new(false);
        for key in &block.tags {
            tag(&mut tags, key);
        }
        let tags = tags.text().trim_start();
        if !tags.is_empty() {
            buf.atomic(tags);
        }
    }
    let lead = block.r#ref.is_some() || buf.chars() > 0;
    let empty = block.seams.is_empty() && block.texts.iter().all(String::is_empty);
    if empty {
        return;
    }
    if lead {
        buf.atomic(" ");
    }
    for (index, text) in block.texts.iter().enumerate() {
        let slot = u32::try_from(index).ok();
        let mut chars = text.chars();
        match chars.next() {
            Some(first) if index == 0 && !lead && starts_like_a_ref(text) => {
                // A line with no ref whose text starts like a ref would be read as a ref: the first
                // character is written as an escape, which reads back as itself.
                buf.atomic(&format!("\\u{{{:x}}}", first as u32));
                buf.content(chars.as_str(), false, None);
            }
            _ => buf.content(text, false, slot),
        }
        if let Some(seam) = block.seams.get(index) {
            seam_marker(buf, &seam.key);
        }
    }
}

fn starts_like_a_ref(text: &str) -> bool {
    super::refs_syntax::split_ref(text).is_some()
}

/// A marker for one seam.
pub(super) fn seam_marker(buf: &mut LineBuf, key: &Key) {
    match key {
        Key::Open(Span::Ins(_) | Span::MoveIn(_)) => buf.atomic("{++"),
        Key::Close(Span::Ins(rev)) => {
            buf.atomic("++}{");
            buf.atomic(&format!("#{} {}", rev.id, rev.author));
            buf.atomic("}");
        }
        Key::Close(Span::MoveIn(rev)) => {
            buf.atomic("++}{");
            buf.atomic(&format!("#{} {}", rev.id, rev.author));
            rev_move(buf, rev, "from");
            buf.atomic("}");
        }
        Key::Deleted { moved, rev, text } => {
            buf.atomic("{--");
            buf.content(text, false, None);
            buf.atomic("--}{");
            buf.atomic(&format!("#{} {}", rev.id, rev.author));
            if *moved {
                rev_move(buf, rev, "to");
            }
            buf.atomic("}");
        }
        Key::Open(Span::Link(url)) => {
            buf.atomic("{link");
            arg_quoted(buf, url);
            buf.atomic("}");
        }
        Key::Close(Span::Link(_)) => buf.atomic("{/link}"),
        Key::Open(Span::Control(control)) => {
            buf.atomic("{ctl");
            control_words(buf, control);
            buf.atomic("}");
        }
        Key::Close(Span::Control(_)) => buf.atomic("{/ctl}"),
        Key::Control { info, state, text } => {
            buf.atomic("{ctl");
            control_words(buf, info);
            if let Some(state) = state_word(*state) {
                word(buf, state);
            }
            arg_quoted(buf, text);
            buf.atomic("}");
        }
        Key::Field(field) => field_marker(buf, field),
        Key::Hidden(text) => {
            buf.atomic("{hidden");
            arg_quoted(buf, text);
            buf.atomic("}");
        }
        Key::LooksHidden { signal, text } => {
            buf.atomic("{looks-hidden");
            word(buf, signal_word(*signal));
            arg_quoted(buf, text);
            buf.atomic("}");
        }
        Key::Comment { id, edge } => {
            let slash = if *edge == crate::projection::keys::Edge::End {
                "/"
            } else {
                ""
            };
            buf.atomic(&format!("{{{slash}c{id}}}"));
        }
        Key::NoteRef { kind, ordinal } => {
            buf.atomic(&format!("{{{}{}}}", note_letters(*kind), ordinal));
        }
        Key::NoteMark(kind) => buf.atomic(&format!("{{{}.}}", note_letters(*kind))),
        Key::Object(object) => object_marker(buf, object),
        Key::Math(text) => {
            buf.atomic("{math");
            arg_quoted(buf, text);
            buf.atomic("}");
        }
        Key::PageBreak => buf.atomic("{pagebreak}"),
        Key::ColumnBreak => buf.atomic("{colbreak}"),
        Key::Auto(kind) => buf.atomic(&format!("{{{}}}", auto_word(*kind))),
        _ => {}
    }
}

fn rev_move(buf: &mut LineBuf, rev: &RevRef, direction: &str) {
    let peer = rev
        .peer
        .map_or_else(|| "-".to_string(), |id| format!("#{id}"));
    buf.atomic(&format!(" move {direction} {peer}"));
}

fn control_words(buf: &mut LineBuf, control: &ControlKey) {
    word(buf, control_word(control.kind));
    if let Some(alias) = &control.alias {
        arg_quoted(buf, alias);
    }
    if let Some(tag) = &control.tag {
        buf.atomic(" tag=");
        buf.quoted(tag);
    }
}

fn field_marker(buf: &mut LineBuf, field: &FieldKey) {
    buf.atomic("{field");
    word_or_quoted(buf, &field.keyword, &[]);
    match (field.opens, field.closes) {
        (true, true) => {}
        (true, false) => word(buf, "start"),
        (false, false) => word(buf, "more"),
        (false, true) => word(buf, "end"),
    }
    arg_quoted(buf, &field.text);
    if let Some(target) = field.target {
        buf.atomic(&format!(" ->{}", write_ref(target)));
    }
    if field.dirty {
        word(buf, "dirty");
    }
    buf.atomic("}");
}

fn object_marker(buf: &mut LineBuf, object: &ObjectKey) {
    buf.atomic("{");
    buf.atomic(object_word(object.kind));
    if let Some(alt) = &object.alt {
        arg_quoted(buf, alt);
    }
    if let Some(text) = &object.text_box {
        word(buf, "box");
        arg_quoted(buf, text);
    }
    buf.atomic("}");
}
