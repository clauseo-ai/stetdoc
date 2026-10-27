//! The JSON form of the neutral model: what `json/1` writes and reads.
//!
//! Every type is written as serde's default representation would write it (see `json_value.rs`), and
//! its field names are the model's. A change to a field is additive or a new version of the format.

use super::json_value::{
    Fields, FromJson, Json, JsonError, ToJson, object, tagged, unknown_variant, variant,
};
use crate::api::view::{Mode, Position, Warning, WarningCode};
use crate::projection::model::{
    Atom, Author, AutoKind, Block, Body, Cell, CellChange, CellRev, ChangeKind, ChangeList,
    ControlInfo, ControlState, Field, Head, HiddenReason, HitList, Inline, Label, LinkTarget,
    LooksHiddenSignal, Margin, Mark, MarkKind, MarkRev, NoteKind, ObjectInfo, ObjectKind,
    OpaqueBlock, Outline, Output, ParaFlags, Paragraph, Projection, PropRev, ReadOnly, RevKind,
    RevTag, Row, RowChange, RowRev, SectionLine, SectionUse, Slot, Story, Summary, Table,
    ThreadList, Unit, UnitKind, UnitMeta,
};
use crate::refs::{BaseToken, BlockRef};
use crate::wordml::{ControlKind, SectionBreak, StoryKind, VMerge};

macro_rules! json_struct {
    ($ty:ident { $($field:ident => $name:literal),+ $(,)? }) => {
        impl ToJson for $ty {
            fn to_json(&self) -> Json {
                Json::Obj(vec![$(($name.to_string(), self.$field.to_json())),+])
            }
        }

        impl FromJson for $ty {
            fn from_json(value: &Json) -> Result<$ty, JsonError> {
                let mut fields = Fields::of(value)?;
                let out = $ty { $($field: fields.take($name)?),+ };
                fields.done()?;
                Ok(out)
            }
        }
    };
}

macro_rules! json_names {
    ($ty:ident, $what:literal, { $($variant:ident => $name:literal),+ $(,)? }) => {
        impl ToJson for $ty {
            fn to_json(&self) -> Json {
                Json::Str(
                    match self {
                        $($ty::$variant => $name),+
                    }
                    .to_string(),
                )
            }
        }

        impl FromJson for $ty {
            fn from_json(value: &Json) -> Result<$ty, JsonError> {
                match value {
                    Json::Str(name) => match name.as_str() {
                        $($name => Ok($ty::$variant),)+
                        other => Err(unknown_variant($what, other)),
                    },
                    _ => Err(JsonError::shape(concat!("expected a ", $what))),
                }
            }
        }
    };
}

pub(super) use {json_names, json_struct};

json_names!(Mode, "mode", { Final => "final", Original => "original", Redline => "redline" });
json_names!(StoryKind, "story kind", {
    Body => "body", Header => "header", Footer => "footer", Footnotes => "footnotes",
    Endnotes => "endnotes", Comments => "comments"
});
json_names!(VMerge, "vertical merge", { None => "none", Restart => "restart", Continue => "continue" });
json_names!(SectionBreak, "section break", {
    NextPage => "next_page", Continuous => "continuous", EvenPage => "even_page",
    OddPage => "odd_page", NextColumn => "next_column"
});
json_names!(ControlKind, "control kind", {
    PlainText => "plain_text", RichText => "rich_text", Date => "date", DropDown => "drop_down",
    ComboBox => "combo_box", Checkbox => "checkbox", Picture => "picture", Citation => "citation",
    Bibliography => "bibliography", BuildingBlock => "building_block", Group => "group",
    RepeatingSection => "repeating_section", Equation => "equation", Unknown => "unknown"
});
json_names!(Slot, "slot", { Default => "default", First => "first", Even => "even" });
json_names!(UnitKind, "unit kind", {
    Body => "body", Header => "header", Footer => "footer", Footnote => "footnote",
    Endnote => "endnote", Comment => "comment"
});
json_names!(MarkKind, "mark kind", {
    Inserted => "inserted", Deleted => "deleted", MovedTo => "moved_to", MovedFrom => "moved_from"
});
json_names!(ChangeKind, "change kind", {
    Run => "run", Paragraph => "paragraph", Section => "section", Table => "table", Grid => "grid",
    Row => "row", Cell => "cell", Numbering => "numbering"
});
json_names!(ReadOnly, "read-only reason", {
    DeletedBlock => "deleted_block", LockedControl => "locked_control", DataBound => "data_bound",
    Placeholder => "placeholder"
});
json_names!(RowChange, "row change", { Inserted => "inserted", Deleted => "deleted" });
json_names!(CellChange, "cell change", { Inserted => "inserted", Deleted => "deleted", Merged => "merged" });
json_names!(RevKind, "revision kind", { Ins => "ins", Del => "del", MoveTo => "move_to", MoveFrom => "move_from" });
json_names!(ControlState, "control state", {
    Editable => "editable", Locked => "locked", DataBound => "data_bound", Placeholder => "placeholder"
});
json_names!(HiddenReason, "hidden reason", { Vanish => "vanish" });
json_names!(LooksHiddenSignal, "looks-hidden signal", { White => "white", Tiny => "tiny" });
json_names!(NoteKind, "note kind", { Footnote => "footnote", Endnote => "endnote" });
json_names!(AutoKind, "auto kind", { Page => "page", Date => "date", Value => "value" });
json_names!(ObjectKind, "object kind", {
    Image => "image", Shape => "shape", Chart => "chart", Object => "object", TextBox => "text_box"
});
// A warning code is its wire name, which `WarningCode` owns, so that a code added in `api` is one added here.
impl ToJson for WarningCode {
    fn to_json(&self) -> Json {
        Json::Str(self.as_str().to_string())
    }
}

