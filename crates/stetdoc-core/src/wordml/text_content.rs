//! What each kind of child of a paragraph or of a run becomes in a [`super::text_model::TextMap`].
//!
//! The rows of `docs/design/wordml.md` 4.1 and the shapes of 4.5 are here, one match arm each. An
//! element the builder does not know is skipped, with its content, and counted: descending into
//! unknown markup would put text we do not understand into the paragraph.

use super::anomaly::AnomalyKind;
use super::mc;
use super::names::{M, XML, is_in, is_word_ns, w_attr, w_child};
use super::text_build::{Builder, Frame, FrameKind, Info, displayed};
use super::text_fields::{Instr, Sep};
use super::text_model::{CDATA, ItemKind, WrapperKind};
use crate::xml::{NodeKind, NodeRef};
use std::ops::Range;

/// Content-level elements that carry nothing the model keeps and are not an anomaly.
const SILENT: [&str; 14] = [
    "proofErr",
    "customXmlInsRangeStart",
    "customXmlInsRangeEnd",
    "customXmlDelRangeStart",
    "customXmlDelRangeEnd",
    "customXmlMoveFromRangeStart",
    "customXmlMoveFromRangeEnd",
    "customXmlMoveToRangeStart",
    "customXmlMoveToRangeEnd",
    "smartTagPr",
    "customXmlPr",
    "rPr",
    "sdtPr",
    "sdtEndPr",
];

/// Run-level elements that carry nothing the model keeps and are not an anomaly.
const SILENT_IN_RUN: [&str; 6] = [
    "annotationRef",
    "lastRenderedPageBreak",
    "separator",
    "continuationSeparator",
    "fitText",
    "contentPart",
];

/// The characters of a `w:t` node that count. The choice of what counts sits in this one function
/// (`docs/design/wordml.md` 4.7): every character does, with or without `xml:space="preserve"`, because Word
/// shows the white space at the ends of a `w:t` that does not say it (the hand check of 2026-10-10, pack file 37),
/// where LibreOffice drops it (a documented deviation, `FIDELITY.md`), so `xml:space` is no input. Returns the range of
/// the node's characters that count. It feeds `Item.from` and `Item.len`, so a text node could contribute only part of
/// its text.
pub(super) fn text_span(raw: &str) -> Range<usize> {
    0..raw.chars().count()
}

/// Whether white space in an element is kept: what the nearest element that says `xml:space` says, the element itself or an
/// ancestor, as XML inherits the attribute; `false` when none says it.
pub fn space_preserved(n: &NodeRef<'_>) -> bool {
    let mut cur = Some(*n);
    while let Some(e) = cur {
        if let Some(v) = e.attr(XML, "space") {
            return v == "preserve";
        }
        cur = e.parent();
    }
    false
}

/// True if XML would treat the first or last character as whitespace.
fn edge_space(raw: &str) -> bool {
    let ws = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    raw.chars().next().is_some_and(ws) || raw.chars().next_back().is_some_and(ws)
}

/// The namespace of Word 2016's extended symbols (`w16se:symEx`).
const W16SE: &str = "http://schemas.microsoft.com/office/word/2015/wordml/symex";

fn is_symex(c: &NodeRef<'_>) -> bool {
    c.namespace() == W16SE && c.local_name() == "symEx"
}

/// The character of a `w16se:symEx`, from the hexadecimal code point of its `w16se:char`, when it is one XML can carry.
fn symex_char_of(c: &NodeRef<'_>) -> Option<char> {
    let v = c.attr(W16SE, "char")?;
    let v = v.trim();
    let v = v
        .strip_prefix("0x")
        .or_else(|| v.strip_prefix("0X"))
        .unwrap_or(v);
    u32::from_str_radix(v, 16)
        .ok()
        .and_then(char::from_u32)
        .filter(|&ch| carriable(ch) && !ch.is_control())
}

/// For a branch that holds a `w16se:symEx` and nothing else, its character or `None` when it has none; `None` for any
/// other branch.
fn symex_char(b: &NodeRef<'_>) -> Option<Option<char>> {
    let mut kids = b.child_elements();
    let first = kids.next()?;
    (is_symex(&first) && kids.next().is_none()).then(|| symex_char_of(&first))
}

