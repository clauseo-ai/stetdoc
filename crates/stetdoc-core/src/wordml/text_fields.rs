//! Fields: the machine the text builder runs, and the story's field index.
//!
//! Field code is not text, and a field's result is flagged so a caller can decide what to do with it.
//! A complex field is `w:fldChar` `begin`, code, `separate`, result, `end`; a simple field is a
//! `w:fldSimple` around its result. Fields nest and cross paragraphs, so the state at the start of a
//! paragraph comes from the paragraphs before it: the field pass (here) walks the story in order,
//! building each paragraph's map to learn the state it ends with, and keeps the sparse result in
//! the story. `docs/design/wordml.md` 4.4.

use super::anomaly::{Anomaly, AnomalyKind};
use super::names::{is_word_ns, on_off, w_attr};
use super::story::{BlockKind, NO_UNIT, Story};
use super::text_build;
use super::text_model::{FieldRef, NO_FIELD};
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::collections::{HashMap, HashSet};

/// Nesting deeper than this is ignored with an anomaly.
pub(super) const MAX_DEPTH: usize = 32;

/// The field types Word still shows when they have no stored result (4.4). `MACROBUTTON` and
/// `GOTOBUTTON` show text that is in their code. The rest of the fields with no result (`IF`,
/// `REF`, `XE`, `TA` and others) show nothing.
const DISPLAYED_WITHOUT_RESULT: [&str; 8] = [
    "EQ",
    "MACROBUTTON",
    "LISTNUM",
    "SYMBOL",
    "PAGE",
    "GOTOBUTTON",
    "QUOTE",
    "BARCODE",
];

/// The field type of an instruction: its first word in upper case.
pub(super) fn keyword(instr: &str) -> String {
    instr
        .split_whitespace()
        .next()
        .map(|w| w.trim_matches('"').to_ascii_uppercase())
        .unwrap_or_default()
}

/// True if Word shows something for a field of this type that has no stored result.
pub(super) fn shows_without_result(keyword: &str) -> bool {
    DISPLAYED_WITHOUT_RESULT.contains(&keyword)
}

/// Where a field is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    /// Between `begin` and `separate`: the instruction.
    Code,
    /// After `separate`, and a `w:fldSimple`: the result.
    Result,
}

/// A field that is open: begun and not ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OpenField {
    pub id: u32,
    pub phase: Phase,
    pub begin: NodeId,
    /// The instruction so far. It is kept only while the field is in its code phase, which cannot
    /// cross a paragraph, and is emptied when the field gets its result, so what a paragraph carries to
    /// the next stays small however long the instruction was. The whole instruction is in
    /// [`FieldRef`] and in the story's field list, once.
    pub instr: String,
    /// True once a `separate` was seen.
    pub separate: bool,
    pub simple: bool,
}

/// What a `separate` did.
pub(super) enum Sep {
    Done(u32),
    /// It belongs to a field that was ignored for its depth.
    Ignored,
    /// Nothing open, or the innermost field already has a result.
    Stray,
}

/// What a piece of instruction text did.
pub(super) enum Instr {
    /// It joined the instruction of this field.
    Added(u32),
    /// It belongs to a field that was ignored for its depth.
    Ignored,
    /// No field is open, or the innermost field is past its code.
    Stray,
}

/// What an `end` did.
pub(super) enum End {
    Closed(OpenField),
    Ignored,
    Stray,
}

/// The stack of open fields while a paragraph is read.
#[derive(Default)]
pub(super) struct FieldState {
    pub stack: Vec<OpenField>,
    ignored: u32,
    pub next_id: u32,
    pub began: u32,
    code: u32,
}

impl FieldState {
    /// The state at the start of a paragraph: the fields carried in, and the id the next one gets.
    pub fn new(carry: &[OpenField], base_id: u32) -> FieldState {
        let code = carry.iter().filter(|f| f.phase == Phase::Code).count();
        FieldState {
            stack: carry.to_vec(),
            ignored: 0,
            next_id: base_id,
            began: 0,
            code: u32::try_from(code).unwrap_or(u32::MAX),
        }
    }

    /// True if any open field is in its code phase: everything now is field code.
    pub fn in_code(&self) -> bool {
        self.code > 0
    }

    /// True if the next `end` or `separate` belongs to a field that was ignored for its depth.
    pub fn is_ignoring(&self) -> bool {
        self.ignored > 0
    }

    /// True if some field is open (and none is in its code phase, or the caller would not ask).
    pub fn in_field(&self) -> bool {
        !self.stack.is_empty()
    }

    /// The id of the innermost open field.
    pub fn top_id(&self) -> u32 {
        self.stack.last().map_or(NO_FIELD, |f| f.id)
    }

