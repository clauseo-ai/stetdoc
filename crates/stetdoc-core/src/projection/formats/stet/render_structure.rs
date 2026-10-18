//! Structure lines: tables, rows, sections, blocks that are not shown, and the lines that open a unit.
//!
//! A table is a `t` line, a `r` line for each row, one line for each paragraph of each cell, and an end
//! line, `{/table}`, which is what makes a new line after a table unambiguous and gives nesting for free.

use super::layout::LineBuf;
use super::refs_syntax::write_ref;
use super::render_inline::{arg_quoted, cell_tags, word, word_or_quoted};
use super::render_other::margin_line;
use super::words::{break_word, change_word, mark_word, row_word, slot_word};
use crate::projection::keys::{Key, PropKey, RevRef, RowKey, SectionKey, TableKey, UnitKey};
use crate::projection::model::OpaqueBlock;

fn rev_words(buf: &mut LineBuf, rev: &RevRef) {
    buf.atomic(&format!(" #{} {}", rev.id, rev.author));
}

fn prop_tags(buf: &mut LineBuf, revs: &[PropKey]) {
    for prop in revs {
        buf.atomic(" {rev");
        word(buf, change_word(prop.kind));
        rev_words(buf, &prop.rev);
        buf.atomic("}");
    }
}

/// One structure line.
pub(super) fn structure(buf: &mut LineBuf, key: &Key) {
    match key {
        Key::Table(table) => table_line(buf, table),
        Key::Row(row) => row_line(buf, row),
        Key::EndTable => buf.atomic("{/table}"),
        Key::Section(section) => section_line(buf, section),
        Key::Opaque(opaque) => opaque_line(buf, opaque),
        Key::Unit(unit) => unit_line(buf, unit),
        Key::Anchored(text) => {
            buf.atomic("{anchored");
            arg_quoted(buf, text);
            buf.atomic("}");
        }
        Key::Margin(margin) => margin_line(buf, margin),
        _ => {}
    }
}

fn table_line(buf: &mut LineBuf, table: &TableKey) {
    if let Some(r) = table.r#ref {
        buf.atomic(&write_ref(r));
        cell_tags(buf, &table.cell);
        buf.atomic(" ");
    }
    buf.atomic(&format!("{{table cols={}", table.cols));
    if let Some(style) = &table.style {
        buf.atomic(" style=");
        buf.quoted(style);
    }
    buf.atomic("}");
    prop_tags(buf, &table.revs);
}

fn row_line(buf: &mut LineBuf, row: &RowKey) {
    if let Some(r) = row.r#ref {
        buf.atomic(&write_ref(r));
        buf.atomic(" ");
    }
    buf.atomic("{row");
    if row.header {
        word(buf, "header");
    }
    if let Some((kind, rev)) = &row.change {
        word(buf, row_word(*kind));
        rev_words(buf, rev);
    }
    buf.atomic("}");
    prop_tags(buf, &row.revs);
}

fn section_line(buf: &mut LineBuf, section: &SectionKey) {
    buf.atomic(&format!("{{section {} ends:", section.index));
    word(buf, break_word(section.break_type));
    if section.title_pg {
        word(buf, "title-page");
    }
    if let Some((kind, rev)) = &section.mark {
        word(buf, "mark");
        word(buf, mark_word(*kind));
        rev_words(buf, rev);
        let peer = rev
            .peer
            .map_or_else(|| "-".to_string(), |id| format!("#{id}"));
        buf.atomic(&format!(" peer {peer}"));
    }
    for prop in &section.revs {
        word(buf, "rev");
        word(buf, change_word(prop.kind));
        rev_words(buf, &prop.rev);
    }
    buf.atomic("}");
}

fn opaque_line(buf: &mut LineBuf, opaque: &OpaqueBlock) {
    buf.atomic("{opaque");
    match opaque {
        OpaqueBlock::AltChunk => word(buf, "altchunk"),
        OpaqueBlock::Unknown => word(buf, "unknown"),
        OpaqueBlock::Math(text) => {
            word(buf, "math");
            arg_quoted(buf, text);
        }
        OpaqueBlock::Collapsed {
            keyword,
            first,
            last,
            blocks,
        } => {
            word_or_quoted(buf, keyword, &["altchunk", "unknown", "math"]);
            match (first, last) {
                (Some(first), Some(last)) => {
                    buf.atomic(&format!(" {}..{}", write_ref(*first), write_ref(*last)));
                }
                (first, last) => {
                    if let Some(first) = first {
                        buf.atomic(&format!(" first {}", write_ref(*first)));
                    }
                    if let Some(last) = last {
                        buf.atomic(&format!(" last {}", write_ref(*last)));
                    }
                }
            }
            buf.atomic(&format!(" {blocks} paragraphs"));
        }
    }
    buf.atomic("}");
}

fn unit_line(buf: &mut LineBuf, unit: &UnitKey) {
    match unit {
        UnitKey::Note {
            endnote,
            ordinal,
            mark,
        } => {
            let mark = mark.map(|m| format!(" mark={m}")).unwrap_or_default();
            buf.atomic(&format!(
                "{{{} {}{mark}}}",
                if *endnote { "endnote" } else { "footnote" },
                ordinal
            ));
        }
        UnitKey::Header { footer, uses } => {
            buf.atomic(if *footer { "{footer" } else { "{header" });
            let mut at = 0;
            while let Some(first) = uses.get(at) {
                let run = uses
                    .iter()
                    .skip(at)
                    .take_while(|u| u.slot == first.slot)
                    .count();
                let sections: Vec<String> = uses
                    .iter()
                    .skip(at)
                    .take(run)
                    .map(|u| u.section.to_string())
                    .collect();
                word(buf, slot_word(first.slot));
                buf.atomic(&format!(" sections={}", sections.join(",")));
                at += run;
            }
            buf.atomic("}");
        }
        UnitKey::Comment {
            id,
            author,
            date,
            on,
            resolved,
            reply_to,
        } => {
            match reply_to {
                Some(parent) => buf.atomic(&format!("{{reply c{id} to c{parent}")),
                None => buf.atomic(&format!("{{comment c{id}")),
            }
            if let Some(author) = author {
                buf.atomic(&format!(" by {author}"));
            }
            if let Some(date) = date {
                buf.atomic(" at");
                arg_quoted(buf, date);
            }
            if let Some(on) = on {
                buf.atomic(&format!(" on {}", write_ref(*on)));
            }
            if *resolved {
                word(buf, "resolved");
            }
            buf.atomic("}");
        }
    }
}