impl FromJson for WarningCode {
    fn from_json(value: &Json) -> Result<WarningCode, JsonError> {
        match value {
            Json::Str(name) => {
                WarningCode::from_name(name).ok_or_else(|| unknown_variant("warning code", name))
            }
            _ => Err(JsonError::shape("expected a warning code")),
        }
    }
}

impl ToJson for BlockRef {
    fn to_json(&self) -> Json {
        Json::Str(self.to_string())
    }
}

impl FromJson for BlockRef {
    fn from_json(value: &Json) -> Result<BlockRef, JsonError> {
        match value {
            Json::Str(text) => {
                BlockRef::parse(text).map_err(|e| JsonError::shape(e.message().to_string()))
            }
            _ => Err(JsonError::shape("expected a ref")),
        }
    }
}

impl ToJson for BaseToken {
    fn to_json(&self) -> Json {
        Json::Str(self.to_string())
    }
}

impl FromJson for BaseToken {
    fn from_json(value: &Json) -> Result<BaseToken, JsonError> {
        match value {
            Json::Str(text) => {
                BaseToken::parse(text).map_err(|e| JsonError::shape(e.message().to_string()))
            }
            _ => Err(JsonError::shape("expected a base token")),
        }
    }
}

json_struct!(Position {
    scope => "scope", first => "first", last => "last", shown => "shown", total => "total",
    remaining_tokens => "remaining_tokens"
});
json_struct!(Warning {
    code => "code", block => "block", story => "story", message => "message", next => "next"
});
json_struct!(Projection { head => "head", body => "body", appendix => "appendix" });
json_struct!(Head {
    base => "base", mode => "mode", output => "output", common_style => "common_style",
    authors => "authors", summary => "summary", position => "position", warnings => "warnings"
});
json_struct!(Author { code => "code", name => "name" });
json_struct!(Summary {
    changes => "changes", comments => "comments", resolved => "resolved", footnotes => "footnotes",
    endnotes => "endnotes", sections => "sections", readonly => "readonly"
});
json_struct!(Story { kind => "kind", part => "part", uses => "uses", units => "units" });
json_struct!(SectionUse { section => "section", slot => "slot" });
// A unit's `mark` is written only when a note has one, so that every other unit reads as it did.
impl ToJson for Unit {
    fn to_json(&self) -> Json {
        let mut fields = vec![
            ("kind".to_string(), self.kind.to_json()),
            ("id".to_string(), self.id.to_json()),
            ("ordinal".to_string(), self.ordinal.to_json()),
        ];
        if let Some(mark) = self.mark {
            fields.push(("mark".to_string(), mark.to_json()));
        }
        fields.push(("meta".to_string(), self.meta.to_json()));
        fields.push(("blocks".to_string(), self.blocks.to_json()));
        Json::Obj(fields)
    }
}