    /// Opens a field. `None` if it is too deep, in which case the field is ignored.
    pub fn begin(&mut self, begin: NodeId, simple: bool, instr: String) -> Option<u32> {
        if self.stack.len() >= MAX_DEPTH {
            // a complex field is ignored with its separate and its end, which are counted. A simple
            // field ends with its element, so there is nothing to balance.
            if !simple {
                self.ignored += 1;
            }
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.began += 1;
        let phase = if simple { Phase::Result } else { Phase::Code };
        if phase == Phase::Code {
            self.code += 1;
        }
        self.stack.push(OpenField {
            id,
            phase,
            begin,
            instr: if simple { String::new() } else { instr },
            separate: simple,
            simple,
        });
        Some(id)
    }

    /// Moves the innermost field from code to result.
    pub fn separate(&mut self) -> Sep {
        if self.ignored > 0 {
            return Sep::Ignored;
        }
        match self.stack.last_mut() {
            Some(f) if f.phase == Phase::Code => {
                f.phase = Phase::Result;
                f.separate = true;
                f.instr = String::new();
                self.code = self.code.saturating_sub(1);
                Sep::Done(f.id)
            }
            _ => Sep::Stray,
        }
    }

    /// Lets the innermost field show a value of its own, as if it had a result.
    pub fn show_value(&mut self) {
        if let Some(f) = self.stack.last_mut()
            && f.phase == Phase::Code
        {
            f.phase = Phase::Result;
            f.instr = String::new();
            self.code = self.code.saturating_sub(1);
        }
    }

    /// Appends a piece of instruction to the innermost field, if it is in its code phase. An
    /// instruction in a field's result, or with no field open, is stray, and one in a field that was
    /// ignored for its depth is dropped with it.
    pub fn add_instr(&mut self, piece: &str) -> Instr {
        if self.ignored > 0 {
            return Instr::Ignored;
        }
        match self.stack.last_mut() {
            Some(f) if f.phase == Phase::Code => {
                f.instr.push_str(piece);
                Instr::Added(f.id)
            }
            _ => Instr::Stray,
        }
    }

    /// Closes the innermost field.
    pub fn end(&mut self) -> End {
        if self.ignored > 0 {
            self.ignored -= 1;
            return End::Ignored;
        }
        match self.stack.pop() {
            Some(f) => {
                if f.phase == Phase::Code {
                    self.code = self.code.saturating_sub(1);
                }
                End::Closed(f)
            }
            None => End::Stray,
        }
    }
}

/// A field of the story, assembled from the paragraphs that hold its parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    /// The story-wide id: fields are numbered in document order of their start. It is an internal
    /// handle, renumbered when a field is added or removed, and is not stable across edits. Name a
    /// field across edits by [`FieldInfo::begin`].
    pub id: u32,
    /// The `w:fldChar` `begin`, or the `w:fldSimple`.
    pub begin: NodeId,
    /// The `w:fldChar` `separate`, if there is one.
    pub separate: Option<NodeId>,
    /// The `w:fldChar` `end`, if the field was closed.
    pub end: Option<NodeId>,
    /// The field type, such as `REF` or `HYPERLINK`, in upper case.
    pub keyword: Box<str>,
    /// The whole instruction, assembled from its pieces.
    pub instr: String,
    /// True for a `w:fldSimple`.
    pub simple: bool,
    /// The paragraph that holds the start.
    pub paragraph_begin: NodeId,
    /// The paragraph that holds the end, if the field was closed.
    pub paragraph_end: Option<NodeId>,
    /// The id of the field this one is nested in.
    pub parent: Option<u32>,
    /// `w:dirty`: Word is asked to refresh the result.
    pub dirty: bool,
    /// `w:fldLock`: the result is not to be refreshed.
    pub locked: bool,
}

impl FieldInfo {
    /// The text Word shows for a `MACROBUTTON` or `GOTOBUTTON`, which is in the instruction after the
    /// macro name. `None` for other fields.
    pub fn display_text(&self) -> Option<String> {
        if !matches!(&*self.keyword, "MACROBUTTON" | "GOTOBUTTON") {
            return None;
        }
        let rest = self.instr.trim_start();
        let rest = rest.split_once(char::is_whitespace)?.1.trim_start();
        let shown = rest.split_once(char::is_whitespace)?.1.trim();
        (!shown.is_empty()).then(|| shown.to_string())
    }
}

/// What a paragraph starts with when it is in or begins a field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ParaState {
    /// The id its first new field gets.
    pub base: u32,
    /// How many fields start in it.
    pub began: u32,
    /// The fields open when it starts.
    pub start: Vec<OpenField>,
    /// The fields open when it ends.
    pub end: Vec<OpenField>,
}

/// What a rebuild of the field index may take from the index of the story before the edit.
pub(super) struct Reuse<'a> {
    /// The index of the previous story.
    pub old: &'a FieldIndex,
    /// The paragraphs that must be looked at again: touched, new and moved.
    pub dirty: &'a HashSet<NodeId>,
}

/// The fields of a story, and the state each paragraph that needs one starts with.
///
/// The ids of fields are story-wide ordinals in document order of their start. They are internal
/// handles: adding or removing a field renumbers every later one, so nothing outside `wordml` may
/// keep one across an edit, and no view or ref exposes one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldIndex {
    pub(super) fields: Vec<FieldInfo>,
    pub(super) paras: HashMap<NodeId, ParaState>,
    /// The paragraphs that hold any field markup (`w:fldChar`, `w:fldSimple`, `w:instrText`). Only
    /// these can change the state of a field, so a rebuild looks at them and at what was edited, and
    /// not at the whole story.
    pub(super) marked: HashSet<NodeId>,
    /// Fields still open where their note, comment or story ends.
    pub(super) anomalies: Vec<Anomaly>,
}

