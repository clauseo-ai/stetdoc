//! The JSON form of the parts of a model that only a view has (see `json_model.rs`).

use super::json_model::{json_names, json_struct};
use super::json_value::{
    Fields, FromJson, Json, JsonError, ToJson, object, tagged, unknown_variant, variant,
};
use crate::api::lookup::{Hit, Lookup, Reference, Site};
use crate::api::revision::{ChangeKind as RevisionKind, RevisionInfo};
use crate::projection::model::{
    AppendixEntry, ChangeList, FindHit, HitList, Margin, MentionState, NoteKind, Outline,
    OutlineEntry, OutlineSource, Target, TermCount, ThreadComment, ThreadEntry, ThreadList, Tip,
};
use crate::refs::BlockRef;

json_names!(OutlineSource, "outline source", {
    Styles => "styles", Numbered => "numbered", BoldLines => "bold_lines", Chunks => "chunks"
});
json_names!(MentionState, "mention state", {
    Resolved => "resolved", Relative => "relative", Ambiguous => "ambiguous", Outside => "outside",
    Unresolved => "unresolved"
});
json_struct!(Target { r#ref => "ref", text => "text" });
json_struct!(Outline { source => "source", depth => "depth", total => "total", entries => "entries" });
json_struct!(OutlineEntry {
    r#ref => "ref", level => "level", label => "label", text => "text", blocks => "blocks",
    chars => "chars", first => "first", last => "last", hidden => "hidden"
});
json_names!(RevisionKind, "revision change kind", {
    Insertion => "insertion", Deletion => "deletion", Move => "move",
    ParagraphInserted => "paragraph_inserted", ParagraphDeleted => "paragraph_deleted",
    MarkInserted => "mark_inserted", MarkDeleted => "mark_deleted", RowInserted => "row_inserted",
    RowDeleted => "row_deleted", CellInserted => "cell_inserted", CellDeleted => "cell_deleted",
    CellMerged => "cell_merged", RunFormat => "run_format", ParagraphFormat => "paragraph_format",
    SectionFormat => "section_format", TableFormat => "table_format", RowFormat => "row_format",
    CellFormat => "cell_format", GridChange => "grid_change", NumberingChange => "numbering_change"
});
json_struct!(RevisionInfo {
    ids => "ids", kind => "kind", author => "author", date => "date", block => "block",
    story => "story", inserted => "inserted", deleted => "deleted", text_truncated => "text_truncated"
});
// `full` is written only when it is set, so that every other ledger reads as it did.
impl ToJson for ChangeList {
    fn to_json(&self) -> Json {
        let mut fields = vec![
            ("total".to_string(), self.total.to_json()),
            ("changes".to_string(), self.changes.to_json()),
        ];
        if self.full {
            fields.push(("full".to_string(), true.to_json()));
        }
        Json::Obj(fields)
    }
}

impl FromJson for ChangeList {
    fn from_json(value: &Json) -> Result<ChangeList, JsonError> {
        let mut fields = Fields::of(value)?;
        let out = ChangeList {
            total: fields.take("total")?,
            changes: fields.take("changes")?,
            full: fields.take::<Option<bool>>("full")?.unwrap_or(false),
        };
        fields.done()?;
        Ok(out)
    }
}
json_struct!(TermCount { term => "term", count => "count" });
json_struct!(HitList { needles => "needles", hits => "hits", tips => "tips" });
json_struct!(FindHit {
    r#ref => "ref", under => "under", clause => "clause", text => "text", cut_before => "cut_before",
    cut_after => "cut_after", terms => "terms", deleted_only => "deleted_only"
});
json_struct!(Tip { r#ref => "ref", clause => "clause", text => "text" });
json_struct!(ThreadList { total => "total", resolved => "resolved", threads => "threads" });
json_struct!(ThreadEntry { root => "root", resolved => "resolved", comments => "comments" });
json_struct!(ThreadComment {
    id => "id", author => "author", date => "date", on => "on", anchored => "anchored", text => "text"
});

impl ToJson for Margin {
    fn to_json(&self) -> Json {
        match self {
            Margin::Matches(n) => tagged("matches", n.to_json()),
            Margin::Terms(terms) => tagged("terms", terms.to_json()),
            Margin::Gap(n) => tagged("gap", n.to_json()),
            Margin::Continued { table, from } => tagged(
                "continued",
                object(vec![("table", table.to_json()), ("from", from.to_json())]),
            ),
            Margin::Mention {
                text,
                state,
                targets,
                more,
            } => tagged(
                "mention",
                object(vec![
                    ("text", text.to_json()),
                    ("state", state.to_json()),
                    ("targets", targets.to_json()),
                    ("more", more.to_json()),
                ]),
            ),
            Margin::CitedBy { from, more } => tagged(
                "cited_by",
                object(vec![("from", from.to_json()), ("more", more.to_json())]),
            ),
        }
    }
}

impl FromJson for Margin {
    fn from_json(value: &Json) -> Result<Margin, JsonError> {
        match variant(value)? {
            ("matches", Some(n)) => u32::from_json(n).map(Margin::Matches),
            ("terms", Some(t)) => Vec::from_json(t).map(Margin::Terms),
            ("gap", Some(n)) => u32::from_json(n).map(Margin::Gap),
            ("continued", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let table: BlockRef = fields.take("table")?;
                let from: BlockRef = fields.take("from")?;
                fields.done()?;
                Ok(Margin::Continued { table, from })
            }
            ("mention", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let text: Box<str> = fields.take("text")?;
                let state: MentionState = fields.take("state")?;
                let targets: Vec<Target> = fields.take("targets")?;
                let more: u32 = fields.take("more")?;
                fields.done()?;
                Ok(Margin::Mention {
                    text,
                    state,
                    targets,
                    more,
                })
            }
            ("cited_by", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let from: Vec<BlockRef> = fields.take("from")?;
                let more: u32 = fields.take("more")?;
                fields.done()?;
                Ok(Margin::CitedBy { from, more })
            }
            (other, _) => Err(unknown_variant("margin note", other)),
        }
    }
}

impl ToJson for AppendixEntry {
    fn to_json(&self) -> Json {
        match self {
            AppendixEntry::Note {
                kind,
                ordinal,
                text,
                cut,
            } => tagged(
                "note",
                object(vec![
                    ("kind", kind.to_json()),
                    ("ordinal", ordinal.to_json()),
                    ("text", text.to_json()),
                    ("cut", cut.to_json()),
                ]),
            ),
            AppendixEntry::Comment {
                id,
                author,
                date,
                on,
                anchored,
                text,
                cut,
            } => tagged(
                "comment",
                object(vec![
                    ("id", id.to_json()),
                    ("author", author.to_json()),
                    ("date", date.to_json()),
                    ("on", on.to_json()),
                    ("anchored", anchored.to_json()),
                    ("text", text.to_json()),
                    ("cut", cut.to_json()),
                ]),
            ),
        }
    }
}

impl FromJson for AppendixEntry {
    fn from_json(value: &Json) -> Result<AppendixEntry, JsonError> {
        match variant(value)? {
            ("note", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let kind: NoteKind = fields.take("kind")?;
                let out = AppendixEntry::Note {
                    kind,
                    ordinal: fields.take("ordinal")?,
                    text: fields.take("text")?,
                    cut: fields.take("cut")?,
                };
                fields.done()?;
                Ok(out)
            }
            ("comment", Some(payload)) => {
                let mut fields = Fields::of(payload)?;
                let out = AppendixEntry::Comment {
                    id: fields.take("id")?,
                    author: fields.take("author")?,
                    date: fields.take("date")?,
                    on: fields.take("on")?,
                    anchored: fields.take("anchored")?,
                    text: fields.take("text")?,
                    cut: fields.take("cut")?,
                };
                fields.done()?;
                Ok(out)
            }
            (other, _) => Err(unknown_variant("appendix entry", other)),
        }
    }
}

json_struct!(Site { block => "block", story => "story", kind => "kind", snippet => "snippet" });
json_struct!(Reference {
    from => "from", to => "to", text => "text", state => "state", candidates => "candidates"
});
json_struct!(Lookup { base => "base", query => "query", hits => "hits", text => "text" });

impl ToJson for Hit {
    fn to_json(&self) -> Json {
        match self {
            Hit::Block {
                block,
                story,
                label,
                full,
                text,
                inbound,
                outbound,
                defines,
            } => tagged(
                "block",
                object(vec![
                    ("block", block.to_json()),
                    ("story", story.to_json()),
                    ("label", label.to_json()),
                    ("full", full.to_json()),
                    ("text", text.to_json()),
                    ("inbound", inbound.to_json()),
                    ("outbound", outbound.to_json()),
                    ("defines", defines.to_json()),
                ]),
            ),
            Hit::Clause {
                block,
                story,
                label,
                full,
                source,
                composed,
                noun,
                text,
                matched_by,
            } => tagged(
                "clause",
                object(vec![
                    ("block", block.to_json()),
                    ("story", story.to_json()),
                    ("label", label.to_json()),
                    ("full", full.to_json()),
                    ("source", source.to_json()),
                    ("composed", composed.to_json()),
                    ("noun", noun.to_json()),
                    ("text", text.to_json()),
                    ("matched_by", matched_by.to_json()),
                ]),
            ),
            Hit::Term {
                term,
                matched,
                definitions,
                uses,
                use_count,
                multiple_definitions,
            } => tagged(
                "term",
                object(vec![
                    ("term", term.to_json()),
                    ("matched", matched.to_json()),
                    ("definitions", definitions.to_json()),
                    ("uses", uses.to_json()),
                    ("use_count", use_count.to_json()),
                    ("multiple_definitions", multiple_definitions.to_json()),
                ]),
            ),
        }
    }
}

impl FromJson for Hit {
    fn from_json(value: &Json) -> Result<Hit, JsonError> {
        match variant(value)? {
            ("block", Some(payload)) => {
                let mut f = Fields::of(payload)?;
                let hit = Hit::Block {
                    block: f.take("block")?,
                    story: f.take("story")?,
                    label: f.take("label")?,
                    full: f.take("full")?,
                    text: f.take("text")?,
                    inbound: f.take("inbound")?,
                    outbound: f.take("outbound")?,
                    defines: f.take("defines")?,
                };
                f.done()?;
                Ok(hit)
            }
            ("clause", Some(payload)) => {
                let mut f = Fields::of(payload)?;
                let hit = Hit::Clause {
                    block: f.take("block")?,
                    story: f.take("story")?,
                    label: f.take("label")?,
                    full: f.take("full")?,
                    source: f.take("source")?,
                    composed: f.take("composed")?,
                    noun: f.take("noun")?,
                    text: f.take("text")?,
                    matched_by: f.take("matched_by")?,
                };
                f.done()?;
                Ok(hit)
            }
            ("term", Some(payload)) => {
                let mut f = Fields::of(payload)?;
                let hit = Hit::Term {
                    term: f.take("term")?,
                    matched: f.take("matched")?,
                    definitions: f.take("definitions")?,
                    uses: f.take("uses")?,
                    use_count: f.take("use_count")?,
                    multiple_definitions: f.take("multiple_definitions")?,
                };
                f.done()?;
                Ok(hit)
            }
            (other, _) => Err(unknown_variant("lookup hit", other)),
        }
    }
}
