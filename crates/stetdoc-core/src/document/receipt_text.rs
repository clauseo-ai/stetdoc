//! `Receipt.text`: the form an agent reads (design `document-and-ops.md` 4.3).
//!
//! A head line, one line for each operation, the blocks as the readable file will hold them, then the warnings and what `check`
//! carried. It is deterministic and token-lean, and it lives in this one file so that the format experiments can change its
//! wording without touching the data.

use crate::api::receipt::{ManualMark, OpResult, Receipt};
use crate::api::revision::ChangeKind;

/// What a change is called in a line of a receipt.
fn kind_word(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Insertion => "insertion",
        ChangeKind::Deletion => "deletion",
        ChangeKind::Move => "move",
        ChangeKind::ParagraphInserted => "paragraph insertion",
        ChangeKind::ParagraphDeleted => "paragraph deletion",
        ChangeKind::MarkInserted => "split",
        ChangeKind::MarkDeleted => "merge",
        ChangeKind::RowInserted => "row insertion",
        ChangeKind::RowDeleted => "row deletion",
        ChangeKind::CellInserted => "cell insertion",
        ChangeKind::CellDeleted => "cell deletion",
        ChangeKind::CellMerged => "cell merge",
        ChangeKind::RunFormat => "run formatting",
        ChangeKind::ParagraphFormat => "paragraph formatting",
        ChangeKind::SectionFormat => "section formatting",
        ChangeKind::TableFormat => "table formatting",
        ChangeKind::RowFormat => "row formatting",
        ChangeKind::CellFormat => "cell formatting",
        ChangeKind::GridChange => "table grid change",
        ChangeKind::NumberingChange => "numbering change",
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The longest text a receipt line quotes of manual markup; the JSON has all of it.
const QUOTED: usize = 60;

/// A text quoted for a receipt line, cut in the middle when it is long.
fn quoted(text: &str) -> String {
    let n = text.chars().count();
    if n <= QUOTED {
        return format!("{text:?}");
    }
    let head: String = text.chars().take(QUOTED / 2).collect();
    let tail: String = text.chars().skip(n - QUOTED / 2).collect();
    format!("{:?}", format!("{head}…{tail}"))
}

/// One change of manual markup, in words: `p4: struck "thirty", underlined "sixty"`.
fn marked(m: &ManualMark, with_block: bool) -> String {
    let mut said = Vec::new();
    if !m.struck.is_empty() {
        said.push(format!("struck {}", quoted(&m.struck)));
    }
    if !m.underlined.is_empty() {
        said.push(format!("underlined {}", quoted(&m.underlined)));
    }
    match m.block.as_ref().filter(|_| with_block) {
        Some(block) => format!("{block}: {}", said.join(", ")),
        None => said.join(", "),
    }
}

/// What one operation did, in words.
fn summary(op: &OpResult, comment: Option<String>, dry_run: bool, r: &Receipt) -> String {
    let tracked = r.tracked;
    let mut parts: Vec<String> = comment.into_iter().collect();
    let several = op.blocks.len() + op.added.len() > 1;
    parts.extend(op.marked.iter().map(|m| marked(m, several)));
    parts.extend(op.revisions.iter().map(|c| match c.ids.first() {
        Some(id) => format!("{} #{id}", kind_word(c.kind)),
        None => kind_word(c.kind).to_string(),
    }));
    if let Some(r) = &op.resolution {
        let done = if r.action == "accept" {
            "accepted"
        } else {
            "rejected"
        };
        if r.resolved.is_empty() {
            parts.push(format!("{done} nothing"));
        }
        for c in &r.resolved {
            parts.push(format!(
                "{done} {} by {}",
                plural(
                    c.count as usize,
                    &format!("{} element", c.kind),
                    &format!("{} elements", c.kind)
                ),
                c.author
            ));
        }
        if !r.removed.is_empty() {
            let gone: Vec<&str> = r.removed.iter().map(|x| x.r#ref.as_str()).collect();
            parts.push(format!("removed {}", gone.join(", ")));
        }
        for (first, second) in &r.joined {
            parts.push(format!("{second} joined into {first}"));
        }
    }
    if op.added_count > 0 && dry_run {
        parts.push(format!(
            "adds {}",
            plural(op.added_count as usize, "new block", "new blocks")
        ));
    }
    for r in &op.removed_own {
        parts.push(format!("removed your own pending {}", r.kind));
    }
    if parts.is_empty() {
        match (op.changed, tracked) {
            (false, _) => "no change".to_string(),
            (true, false) => "applied directly".to_string(),
            (true, true) => "its revisions were resolved by a later operation".to_string(),
        }
    } else {
        parts.join(", ")
    }
}

/// What an operation did to a comment: `c12 added`, `c3 resolved`.
fn comment_said(r: &Receipt, op: &OpResult) -> Option<String> {
    let id = op.comment.as_deref()?;
    let entry = r.comments.iter().find(|c| c.id == id)?;
    Some(format!("{id} {}", entry.action))
}

/// The text of a receipt.
pub(super) fn compose(r: &Receipt) -> String {
    let mut out = String::new();
    let ops = plural(r.operations.len(), "operation", "operations");
    let who = if r.markup.is_manual() {
        "as manual markup: old text kept and struck through, new text underlined, and no tracked changes. Each operation says what it struck and underlined; the text of a block below runs old and new together. Tell the reader this is a manual redline, not Track Changes".to_string()
    } else if r.tracked {
        format!("tracked as {} {}", r.stamp.author, r.stamp.date)
    } else {
        "untracked".to_string()
    };
    if r.dry_run {
        out.push_str(&format!(
            "dry run: nothing was applied. A new block has no ref until the edit is applied. {ops}, {who}. base {}\n",
            r.base
        ));
    } else if r.operations.iter().any(|o| o.changed) {
        out.push_str(&format!("applied {ops}, {who}. base {}\n", r.base));
    } else {
        out.push_str(&format!(
            "applied {ops}, and none changed the document. base {}\n",
            r.base
        ));
    }
    for op in &r.operations {
        let mut named: Vec<&str> = op.blocks.iter().map(String::as_str).collect();
        named.extend(op.added.iter().map(String::as_str));
        let at = if named.is_empty() {
            String::new()
        } else {
            format!(" {}", named.join(", "))
        };
        // An edit of files says which line the operation came from, which is what the agent wrote.
        let from = op
            .origin
            .as_ref()
            .map(|o| format!(" ({} line {})", o.file, o.line))
            .unwrap_or_default();
        out.push_str(&format!(
            "{} {}{at}{from}: {}\n",
            op.index,
            op.op,
            summary(op, comment_said(r, op), r.dry_run, r)
        ));
    }
    for block in &r.blocks {
        if block.r#ref.is_none() {
            match &block.anchor {
                Some(anchor) => out.push_str(&format!("new, after {anchor}:\n")),
                None => out.push_str("new:\n"),
            }
        }
        out.push_str(block.text.trim_end_matches('\n'));
        out.push('\n');
    }
    if r.blocks_omitted > 0 {
        let n = r.blocks_omitted;
        let word = if n == 1 { "block" } else { "blocks" };
        out.push_str(&format!("and {n} more {word}.\n"));
    }
    tail(r, &mut out);
    out
}

/// The warnings of a receipt, what was retired and what `check` said: the end of every receipt's text.
fn tail(r: &Receipt, out: &mut String) {
    for w in &r.warnings {
        let at = w
            .block
            .as_deref()
            .map(|b| format!(" {b}"))
            .unwrap_or_default();
        out.push_str(&format!("warning {}{at}: {}\n", w.code.as_str(), w.message));
        if let Some(next) = &w.next {
            out.push_str(&format!("  next: {next}\n"));
        }
    }
    if !r.retired.is_empty() {
        let gone: Vec<String> = r
            .retired
            .iter()
            .map(|x| format!("{} ({})", x.r#ref, x.reason))
            .collect();
        out.push_str(&format!("retired: {}\n", gone.join(", ")));
    }
    let v = &r.validation;
    if v.units > 0 {
        let units = plural(v.units as usize, "unit", "units");
        if v.carried_total == 0 {
            out.push_str(&format!("checked {units}.\n"));
        } else {
            // One line, not a list: a problem the input had is the same in every receipt, and the receipt is about what the
            // edit did. A new problem never reaches a receipt, because it refuses the edit (`validation_failed`). The JSON
            // keeps the first ones in `carried`.
            let was = plural(v.carried_total as usize, "problem was", "problems were");
            out.push_str(&format!(
                "checked {units}. {was} already in the input, and no new one was added: `stetdoc check --all` lists them.\n"
            ));
        }
    }
    for f in &v.notes {
        let at = f
            .block
            .as_deref()
            .map(|b| format!(" {b}"))
            .unwrap_or_default();
        out.push_str(&format!("note {}{at}: {}\n", f.code.as_str(), f.message));
    }
}

/// The text of the receipt of a comparison: the revisions by author, the moves, and the warnings.
pub(super) fn compose_compared(r: &Receipt) -> String {
    let mut out = String::new();
    let total: u32 = r
        .revisions
        .iter()
        .map(|a| a.counts.iter().map(|(_, n)| n).sum::<u32>())
        .sum();
    out.push_str(&format!(
        "compared: {}, tracked as {} {}. base {}
",
        plural(total as usize, "revision", "revisions"),
        r.stamp.author,
        r.stamp.date,
        r.base
    ));
    for a in &r.revisions {
        let counts: Vec<String> = a
            .counts
            .iter()
            .map(|(kind, n)| format!("{} {n}", kind_word(*kind)))
            .collect();
        out.push_str(&format!(
            "{}: {}
",
            a.author,
            counts.join(", ")
        ));
    }
    for m in &r.moves {
        out.push_str(&format!(
            "moved {} to {}: {}
",
            m.from, m.to, m.text
        ));
    }
    tail(r, &mut out);
    out
}
