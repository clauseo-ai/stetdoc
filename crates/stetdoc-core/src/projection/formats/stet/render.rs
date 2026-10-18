//! A whole file: the head block, then the lines of the draft.
//!
//! The renderer writes the draft of the model, so that what a file shows is exactly what `parse` will
//! read back: every read-only fact is a `Key`, and every key has one spelling. The head block is the
//! header, the legend, the authors, the summary and the warnings. All its lines start with `{`, they
//! come first, and a parse ignores them after the header.

use super::layout::LineBuf;
use super::render_inline::paragraph;
use super::render_other::{appendix_lines, ledger_lines, lookup_text, outline_lines, thread_lines};
use super::render_structure::structure;
use crate::api::view::{Mode, Position, Warning};
use crate::projection::draft::{Draft, DraftFile, DraftLine, Policy};
use crate::projection::draft_build::{FileGroup, file_draft};
use crate::projection::formats::{
    BaselineParts, Rendered, Wrap, baseline_by_rendering, top_paragraphs,
};
use crate::projection::keys::{Key, PropKey, RevRef, Span};
use crate::projection::model::{
    Author, Block, Body, Head, Output, Projection, Story, Summary, Unit, UnitKind, UnitMeta,
};
use crate::refs::sha256;

/// The names of the files.
pub(super) const POLICY: Policy = Policy {
    files: ["document.md", "notes.md", "headers.md", "comments.md"],
    view: "view",
};

/// The one line that tells an agent the rules that stop a cold agent from breaking the file. About 70 tokens.
const LEGEND: &str = "{rules: edit the text after a ref · markers in braces and {§\"numbers\"} are read-only · a line with no ref is a new paragraph, text only · delete a line to delete its paragraph · blank lines and trailing spaces are ignored · write \\{ \\} \\\\ for a literal brace or backslash, \\t for a tab}";

/// At most this many warnings are listed in a head block.
const MAX_WARNINGS: usize = 20;

/// The name of a mode as the header spells it.
pub(super) fn mode_word(mode: Mode) -> &'static str {
    match mode {
        Mode::Final => "final",
        Mode::Original => "original",
        Mode::Redline => "redline",
    }
}

/// Builds a logical line with `fill`, twice if it is long, so that the places a break may fall are only
/// kept for a line that needs them.
pub(super) fn build(fill: impl Fn(&mut LineBuf)) -> LineBuf {
    let mut buf = LineBuf::new(false);
    fill(&mut buf);
    if buf.chars() > super::layout::WRAP_AT {
        buf = LineBuf::new(true);
        fill(&mut buf);
    }
    buf
}

/// Writes the physical lines of a logical line to the file text.
pub(super) fn emit(out: &mut String, buf: &LineBuf) -> Vec<super::layout::WrapAt> {
    let (lines, wraps) = buf.wrap();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    wraps
}

pub(super) fn render(p: &Projection) -> Rendered {
    if let Body::Lookup(lookup) = &p.body {
        return Rendered {
            files: vec![(POLICY.view.to_string(), lookup_text(lookup))],
            wraps: Vec::new(),
        };
    }
    let draft = p.draft(&POLICY);
    let mut files = Vec::new();
    let mut wraps = Vec::new();
    write_files(p, &draft, &mut wraps, &mut |name, text| {
        files.push((name, text));
    });
    Rendered { files, wraps }
}

/// What an edit of files is compared with, made so that the model is gone before any text is written. The text
/// of a file is written from its draft and the head of the model, which is all a projection of files has to say
/// beyond its stories, and is hashed and dropped as soon as it is complete.
pub(super) fn baseline(format: &super::StetFormat, model: Projection) -> BaselineParts {
    let of_files =
        matches!(model.head.output, Output::Files) && matches!(model.body, Body::Stories(_));
    if !of_files {
        return baseline_by_rendering(format, model);
    }
    let draft = model.draft(&POLICY);
    let shell = Projection {
        head: model.head.clone(),
        body: Body::Stories(Vec::new()),
        appendix: Vec::new(),
    };
    drop(model);
    let mut wraps = Vec::new();
    let mut hashes = Vec::new();
    write_files(&shell, &draft, &mut wraps, &mut |name, text| {
        hashes.push((name, sha256(text.as_bytes())));
    });
    BaselineParts {
        draft,
        wraps,
        hashes,
    }
}