/// Whether XML can carry the character, so that a symbol's code point cannot be a control code.
fn carriable(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

impl<'t> Builder<'t> {
    /// A child of a paragraph or of a wrapper.
    pub(super) fn content_child(&mut self, c: &NodeRef<'t>, info: Info) {
        let id = c.id();
        if is_word_ns(c.namespace()) {
            let children = c.children();
            let wrap = |kind| (kind, FrameKind::Content);
            let wrapper = match c.local_name() {
                "pPr" => return,
                "r" => {
                    self.stack.push(Frame {
                        it: children,
                        info: Info {
                            run: Some(id),
                            ..info
                        },
                        kind: FrameKind::Run,
                    });
                    return;
                }
                "ins" => wrap(WrapperKind::Ins),
                "del" => wrap(WrapperKind::Del),
                "moveFrom" => wrap(WrapperKind::MoveFrom),
                "moveTo" => wrap(WrapperKind::MoveTo),
                "hyperlink" => wrap(WrapperKind::Hyperlink),
                "smartTag" => wrap(WrapperKind::SmartTag),
                "customXml" => wrap(WrapperKind::CustomXml),
                "dir" => wrap(WrapperKind::Dir),
                "bdo" => wrap(WrapperKind::Bdo),
                "sdt" => {
                    if let Some(content) = w_child(c, "sdtContent") {
                        self.push_wrapper(
                            info,
                            WrapperKind::Control,
                            c,
                            content.children(),
                            FrameKind::Content,
                        );
                    }
                    return;
                }
                "fldSimple" => return self.simple_field(c, info),
                "bookmarkStart" => return self.mark(ItemKind::BookmarkStart, id, info),
                "bookmarkEnd" => return self.mark(ItemKind::BookmarkEnd, id, info),
                "commentRangeStart" => return self.mark(ItemKind::CommentStart, id, info),
                "commentRangeEnd" => return self.mark(ItemKind::CommentEnd, id, info),
                "permStart" => return self.mark(ItemKind::PermStart, id, info),
                "permEnd" => return self.mark(ItemKind::PermEnd, id, info),
                "moveFromRangeStart" | "moveToRangeStart" => {
                    return self.mark(ItemKind::MoveRangeStart, id, info);
                }
                "moveFromRangeEnd" | "moveToRangeEnd" => {
                    return self.mark(ItemKind::MoveRangeEnd, id, info);
                }
                "p" | "tbl" => return self.anomaly(AnomalyKind::NestedBlock, id),
                name if SILENT.contains(&name) => return,
                _ => return self.anomaly(AnomalyKind::UnknownElement, id),
            };
            self.push_wrapper(info, wrapper.0, c, children, wrapper.1);
        } else if is_in(c, &M, "oMath") || is_in(c, &M, "oMathPara") {
            self.atom(ItemKind::Math, id, info);
        } else {
            self.foreign(c, info, FrameKind::Content);
        }
    }

    /// A child of a run.
    pub(super) fn run_child(&mut self, c: &NodeRef<'t>, info: Info) {
        let id = c.id();
        if !is_word_ns(c.namespace()) {
            self.foreign(c, info, FrameKind::Run);
            return;
        }
        match c.local_name() {
            "rPr" => {}
            "t" | "delText" => self.text_node(c, info),
            "instrText" | "delInstrText" => self.instruction(c, info),
            "fldChar" => self.field_char(c, info),
            "tab" | "ptab" => self.atom(ItemKind::Tab, id, info),
            "cr" => self.atom(ItemKind::LineBreak, id, info),
            "br" => {
                let kind = match w_attr(c, "type").as_deref() {
                    Some("page") => ItemKind::PageBreak,
                    Some("column") => ItemKind::ColumnBreak,
                    _ => ItemKind::LineBreak,
                };
                self.atom(kind, id, info);
            }
            "noBreakHyphen" => self.atom(ItemKind::Hyphen, id, info),
            "softHyphen" => self.atom(ItemKind::SoftHyphen, id, info),
            "sym" => self.symbol(c, info),
            "drawing" | "pict" | "object" => self.atom(ItemKind::Object, id, info),
            "footnoteReference" | "endnoteReference" => self.atom(ItemKind::NoteRef, id, info),
            "footnoteRef" | "endnoteRef" => self.atom(ItemKind::NoteMark, id, info),
            "pgNum" | "dayShort" | "dayLong" | "monthShort" | "monthLong" | "yearShort"
            | "yearLong" => self.atom(ItemKind::Auto, id, info),
            "commentReference" => self.mark(ItemKind::CommentRef, id, info),
            "ruby" => {
                if let Some(base) = w_child(c, "rubyBase") {
                    self.push_wrapper(
                        info,
                        WrapperKind::Ruby,
                        c,
                        base.children(),
                        FrameKind::Content,
                    );
                }
            }
            name if SILENT_IN_RUN.contains(&name) => {}
            _ => self.anomaly(AnomalyKind::UnknownElement, id),
        }
    }

    /// An element that is not in the Word namespace, where content or a run's content may stand: the
    /// branch Word reads of an `mc:AlternateContent`, the children of an element that
    /// `mc:ProcessContent` names, nothing for an ignorable element it does not, and an element the
    /// model does not know otherwise, which is skipped with its content and counted.
    fn foreign(&mut self, c: &NodeRef<'t>, info: Info, frame: FrameKind) {
        if is_symex(c) {
            return self.symex(c, info);
        }
        match mc::disposition(c) {
            mc::Mc::Branch(b) => {
                // A Choice of a symbol (`w16se:symEx`) whose character cannot be read gives way to the Fallback, which
                // holds the same character as text: never nothing.
                let b = match (symex_char(&b), b.local_name()) {
                    (Some(None), "Choice") => c
                        .child_elements()
                        .find(|x| x.local_name() == "Fallback" && mc::is_branch(x))
                        .unwrap_or(b),
                    _ => b,
                };
                self.push_wrapper(info, WrapperKind::Alternate, &b, b.children(), frame);
            }
            mc::Mc::NoBranch => self.anomaly(AnomalyKind::NoBranch, c.id()),
            mc::Mc::ProcessContent => self.stack.push(Frame {
                it: c.children(),
                info,
                kind: frame,
            }),
            mc::Mc::Skip => {}
            mc::Mc::Normal => self.anomaly(AnomalyKind::UnknownElement, c.id()),
        }
    }

    /// The text and CDATA children of a `w:t` or `w:delText`, each one item.
    fn text_node(&mut self, c: &NodeRef<'t>, info: Info) {
        let deleted_text = c.local_name() == "delText";
        if !deleted_text && info.in_del {
            self.anomaly(AnomalyKind::TextInDeletion, c.id());
        }
        if deleted_text && !info.in_del && info.vis.0 & super::text_model::Vis::FINAL != 0 {
            self.anomaly(AnomalyKind::DelTextOutsideDeletion, c.id());
        }
        // Whether the element keeps its white space matters only for a text with white space at an end,
        // so it is looked up (an attribute of it or of an ancestor) for the first such text only.
        let mut preserve: Option<bool> = None;
        for child in c.children() {
            let cdata = match child.kind() {
                NodeKind::Text => 0,
                NodeKind::CData => CDATA,
                _ => continue,
            };
            let raw = child.text();
            if raw.is_empty() {
                continue;
            }
            if edge_space(&raw) && !*preserve.get_or_insert_with(|| space_preserved(c)) {
                self.anomaly(AnomalyKind::EdgeSpaceWithoutPreserve, child.id());
            }
            let span = text_span(&raw);
            if span.is_empty() {
                // An empty text node: no character.
                continue;
            }
            let chars = raw.chars().count();
            let whole = span.start == 0 && span.end == chars;
            let counted: std::borrow::Cow<'_, str> = if whole {
                std::borrow::Cow::Borrowed(&*raw)
            } else {
                raw.chars()
                    .skip(span.start)
                    .take(span.len())
                    .collect::<String>()
                    .into()
            };
            let from = u32::try_from(span.start).unwrap_or(u32::MAX);
            let width = u32::try_from(span.len()).unwrap_or(u32::MAX);
            self.item_of_width(
                ItemKind::Text,
                child.id(),
                info,
                &counted,
                width,
                from,
                cdata,
            );
        }
    }

    /// A piece of field instruction.
    fn instruction(&mut self, c: &NodeRef<'t>, info: Info) {
        let piece = c.text_content();
        match self.state.add_instr(&piece) {
            Instr::Added(field) => {
                if let Some(r) = self.field_ref(field) {
                    r.instr.push_str(&piece);
                }
            }
            Instr::Ignored => {}
            Instr::Stray => self.anomaly(AnomalyKind::StrayFieldChar, c.id()),
        }
        self.mark(ItemKind::FieldCode, c.id(), info);
    }

    /// A `w:fldChar`: the begin, separate or end of a complex field.
    fn field_char(&mut self, c: &NodeRef<'t>, info: Info) {
        let id = c.id();
        match w_attr(c, "fldCharType").as_deref() {
            Some("begin") => match self.open_field(c, false, String::new()) {
                Some(_) => self.mark(ItemKind::FieldBegin, id, info),
                None => self.anomaly(AnomalyKind::FieldTooDeep, id),
            },
            Some("separate") => match self.state.separate() {
                Sep::Done(field) => {
                    if let Some(r) = self.field_ref(field) {
                        r.separate = Some(id);
                    }
                    self.mark(ItemKind::FieldSeparate, id, info);
                }
                Sep::Ignored => {}
                Sep::Stray => self.anomaly(AnomalyKind::StrayFieldChar, id),
            },
            Some("end") => self.field_end(c, info),
            _ => self.anomaly(AnomalyKind::UnknownElement, id),
        }
    }

    /// The end of a complex field. One that has no result, and is a type Word still shows, gets an
    /// atom for what it shows.
    fn field_end(&mut self, c: &NodeRef<'t>, info: Info) {
        let id = c.id();
        if self.state.is_ignoring() {
            self.state.end();
            return;
        }
        let Some(top) = self.state.stack.last() else {
            self.anomaly(AnomalyKind::StrayFieldChar, id);
            return;
        };
        if !top.separate && !top.simple && displayed(&top.instr) {
            self.state.show_value();
            self.atom(ItemKind::FieldValue, id, info);
        }
        self.mark(ItemKind::FieldEnd, id, info);
        self.pop_field(id);
    }

    /// A `w:fldSimple`: a wrapper whose content is the field's result.
    fn simple_field(&mut self, c: &NodeRef<'t>, info: Info) {
        let instr = w_attr(c, "instr")
            .map(|v| v.into_owned())
            .unwrap_or_default();
        let displayed = displayed(&instr);
        if self.open_field(c, true, instr).is_none() {
            self.anomaly(AnomalyKind::FieldTooDeep, c.id());
            self.push_wrapper(
                info,
                WrapperKind::FieldSimple,
                c,
                c.children(),
                FrameKind::Content,
            );
            return;
        }
        let inner = self.wrap(info, WrapperKind::FieldSimple, c.id());
        self.mark(ItemKind::FieldBegin, c.id(), inner);
        let items_before = self.items.len();
        self.stack.push(Frame {
            it: c.children(),
            info: inner,
            kind: FrameKind::Simple {
                node: c.id(),
                items_before,
                displayed,
            },
        });
    }

    /// A `w16se:symEx` (Word 2016's symbol, an emoji in an `mc:Choice`): the character its `w16se:char` encodes.
    fn symex(&mut self, c: &NodeRef<'t>, info: Info) {
        let ch = symex_char_of(c).unwrap_or_else(|| {
            self.anomaly(AnomalyKind::InvalidSymbol, c.id());
            '\u{fffd}'
        });
        let mut buf = [0u8; 4];
        let text = ch.encode_utf8(&mut buf);
        self.item(ItemKind::Symbol, c.id(), info, text, 0, 0);
    }

    /// A `w:sym`: the code point of its `w:char`.
    fn symbol(&mut self, c: &NodeRef<'t>, info: Info) {
        let code = w_attr(c, "char").and_then(|v| {
            let v = v.trim();
            let v = v
                .strip_prefix("0x")
                .or_else(|| v.strip_prefix("0X"))
                .unwrap_or(v);
            u32::from_str_radix(v, 16).ok()
        });
        let ch = code.and_then(char::from_u32).filter(|&ch| carriable(ch));
        let ch = ch.unwrap_or_else(|| {
            self.anomaly(AnomalyKind::InvalidSymbol, c.id());
            '\u{fffd}'
        });
        let mut buf = [0u8; 4];
        let text = ch.encode_utf8(&mut buf);
        self.item(ItemKind::Symbol, c.id(), info, text, 0, 0);
    }
}
