//! The parts of a page that are not lines of a file: margin notes, the outline, and the appendix.
//!
//! None of them is ever in a file, so none of them has a parser: a line of a view that is pasted into a file
//! is refused with a message that says where it came from (`parse.rs`).

use super::layout::LineBuf;
use super::refs_syntax::write_ref;
use super::render::{build, emit};
use super::words::note_letters;
use crate::api::lookup::{Hit, Lookup, Reference, Site};
use crate::api::revision::RevisionInfo;
use crate::projection::ledger::family;
use crate::projection::model::{
    AppendixEntry, ChangeList, Margin, MentionState, Outline, OutlineSource, Projection, Target,
    TermCount, ThreadList,
};

/// The needles a block holds and how many times each: `"payment" 2, "days" 1`.
pub(super) fn terms_text(terms: &[TermCount]) -> String {
    terms
        .iter()
        .map(|t| format!("{:?} {}", &*t.term, t.count))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The line of a margin note: `{~ 3 matches}`.
pub(super) fn margin_line(buf: &mut LineBuf, margin: &Margin) {
    match margin {
        Margin::Matches(n) => {
            buf.atomic(&format!(
                "{{~ {n} {}}}",
                if *n == 1 { "match" } else { "matches" }
            ));
        }
        Margin::Terms(terms) => buf.atomic(&format!("{{~ {}}}", terms_text(terms))),
        Margin::Gap(n) => {
            buf.atomic(&format!(
                "{{~ {n} {} not shown}}",
                if *n == 1 { "block" } else { "blocks" }
            ));
        }
        Margin::Continued { table, from } => {
            buf.atomic(&format!(
                "{{~ table {} goes on at row {}}}",
                write_ref(*table),
                write_ref(*from)
            ));
        }
        Margin::Mention {
            text,
            state,
            targets,
            more,
        } => mention_line(buf, text, *state, targets, *more),
        Margin::CitedBy { from, more } => {
            let refs: Vec<String> = from.iter().map(|r| write_ref(*r)).collect();
            let tail = if *more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            buf.atomic(&format!("{{~ cited by {}{tail}}}", refs.join(", ")));
        }
    }
}

/// `{~ "clause 9.1" -> p301 (9.1 Notices)}`, and the other ways a mention comes out.
fn mention_line(buf: &mut LineBuf, text: &str, state: MentionState, targets: &[Target], more: u32) {
    let target = |buf: &mut LineBuf, t: &Target| {
        buf.atomic(&write_ref(t.r#ref));
        if !t.text.is_empty() {
            buf.atomic(" (");
            buf.content(&t.text, false, None);
            buf.atomic(")");
        }
    };
    let list = |buf: &mut LineBuf| {
        for (i, t) in targets.iter().enumerate() {
            if i > 0 {
                buf.atomic(", ");
            }
            target(buf, t);
        }
        if more > 0 {
            buf.atomic(&format!(" and {more} more"));
        }
    };
    buf.atomic("{~ ");
    buf.quoted(text);
    buf.atomic(" -> ");
    match state {
        MentionState::Resolved => list(buf),
        MentionState::Relative => {
            // The chosen target, then the candidates it was chosen over.
            if let Some((chosen, rest)) = targets.split_first() {
                target(buf, chosen);
                buf.atomic(" relative to this block");
                if !rest.is_empty() || more > 0 {
                    buf.atomic(", or else ");
                    for (i, t) in rest.iter().enumerate() {
                        if i > 0 {
                            buf.atomic(", ");
                        }
                        target(buf, t);
                    }
                    if more > 0 {
                        buf.atomic(&format!(" and {more} more"));
                    }
                }
            }
        }
        MentionState::Ambiguous => {
            buf.atomic("ambiguous: ");
            list(buf);
        }
        MentionState::Outside => {
            buf.atomic("outside the document");
            if !targets.is_empty() {
                buf.atomic(", though this document has ");
                list(buf);
            }
        }
        MentionState::Unresolved => buf.atomic("unresolved"),
    }
    buf.atomic("}");
}

/// A count of characters as a reader skims it: `812`, `18.2K`.
fn size(n: u32) -> String {
    if n < 1000 {
        n.to_string()
    } else {
        format!("{}.{}K", n / 1000, (n % 1000) / 100)
    }
}

fn source_words(source: OutlineSource) -> &'static str {
    match source {
        OutlineSource::Styles => "headings from styles",
        OutlineSource::Numbered => "headings from numbered paragraphs",
        OutlineSource::BoldLines => "headings from bold lines",
        OutlineSource::Chunks => "no headings found, so runs of blocks",
    }
}

/// The lines of an outline: what it is made from, then one line for each entry.
pub(super) fn outline_lines(out: &mut String, outline: &Outline) {
    let shown = u32::try_from(outline.entries.len()).unwrap_or(u32::MAX);
    let mut info = format!("{{outline: {}", source_words(outline.source));
    if outline.source != OutlineSource::Chunks {
        info.push_str(&format!(" · to level {}", outline.depth));
    }
    if shown < outline.total {
        info.push_str(&format!(" · {shown} of {} entries", outline.total));
    } else {
        info.push_str(&format!(
            " · {shown} {}",
            if shown == 1 { "entry" } else { "entries" }
        ));
    }
    info.push('}');
    emit(out, &build(|b| b.atomic(&info)));
    for entry in &outline.entries {
        let buf = build(|b| {
            for _ in 1..entry.level {
                b.atomic("  ");
            }
            if let Some(r) = entry.r#ref {
                b.atomic(&write_ref(r));
                b.atomic(" ");
            }
            if let Some(label) = &entry.label {
                b.atomic("{§");
                b.quoted(label);
                b.atomic("} ");
            }
            b.content(&entry.text, false, None);
            if entry.hidden > 0 {
                b.atomic(&format!(" (+{} entries)", entry.hidden));
            }
            let mut sizes = format!(
                "  [{} {}, {} chars",
                entry.blocks,
                if entry.blocks == 1 { "block" } else { "blocks" },
                size(entry.chars)
            );
            if let (Some(first), Some(last)) = (entry.first, entry.last) {
                sizes.push_str(&format!(", {}..{}", write_ref(first), write_ref(last)));
            }
            sizes.push(']');
            b.atomic(&sizes);
        });
        emit(out, &buf);
    }
}

/// The appendix: the notes and comments that the page cites.
pub(super) fn appendix_lines(out: &mut String, p: &Projection) {
    if p.appendix.is_empty() {
        return;
    }
    emit(out, &build(|b| b.atomic("{appendix}")));
    for entry in &p.appendix {
        let buf = build(|b| match entry {
            AppendixEntry::Note {
                kind,
                ordinal,
                text,
                cut,
            } => {
                b.atomic(&format!("{{{}{ordinal}}} ", note_letters(*kind)));
                b.content(text, false, None);
                if *cut {
                    let file = match kind {
                        crate::projection::model::NoteKind::Footnote
                        | crate::projection::model::NoteKind::Endnote => "notes.md",
                    };
                    b.atomic(&format!(" {{more in {file}}}"));
                }
            }
            AppendixEntry::Comment {
                id,
                author,
                date,
                on,
                anchored,
                text,
                cut,
            } => {
                b.atomic(&format!("{{c{id}"));
                if let Some(code) = author.and_then(|a| p.head.authors.get(a as usize)) {
                    b.atomic(&format!(" {}", code.code));
                }
                if let Some(date) = date {
                    let day = date.split('T').next().unwrap_or(date);
                    b.atomic(&format!(" {day}"));
                }
                if let Some(r) = on {
                    b.atomic(&format!(" on {}", write_ref(*r)));
                }
                if let Some(text) = anchored {
                    b.atomic(" ");
                    b.quoted(text);
                }
                b.atomic("} ");
                b.content(text, false, None);
                if *cut {
                    b.atomic(" {more in comments.md}");
                }
            }
        });
        emit(out, &buf);
    }
}

/// The lines of a `Comments` view: what is listed, then for each thread a line that says whether it is resolved
/// and where it is anchored, and one line for each of its comments, the root first.
///
/// ```text
/// {threads: 2 of 5 threads · 1 resolved}
/// {thread c3 open · on p121 "Purchase Price"}
///   {c3 JD 2026-09-30} Should this be capitalised?
///   {c4 EW 2026-10-01} Yes.
/// ```
pub(super) fn thread_lines(out: &mut String, p: &Projection, list: &ThreadList) {
    let shown = u32::try_from(list.threads.len()).unwrap_or(u32::MAX);
    let mut info = if shown < list.total {
        format!("{{threads: {shown} of {} threads", list.total)
    } else {
        format!(
            "{{threads: {shown} {}",
            if shown == 1 { "thread" } else { "threads" }
        )
    };
    if list.resolved > 0 {
        info.push_str(&format!(" · {} resolved", list.resolved));
    }
    info.push('}');
    emit(out, &build(|b| b.atomic(&info)));
    for thread in &list.threads {
        let root = thread.comments.first();
        let buf = build(|b| {
            b.atomic(&format!(
                "{{thread c{} {}",
                thread.root,
                if thread.resolved { "resolved" } else { "open" }
            ));
            match root.and_then(|c| c.on) {
                Some(r) => b.atomic(&format!(" · on {}", write_ref(r))),
                None if root.is_some_and(|c| c.anchored.is_none()) => b.atomic(" · unanchored"),
                None => {}
            }
            if let Some(text) = root.and_then(|c| c.anchored.as_deref()) {
                b.atomic(" ");
                b.quoted(text);
            }
            b.atomic("}");
        });
        emit(out, &buf);
        for comment in &thread.comments {
            let buf = build(|b| {
                b.atomic(&format!("  {{c{}", comment.id));
                if let Some(code) = comment.author.and_then(|a| p.head.authors.get(a as usize)) {
                    b.atomic(&format!(" {}", code.code));
                }
                if let Some(date) = &comment.date {
                    let day = date.split('T').next().unwrap_or(date);
                    b.atomic(&format!(" {day}"));
                }
                b.atomic("} ");
                b.content(&comment.text, false, None);
            });
            emit(out, &buf);
        }
    }
}

/// How many characters of a change's text its line shows.
const LINE_TEXT: usize = 80;

/// The lines of a `Revisions` view: what is listed, then one line for each change, in ledger order.
///
/// ```text
/// {changes: 2 of 5 changes · by JD}
/// #12 insert JD 2026-09-30 p42 "the Buyer shall"
/// #13 delete EW 2026-10-01 p42 "shall"
/// ```
pub(super) fn ledger_lines(out: &mut String, p: &Projection, list: &ChangeList) {
    let shown = u32::try_from(list.changes.len()).unwrap_or(u32::MAX);
    let mut info = if shown < list.total {
        format!("{{changes: {shown} of {} changes", list.total)
    } else {
        format!(
            "{{changes: {shown} {}",
            if shown == 1 { "change" } else { "changes" }
        )
    };
    info.push('}');
    emit(out, &build(|b| b.atomic(&info)));
    for change in &list.changes {
        emit(out, &build(|b| change_line(b, p, change, list.full)));
    }
}

/// One change: its ids, its kind as a family, its author, its day, its block, its story when it is not the body, and
/// the text it deletes and inserts, each cut short unless the list is `full`.
fn change_line(b: &mut LineBuf, p: &Projection, change: &RevisionInfo, full: bool) {
    let ids: Vec<String> = change.ids.iter().map(|id| id.to_string()).collect();
    b.atomic(&format!("#{} {}", ids.join(","), family(change.kind)));
    let who = match p.head.author_named(change.author.as_deref()) {
        Some(author) => author.code.to_string(),
        None => "Unknown".to_string(),
    };
    b.atomic(&format!(" {who}"));
    if let Some(date) = &change.date {
        let day = date.split('T').next().unwrap_or(date);
        b.atomic(&format!(" {day}"));
    }
    if let Some(block) = &change.block {
        b.atomic(&format!(" {block}"));
    }
    if change.story != "body" {
        b.atomic(&format!(" in {}", change.story));
    }
    let limit = if full { None } else { Some(LINE_TEXT) };
    let deleted = one_line(change.deleted.as_deref(), limit, change.text_truncated);
    let inserted = one_line(change.inserted.as_deref(), limit, change.text_truncated);
    match (deleted, inserted) {
        (Some(old), Some(new)) => {
            b.atomic(" ");
            b.quoted(&old);
            b.atomic(" -> ");
            b.quoted(&new);
        }
        (Some(text), None) | (None, Some(text)) => {
            b.atomic(" ");
            b.quoted(&text);
        }
        (None, None) => {}
    }
}

/// The text on one line, its white space collapsed and cut to `limit` characters with an ellipsis, or `None` for none.
/// With no limit, a text that the description itself cut (`cut`, and the text is at the most a description carries)
/// ends with an ellipsis too.
fn one_line(text: Option<&str>, limit: Option<usize>, cut: bool) -> Option<String> {
    let raw = text?;
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    let Some(limit) = limit else {
        let at_most = cut && raw.chars().count() >= crate::revisions::changes::CHANGE_TEXT_LIMIT;
        return Some(if at_most { format!("{text}…") } else { text });
    };
    if text.chars().count() <= limit {
        return Some(text);
    }
    let head: String = text.chars().take(limit).collect();
    Some(format!("{head}…"))
}

/// How many uses and definitions a hit lists before it counts the rest.
const SITES_SHOWN: usize = 12;

fn site_line(out: &mut String, word: &str, site: &Site) {
    let buf = build(|b| {
        b.atomic(&format!("  {word} {}", site.block));
        if site.kind != "use" {
            b.atomic(&format!(" ({})", site.kind));
        }
        if site.story != "body" {
            b.atomic(&format!(" in {}", site.story));
        }
        b.atomic(": ");
        b.content(&site.snippet, false, None);
    });
    emit(out, &buf);
}

fn reference_line(out: &mut String, word: &str, shown: &str, r: &Reference) {
    let buf = build(|b| {
        if shown.is_empty() {
            b.atomic(&format!("  {word} "));
        } else {
            b.atomic(&format!("  {word} {shown} "));
        }
        b.quoted(&r.text);
        b.atomic(&format!(" ({}", r.state));
        match (&r.to, r.candidates.as_slice()) {
            (Some(to), _) => b.atomic(&format!(" -> {to}")),
            (None, []) => {}
            (None, many) => b.atomic(&format!(" -> {}", many.join(", "))),
        }
        b.atomic(")");
    });
    emit(out, &buf);
}

/// The page of a lookup: the head, then each hit with its details.
pub(super) fn lookup_text(lookup: &Lookup) -> String {
    let mut out = String::new();
    let head = build(|b| {
        b.atomic(&format!("{{stet 1 view=lookup base={} query=", lookup.base));
        b.quoted(&lookup.query);
        b.atomic("}");
    });
    emit(&mut out, &head);
    for hit in &lookup.hits {
        match hit {
            Hit::Term {
                term,
                matched,
                definitions,
                uses,
                use_count,
                multiple_definitions,
            } => {
                let buf = build(|b| {
                    b.atomic("{term ");
                    b.quoted(term);
                    b.atomic(&format!(
                        " \u{b7} {matched} \u{b7} {} {}",
                        definitions.len(),
                        if definitions.len() == 1 {
                            "definition"
                        } else {
                            "definitions"
                        }
                    ));
                    if *multiple_definitions {
                        b.atomic(" (defined more than once)");
                    }
                    b.atomic(&format!(
                        " \u{b7} {use_count} {}}}",
                        if *use_count == 1 { "use" } else { "uses" }
                    ));
                });
                emit(&mut out, &buf);
                for d in definitions {
                    site_line(&mut out, "defined", d);
                }
                for u in uses.iter().take(SITES_SHOWN) {
                    site_line(&mut out, "used", u);
                }
                let listed = u32::try_from(uses.len().min(SITES_SHOWN)).unwrap_or(u32::MAX);
                if *use_count > listed {
                    emit(
                        &mut out,
                        &build(|b| b.atomic(&format!("  and {} more uses", use_count - listed))),
                    );
                }
            }
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
            } => {
                let buf = build(|b| {
                    b.atomic(&format!("{{clause {block} "));
                    b.quoted(full);
                    b.atomic(&format!(" \u{b7} {source}"));
                    if *composed {
                        b.atomic(" \u{b7} composed across lists");
                    }
                    if let Some(noun) = noun {
                        b.atomic(&format!(" \u{b7} {noun}"));
                    }
                    if story != "body" {
                        b.atomic(&format!(" \u{b7} in {story}"));
                    }
                    b.atomic(&format!(" \u{b7} matched by {matched_by}}}"));
                });
                emit(&mut out, &buf);
                emit(
                    &mut out,
                    &build(|b| {
                        b.atomic("  ");
                        b.quoted(label);
                        b.atomic(" ");
                        b.content(text, false, None);
                    }),
                );
            }
            Hit::Block {
                block,
                story,
                label,
                full,
                text,
                inbound,
                outbound,
                defines,
            } => {
                let buf = build(|b| {
                    b.atomic(&format!("{{block {block}"));
                    if story != "body" {
                        b.atomic(&format!(" in {story}"));
                    }
                    if let (Some(label), Some(full)) = (label, full) {
                        b.atomic(" \u{b7} clause ");
                        b.quoted(label);
                        b.atomic(" full ");
                        b.quoted(full);
                    }
                    b.atomic("}");
                });
                emit(&mut out, &buf);
                emit(
                    &mut out,
                    &build(|b| {
                        b.atomic("  ");
                        b.content(text, false, None);
                    }),
                );
                if !defines.is_empty() {
                    emit(
                        &mut out,
                        &build(|b| {
                            b.atomic("  defines ");
                            for (i, term) in defines.iter().enumerate() {
                                if i > 0 {
                                    b.atomic(", ");
                                }
                                b.quoted(term);
                            }
                        }),
                    );
                }
                for r in inbound {
                    reference_line(&mut out, "cited by", &r.from, r);
                }
                for r in outbound {
                    reference_line(&mut out, "cites", "", r);
                }
            }
        }
    }
    out
}