/// Writes each file of a draft, with the head and the lines that a projection adds around it, and hands the
/// name and the text of each to `done`. The places long lines were broken are added to `wraps`.
fn write_files(
    p: &Projection,
    draft: &Draft,
    wraps: &mut Vec<Wrap>,
    done: &mut dyn FnMut(String, String),
) {
    for file in &draft.files {
        let mut text = String::new();
        head_block(&mut text, p, file);
        match &p.body {
            Body::Outline(outline) => outline_lines(&mut text, outline),
            Body::Threads(list) => thread_lines(&mut text, p, list),
            Body::Ledger(list) => ledger_lines(&mut text, p, list),
            Body::Stories(_) | Body::Lookup(_) => {}
        }
        for line in &file.lines {
            match line {
                DraftLine::Block(block) => {
                    let buf = build(|b| paragraph(b, block));
                    let at = emit(&mut text, &buf);
                    push_wraps(wraps, &file.name, block.r#ref, at);
                }
                DraftLine::Structure(s) => {
                    let buf = build(|b| structure(b, &s.key));
                    emit(&mut text, &buf);
                }
            }
        }
        if matches!(p.head.output, Output::View { .. }) {
            appendix_lines(&mut text, p);
        }
        if let (Output::View { .. }, Some(position)) = (&p.head.output, &p.head.position) {
            let unit = match &p.head.output {
                Output::View { name } if &**name == "outline" => "entries",
                Output::View { name } if &**name == "threads" => "threads",
                Output::View { name } if &**name == "ledger" => "changes",
                _ => "blocks",
            };
            let buf = build(|b| position_line(b, position, unit));
            emit(&mut text, &buf);
        }
        done(file.name.clone(), text);
    }
}

/// Adds the breaks of one paragraph to the list: only a paragraph with a ref can be told of a break.
fn push_wraps(
    out: &mut Vec<Wrap>,
    file: &str,
    r#ref: Option<crate::refs::BlockRef>,
    at: Vec<super::layout::WrapAt>,
) {
    let Some(r) = r#ref else { return };
    out.extend(at.into_iter().map(|at| Wrap {
        file: file.to_string(),
        r#ref: r,
        slot: at.slot,
        at: at.at,
    }));
}

/// The lines of each top-level paragraph of a projection, as a file holds them, one text for each: no head, no
/// unit line. A paragraph that a file would not hold (a blank line with no ref) is an empty text.
pub(super) fn paragraph_texts(p: &Projection) -> Vec<String> {
    top_paragraphs(p)
        .into_iter()
        .map(|(story, par)| {
            let one = Story {
                kind: story.kind,
                part: story.part.clone(),
                uses: story.uses.clone(),
                units: vec![Unit {
                    kind: UnitKind::Body,
                    id: None,
                    ordinal: 0,
                    mark: None,
                    meta: UnitMeta::default(),
                    blocks: vec![Block::Paragraph(par)],
                }],
            };
            let file = file_draft("receipt", &p.head, &[&one]);
            let mut text = String::new();
            for line in &file.lines {
                if let DraftLine::Block(block) = line {
                    emit(&mut text, &build(|b| paragraph(b, block)));
                }
            }
            text.trim_end_matches('\n').to_string()
        })
        .collect()
}

fn head_block(out: &mut String, p: &Projection, file: &DraftFile) {
    let head = &p.head;
    let group = FileGroup::ALL
        .iter()
        .find(|g| POLICY.files.get(g.index()) == Some(&file.name.as_str()))
        .copied();
    emit(out, &build(|b| header(b, head, &file.name)));
    if matches!(head.output, Output::Files) {
        emit(out, &build(|b| b.atomic(LEGEND)));
    }
    let mut used = authors_used(file);
    if let Body::Threads(list) = &p.body {
        // The authors of the comments listed, in order of first appearance.
        for comment in list.threads.iter().flat_map(|t| &t.comments) {
            if let Some(author) = comment.author.and_then(|a| head.authors.get(a as usize))
                && !used.iter().any(|c| **c == *author.code)
            {
                used.push(author.code.to_string());
            }
        }
    }
    if let Body::Ledger(list) = &p.body {
        // The authors of the changes listed, in order of first appearance.
        for change in &list.changes {
            if let Some(author) = head.author_named(change.author.as_deref())
                && !used.iter().any(|c| **c == *author.code)
            {
                used.push(author.code.to_string());
            }
        }
    }
    if let Some(buf) = authors_line(&head.authors, &used) {
        emit(out, &buf);
    }
    let wants_summary = matches!(group, Some(FileGroup::Document) | None);
    if wants_summary && let Some(buf) = summary_line(head) {
        emit(out, &buf);
    }
    let story = match group {
        Some(FileGroup::Document) | None => "body",
        Some(FileGroup::Notes) => "notes",
        Some(FileGroup::Headers) => "headers",
        Some(FileGroup::Comments) => "comments",
    };
    let mine: Vec<&Warning> = head
        .warnings
        .iter()
        .filter(|w| group.is_none() || w.story.as_deref() == Some(story))
        .collect();
    for warning in mine.iter().take(MAX_WARNINGS) {
        emit(out, &build(|b| warning_line(b, warning)));
    }
    if mine.len() > MAX_WARNINGS {
        let more = mine.len() - MAX_WARNINGS;
        emit(
            out,
            &build(|b| {
                b.atomic(&format!(
                    "{{warning more: {more} more warnings are not listed}}"
                ))
            }),
        );
    }
}

fn header(buf: &mut LineBuf, head: &Head, name: &str) {
    buf.atomic("{stet 1");
    match &head.output {
        Output::Files => buf.atomic(&format!(" file={name}")),
        Output::View { name } => {
            buf.atomic(" view=");
            buf.content(name, false, None);
        }
    }
    buf.atomic(&format!(
        " base={} mode={}",
        head.base,
        mode_word(head.mode)
    ));
    if let Some(style) = &head.common_style {
        buf.atomic(" common-style=");
        buf.quoted(style);
    }
    if matches!(head.output, Output::View { .. })
        && let Some(position) = &head.position
        && let (Some(first), Some(last)) = (&position.first, &position.last)
    {
        buf.atomic(&format!(" range={first}..{last}"));
    }
    buf.atomic("}");
}

/// The codes of the authors that the lines of a file name, in order of first use.
#[derive(Default)]
struct Used(Vec<String>);

impl Used {
    fn rev(&mut self, rev: &RevRef) {
        if !self.0.iter().any(|code| **code == *rev.author) {
            self.0.push(rev.author.to_string());
        }
    }

    fn props(&mut self, revs: &[PropKey]) {
        for prop in revs {
            self.rev(&prop.rev);
        }
    }

    fn key(&mut self, key: &Key) {
        match key {
            Key::Mark { rev, .. } | Key::CellChange { rev, .. } | Key::Deleted { rev, .. } => {
                self.rev(rev)
            }
            Key::Rev(prop) => self.rev(&prop.rev),
            Key::Open(Span::Ins(rev) | Span::MoveIn(rev))
            | Key::Close(Span::Ins(rev) | Span::MoveIn(rev)) => self.rev(rev),
            Key::Table(table) => {
                self.props(&table.revs);
                for key in &table.cell {
                    self.key(key);
                }
            }
            Key::Row(row) => {
                if let Some((_, rev)) = &row.change {
                    self.rev(rev);
                }
                self.props(&row.revs);
            }
            Key::Section(section) => {
                if let Some((_, rev)) = &section.mark {
                    self.rev(rev);
                }
                self.props(&section.revs);
            }
            _ => {}
        }
    }
}

fn authors_used(file: &DraftFile) -> Vec<String> {
    let mut used = Used::default();
    for line in &file.lines {
        match line {
            DraftLine::Block(block) => {
                for key in &block.tags {
                    used.key(key);
                }
                for seam in &block.seams {
                    used.key(&seam.key);
                }
            }
            DraftLine::Structure(s) => used.key(&s.key),
        }
    }
    used.0
}

fn authors_line(authors: &[Author], used: &[String]) -> Option<LineBuf> {
    let listed: Vec<&Author> = authors
        .iter()
        .filter(|a| used.iter().any(|code| **code == *a.code))
        .collect();
    if listed.is_empty() {
        return None;
    }
    Some(build(|buf| {
        buf.atomic("{authors:");
        for (index, author) in listed.iter().enumerate() {
            if index > 0 {
                buf.atomic(" ·");
            }
            buf.atomic(&format!(" {} ", author.code));
            buf.quoted(&author.name);
        }
        buf.atomic("}");
    }))
}

fn summary_line(head: &Head) -> Option<LineBuf> {
    let parts = summary_parts(&head.summary, &head.authors, head.warnings.len());
    if parts.is_empty() {
        return None;
    }
    Some(build(|buf| {
        buf.atomic(&format!("{{summary: {}}}", parts.join(" · ")))
    }))
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn summary_parts(summary: &Summary, authors: &[Author], warnings: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let total: u32 = summary.changes.iter().sum();
    if total > 0 {
        let by: Vec<String> = summary
            .changes
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .map(|(index, n)| {
                let code = authors.get(index).map_or("?", |a| &*a.code);
                format!("{code} {n}")
            })
            .collect();
        parts.push(format!(
            "{} ({})",
            plural(total, "tracked change", "tracked changes"),
            by.join(", ")
        ));
    }
    if summary.comments > 0 {
        let mut comments = plural(summary.comments, "comment", "comments");
        if summary.resolved > 0 {
            comments.push_str(&format!(", {} resolved", summary.resolved));
        }
        parts.push(comments);
    }
    if summary.footnotes > 0 {
        parts.push(plural(summary.footnotes, "footnote", "footnotes"));
    }
    if summary.endnotes > 0 {
        parts.push(plural(summary.endnotes, "endnote", "endnotes"));
    }
    if summary.sections > 0 {
        parts.push(plural(summary.sections, "section", "sections"));
    }
    if summary.readonly > 0 {
        parts.push(plural(
            summary.readonly,
            "read-only paragraph",
            "read-only paragraphs",
        ));
    }
    if warnings > 0 {
        parts.push(plural(
            u32::try_from(warnings).unwrap_or(u32::MAX),
            "warning",
            "warnings",
        ));
    }
    parts
}

fn warning_line(buf: &mut LineBuf, warning: &Warning) {
    buf.atomic(&format!("{{warning {}", warning.code.as_str()));
    if let Some(block) = &warning.block {
        buf.atomic(&format!(" {block}"));
    }
    buf.atomic(": ");
    buf.content(&warning.message, false, None);
    if let Some(next) = &warning.next {
        buf.atomic(" · next: ");
        buf.content(next, false, None);
    }
    buf.atomic("}");
}

/// `8152` as `8,152`.
fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn position_line(buf: &mut LineBuf, position: &Position, unit: &str) {
    buf.atomic("{position: ");
    match (&position.first, &position.last) {
        (Some(first), Some(last)) => buf.atomic(&format!("{first}..{last} of ")),
        _ => buf.atomic("nothing of "),
    }
    buf.content(&position.scope, false, None);
    buf.atomic(&format!(
        " ({} of {} {unit})",
        grouped(position.shown),
        grouped(position.total)
    ));
    if let Some(last) = &position.last
        && position.shown < position.total
    {
        buf.atomic(&format!(" · next: --after {last}"));
    }
    let tokens = position.remaining_tokens;
    if tokens >= 1000 {
        buf.atomic(&format!(" · about {}K tokens remain", tokens / 1000));
    } else if tokens > 0 {
        buf.atomic(&format!(" · about {tokens} tokens remain"));
    }
    buf.atomic("}");
}