impl FromJson for Unit {
    fn from_json(value: &Json) -> Result<Unit, JsonError> {
        let mut fields = Fields::of(value)?;
        let out = Unit {
            kind: fields.take("kind")?,
            id: fields.take("id")?,
            ordinal: fields.take("ordinal")?,
            mark: fields.take("mark")?,
            meta: fields.take("meta")?,
            blocks: fields.take("blocks")?,
        };
        fields.done()?;
        Ok(out)
    }
}
json_struct!(UnitMeta {
    author => "author", initials => "initials", date => "date", parent => "parent",
    resolved => "resolved", on => "on", anchored => "anchored"
});
json_struct!(Paragraph {
    r#ref => "ref", style => "style", label => "label", mark => "mark", revs => "revs",
    flags => "flags", inlines => "inlines", final_text => "final_text"
});
json_struct!(Label { text => "text", bullet => "bullet", ghost => "ghost", hidden => "hidden" });
json_struct!(MarkRev { kind => "kind", tag => "tag" });
json_struct!(PropRev { kind => "kind", tag => "tag" });
json_struct!(ParaFlags { hidden => "hidden", joins_next => "joins_next", readonly => "readonly" });
json_struct!(Table { r#ref => "ref", style => "style", cols => "cols", revs => "revs", rows => "rows" });
json_struct!(Row {
    r#ref => "ref", change => "change", header => "header", revs => "revs", cells => "cells"
});
json_struct!(RowRev { kind => "kind", tag => "tag" });
json_struct!(Cell {
    col => "col", span => "span", v_merge => "v_merge", change => "change", blocks => "blocks"
});
json_struct!(CellRev { kind => "kind", tag => "tag" });
json_struct!(SectionLine {
    index => "index", break_type => "break_type", title_pg => "title_pg", mark => "mark", revs => "revs"
});
json_struct!(ControlInfo { kind => "kind", alias => "alias", tag => "tag" });
json_struct!(ObjectInfo { kind => "kind", alt => "alt", text_box => "text_box" });
json_struct!(Field {
    keyword => "keyword", text => "text", width => "width", target => "target", opens => "opens",
    closes => "closes", dirty => "dirty"
});
json_struct!(RevTag { id => "id", author => "author", date => "date", peer => "peer" });

impl ToJson for Output {
    fn to_json(&self) -> Json {
        match self {
            Output::Files => Json::Str("files".to_string()),
            Output::View { name } => tagged("view", object(vec![("name", name.to_json())])),
        }
    }
}

impl FromJson for Output {
    fn from_json(value: &Json) -> Result<Output, JsonError> {
        match variant(value)? {
            ("files", None) => Ok(Output::Files),
            ("view", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let name = fields.take("name")?;
                fields.done()?;
                Ok(Output::View { name })
            }
            (other, _) => Err(unknown_variant("output", other)),
        }
    }
}

impl ToJson for Body {
    fn to_json(&self) -> Json {
        match self {
            Body::Stories(stories) => tagged("stories", stories.to_json()),
            Body::Outline(outline) => tagged("outline", outline.to_json()),
            Body::Lookup(lookup) => tagged("lookup", lookup.to_json()),
            Body::Threads(threads) => tagged("threads", threads.to_json()),
            Body::Ledger(changes) => tagged("ledger", changes.to_json()),
            Body::Hits(hits) => tagged("hits", hits.to_json()),
        }
    }
}

impl FromJson for Body {
    fn from_json(value: &Json) -> Result<Body, JsonError> {
        match variant(value)? {
            ("stories", Some(payload)) => Vec::from_json(payload).map(Body::Stories),
            ("outline", Some(payload)) => Outline::from_json(payload).map(Body::Outline),
            ("lookup", Some(payload)) => {
                crate::api::lookup::Lookup::from_json(payload).map(Body::Lookup)
            }
            ("threads", Some(payload)) => ThreadList::from_json(payload).map(Body::Threads),
            ("ledger", Some(payload)) => ChangeList::from_json(payload).map(Body::Ledger),
            ("hits", Some(payload)) => HitList::from_json(payload).map(Body::Hits),
            (other, _) => Err(unknown_variant("body", other)),
        }
    }
}

impl ToJson for Block {
    fn to_json(&self) -> Json {
        match self {
            Block::Paragraph(p) => tagged("paragraph", p.to_json()),
            Block::Table(t) => tagged("table", t.to_json()),
            Block::Section(s) => tagged("section", s.to_json()),
            Block::Opaque(o) => tagged("opaque", o.to_json()),
            Block::Margin(m) => tagged("margin", m.to_json()),
        }
    }
}

impl FromJson for Block {
    fn from_json(value: &Json) -> Result<Block, JsonError> {
        match variant(value)? {
            ("paragraph", Some(p)) => Paragraph::from_json(p).map(Block::Paragraph),
            ("table", Some(t)) => Table::from_json(t).map(Block::Table),
            ("section", Some(s)) => SectionLine::from_json(s).map(Block::Section),
            ("opaque", Some(o)) => OpaqueBlock::from_json(o).map(Block::Opaque),
            ("margin", Some(m)) => Margin::from_json(m).map(Block::Margin),
            (other, _) => Err(unknown_variant("block", other)),
        }
    }
}

impl ToJson for OpaqueBlock {
    fn to_json(&self) -> Json {
        match self {
            OpaqueBlock::AltChunk => Json::Str("alt_chunk".to_string()),
            OpaqueBlock::Unknown => Json::Str("unknown".to_string()),
            OpaqueBlock::Math(text) => tagged("math", text.to_json()),
            OpaqueBlock::Collapsed {
                keyword,
                first,
                last,
                blocks,
            } => tagged(
                "collapsed",
                object(vec![
                    ("keyword", keyword.to_json()),
                    ("first", first.to_json()),
                    ("last", last.to_json()),
                    ("blocks", blocks.to_json()),
                ]),
            ),
        }
    }
}

impl FromJson for OpaqueBlock {
    fn from_json(value: &Json) -> Result<OpaqueBlock, JsonError> {
        match variant(value)? {
            ("alt_chunk", None) => Ok(OpaqueBlock::AltChunk),
            ("unknown", None) => Ok(OpaqueBlock::Unknown),
            ("math", Some(text)) => Box::from_json(text).map(OpaqueBlock::Math),
            ("collapsed", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let out = OpaqueBlock::Collapsed {
                    keyword: fields.take("keyword")?,
                    first: fields.take("first")?,
                    last: fields.take("last")?,
                    blocks: fields.take("blocks")?,
                };
                fields.done()?;
                Ok(out)
            }
            (other, _) => Err(unknown_variant("opaque block", other)),
        }
    }
}

impl ToJson for LinkTarget {
    fn to_json(&self) -> Json {
        match self {
            LinkTarget::External(url) => tagged("external", url.to_json()),
        }
    }
}

impl FromJson for LinkTarget {
    fn from_json(value: &Json) -> Result<LinkTarget, JsonError> {
        match variant(value)? {
            ("external", Some(url)) => Box::from_json(url).map(LinkTarget::External),
            (other, _) => Err(unknown_variant("link target", other)),
        }
    }
}

impl ToJson for Mark {
    fn to_json(&self) -> Json {
        match self {
            Mark::CommentStart(id) => tagged("comment_start", id.to_json()),
            Mark::CommentEnd(id) => tagged("comment_end", id.to_json()),
        }
    }
}

impl FromJson for Mark {
    fn from_json(value: &Json) -> Result<Mark, JsonError> {
        match variant(value)? {
            ("comment_start", Some(id)) => i64::from_json(id).map(Mark::CommentStart),
            ("comment_end", Some(id)) => i64::from_json(id).map(Mark::CommentEnd),
            (other, _) => Err(unknown_variant("mark", other)),
        }
    }
}

impl ToJson for Atom {
    fn to_json(&self) -> Json {
        match self {
            Atom::PageBreak => Json::Str("page_break".to_string()),
            Atom::ColumnBreak => Json::Str("column_break".to_string()),
            Atom::NoteRef { kind, ordinal } => tagged(
                "note_ref",
                object(vec![
                    ("kind", kind.to_json()),
                    ("ordinal", ordinal.to_json()),
                ]),
            ),
            Atom::NoteMark { kind } => tagged("note_mark", object(vec![("kind", kind.to_json())])),
            Atom::Object(info) => tagged("object", info.to_json()),
            Atom::Math(text) => tagged("math", text.to_json()),
            Atom::Auto(kind) => tagged("auto", kind.to_json()),
        }
    }
}

impl FromJson for Atom {
    fn from_json(value: &Json) -> Result<Atom, JsonError> {
        match variant(value)? {
            ("page_break", None) => Ok(Atom::PageBreak),
            ("column_break", None) => Ok(Atom::ColumnBreak),
            ("note_ref", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let out = Atom::NoteRef {
                    kind: fields.take("kind")?,
                    ordinal: fields.take("ordinal")?,
                };
                fields.done()?;
                Ok(out)
            }
            ("note_mark", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let out = Atom::NoteMark {
                    kind: fields.take("kind")?,
                };
                fields.done()?;
                Ok(out)
            }
            ("object", Some(info)) => ObjectInfo::from_json(info).map(Atom::Object),
            ("math", Some(text)) => Box::from_json(text).map(Atom::Math),
            ("auto", Some(kind)) => AutoKind::from_json(kind).map(Atom::Auto),
            (other, _) => Err(unknown_variant("atom", other)),
        }
    }
}

impl ToJson for Inline {
    fn to_json(&self) -> Json {
        match self {
            Inline::Text(text) => tagged("text", text.to_json()),
            Inline::Atom(atom) => tagged("atom", atom.to_json()),
            Inline::Rev { kind, tag, inlines } => tagged(
                "rev",
                object(vec![
                    ("kind", kind.to_json()),
                    ("tag", tag.to_json()),
                    ("inlines", inlines.to_json()),
                ]),
            ),
            Inline::Link { target, inlines } => tagged(
                "link",
                object(vec![
                    ("target", target.to_json()),
                    ("inlines", inlines.to_json()),
                ]),
            ),
            Inline::Control {
                info,
                state,
                inlines,
            } => tagged(
                "control",
                object(vec![
                    ("info", info.to_json()),
                    ("state", state.to_json()),
                    ("inlines", inlines.to_json()),
                ]),
            ),
            Inline::Field(field) => tagged("field", field.to_json()),
            Inline::Hidden { reason, inlines } => tagged(
                "hidden",
                object(vec![
                    ("reason", reason.to_json()),
                    ("inlines", inlines.to_json()),
                ]),
            ),
            Inline::LooksHidden { signal, inlines } => tagged(
                "looks_hidden",
                object(vec![
                    ("signal", signal.to_json()),
                    ("inlines", inlines.to_json()),
                ]),
            ),
            Inline::Mark(mark) => tagged("mark", mark.to_json()),
        }
    }
}

impl FromJson for Inline {
    fn from_json(value: &Json) -> Result<Inline, JsonError> {
        let (name, payload) = variant(value)?;
        let Some(payload) = payload else {
            return Err(unknown_variant("inline", name));
        };
        match name {
            "text" => String::from_json(payload).map(Inline::Text),
            "atom" => Atom::from_json(payload).map(Inline::Atom),
            "field" => Field::from_json(payload).map(Inline::Field),
            "mark" => Mark::from_json(payload).map(Inline::Mark),
            "rev" => {
                let mut fields = Fields::of(payload)?;
                let out = Inline::Rev {
                    kind: fields.take("kind")?,
                    tag: fields.take("tag")?,
                    inlines: fields.take("inlines")?,
                };
                fields.done()?;
                Ok(out)
            }
            "link" => {
                let mut fields = Fields::of(payload)?;
                let out = Inline::Link {
                    target: fields.take("target")?,
                    inlines: fields.take("inlines")?,
                };
                fields.done()?;
                Ok(out)
            }
            "control" => {
                let mut fields = Fields::of(payload)?;
                let out = Inline::Control {
                    info: fields.take("info")?,
                    state: fields.take("state")?,
                    inlines: fields.take("inlines")?,
                };
                fields.done()?;
                Ok(out)
            }
            "hidden" => {
                let mut fields = Fields::of(payload)?;
                let out = Inline::Hidden {
                    reason: fields.take("reason")?,
                    inlines: fields.take("inlines")?,
                };
                fields.done()?;
                Ok(out)
            }
            "looks_hidden" => {
                let mut fields = Fields::of(payload)?;
                let out = Inline::LooksHidden {
                    signal: fields.take("signal")?,
                    inlines: fields.take("inlines")?,
                };
                fields.done()?;
                Ok(out)
            }
            other => Err(unknown_variant("inline", other)),
        }
    }
}