/// True if anything below `n` is a field character, a simple field or field instruction, which are
/// the only things that begin, change or end a field. One walk in document order, by the links of the
/// tree, with no allocation: it is asked of every paragraph of a story.
pub(super) fn has_field_markup(n: &NodeRef<'_>) -> bool {
    let mut cur = n.first_child();
    while let Some(c) = cur {
        if c.is_element() {
            if matches!(
                c.local_name(),
                "fldChar" | "fldSimple" | "instrText" | "delInstrText"
            ) && is_word_ns(c.namespace())
            {
                return true;
            }
            if let Some(first) = c.first_child() {
                cur = Some(first);
                continue;
            }
        }
        // The next node: a sibling, or the sibling of the nearest ancestor below `n` that has one.
        let mut up = c;
        cur = loop {
            if let Some(next) = up.next_sibling() {
                break Some(next);
            }
            match up.parent() {
                Some(p) if p.id() != n.id() => up = p,
                _ => break None,
            }
        };
    }
    false
}

impl FieldIndex {
    /// The field pass: every paragraph of the story in order, each built with the state the one
    /// before ended with. A note or a comment starts afresh.
    ///
    /// Only a paragraph with field markup can change what is open, so only those are built into a
    /// text map. A paragraph with none that sits inside an open field takes the state as it is, and
    /// one outside every field is left out. With `reuse`, the paragraphs that were not marked and
    /// were not edited are taken to have no markup without being looked at, which is the contract
    /// of `touched` (`docs/design/wordml.md` 4.4).
    pub(super) fn build(story: &Story, tree: &SpanTree, reuse: Option<Reuse<'_>>) -> FieldIndex {
        let mut index = FieldIndex::default();
        let mut carry: Vec<OpenField> = Vec::new();
        let mut unit = NO_UNIT;
        let mut next_id = 0u32;
        let mut first = true;
        for e in story
            .entries
            .iter()
            .filter(|e| e.kind == BlockKind::Paragraph)
        {
            if first || e.unit != unit {
                index.unterminated(&carry);
                carry.clear();
                unit = e.unit;
                first = false;
            }
            let p = tree.node_ref(e.node);
            let look = reuse
                .as_ref()
                .is_none_or(|r| r.old.marked.contains(&e.node) || r.dirty.contains(&e.node));
            if !(look && has_field_markup(&p)) {
                if !carry.is_empty() {
                    index.paras.insert(
                        e.node,
                        ParaState {
                            base: next_id,
                            began: 0,
                            start: carry.clone(),
                            end: carry.clone(),
                        },
                    );
                }
                continue;
            }
            index.marked.insert(e.node);
            let map = text_build::build(&p, &carry, next_id);
            if !carry.is_empty() || map.began > 0 {
                index.paras.insert(
                    e.node,
                    ParaState {
                        base: next_id,
                        began: map.began,
                        start: carry.clone(),
                        end: map.open_at_end.clone(),
                    },
                );
            }
            index.merge(tree, e.node, &map.fields);
            next_id += map.began;
            carry = map.open_at_end;
        }
        index.unterminated(&carry);
        for f in &mut index.fields {
            f.keyword = keyword(&f.instr).into_boxed_str();
        }
        index
    }

    /// Notes the fields still open where a unit ends.
    fn unterminated(&mut self, open: &[OpenField]) {
        self.anomalies.extend(open.iter().map(|f| Anomaly {
            kind: AnomalyKind::UnterminatedField,
            node: f.begin,
        }));
    }

    /// Adds what a paragraph saw of its fields.
    fn merge(&mut self, tree: &SpanTree, paragraph: NodeId, seen: &[FieldRef]) {
        for r in seen {
            if r.begins_here {
                let begin = tree.node_ref(r.begin);
                self.fields.push(FieldInfo {
                    id: r.id,
                    begin: r.begin,
                    separate: r.separate,
                    end: r.end,
                    keyword: Box::default(),
                    instr: r.instr.clone(),
                    simple: r.simple,
                    paragraph_begin: paragraph,
                    paragraph_end: r.end.map(|_| paragraph),
                    parent: r.parent,
                    dirty: w_attr(&begin, "dirty").is_some_and(|v| on_off(Some(&v))),
                    locked: w_attr(&begin, "fldLock").is_some_and(|v| on_off(Some(&v))),
                });
            } else if let Some(f) = self.fields.get_mut(r.id as usize) {
                f.instr.push_str(&r.instr);
                if r.separate.is_some() {
                    f.separate = r.separate;
                }
                if r.end.is_some() {
                    f.end = r.end;
                    f.paragraph_end = Some(paragraph);
                }
            }
        }
    }

    /// The fields in id order.
    pub fn fields(&self) -> &[FieldInfo] {
        &self.fields
    }
}
