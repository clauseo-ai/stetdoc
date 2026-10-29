//! `Receipt.text`: the form an agent reads (design `document-and-ops.md` 4.3).
//!
//! A head line, one line for each operation, the blocks as the readable file will hold them, then the warnings and what `check`
//! carried. A text that would be longer than `TEXT_CAP` lists only the first operations and blocks and says where the rest are.
//! It is deterministic and token-lean, and it lives in this one file so that the format experiments can change its wording
//! without touching the data.

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

/// The most bytes the text of a receipt holds. A receipt that would be longer lists the first operations, shows the first blocks
/// that fit and says how many it left out and where they are, so that it fits in an agent's tool output whatever the size of
/// the batch. The head line, the warnings and what `check` said are always whole. A single block is never cut, so a receipt
/// whose one block is longer than this is longer too. The JSON has every operation and up to 200 blocks.
pub(super) const TEXT_CAP: usize = 11_000;

/// The most bytes the operation lines take in a receipt that is cut, so that the blocks keep most of the room.
const OPS_CAP: usize = 2_000;

/// Room kept for the lines that say what a cut receipt left out.
const LEFT_OUT_ROOM: usize = 600;

/// How many of `items`, from the first, fit in `budget` bytes: at least one, when there is one.
fn fitting(items: &[String], budget: usize) -> usize {
    let mut used = 0;
    let mut n = 0;
    for item in items {
        used += item.len();
        if used > budget && n > 0 {
            break;
        }
        n += 1;
    }
    n
}

/// The head line of a receipt: what was applied, how it was marked, and the base.
fn head(r: &Receipt) -> String {
    let ops = plural(r.operations.len(), "operation", "operations");
    let who = if r.markup.is_manual() {
        "as manual markup: old text kept and struck through, new text underlined, and no tracked changes. Each operation says what it struck and underlined; the text of a block below runs old and new together. Tell the reader this is a manual redline, not Track Changes".to_string()
    } else if r.tracked {
        format!("tracked as {} {}", r.stamp.author, r.stamp.date)
    } else {
        "untracked".to_string()
    };
    if r.dry_run {
        format!(
            "dry run: nothing was applied. A new block has no ref until the edit is applied. {ops}, {who}. base {}\n",
            r.base
        )
    } else if r.operations.iter().any(|o| o.changed) {
        format!("applied {ops}, {who}. base {}\n", r.base)
    } else {
        format!(
            "applied {ops}, and none changed the document. base {}\n",
            r.base
        )
    }
}

/// The line of one operation.
fn op_line(r: &Receipt, op: &OpResult) -> String {
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
    format!(
        "{} {}{at}{from}: {}\n",
        op.index,
        op.op,
        summary(op, comment_said(r, op), r.dry_run, r)
    )
}

/// The lines of one block, as the readable file will hold it.
fn block_lines(block: &crate::api::files::RenderedBlock) -> String {
    let mut out = String::new();
    if block.r#ref.is_none() {
        match &block.anchor {
            Some(anchor) => out.push_str(&format!("new, after {anchor}:\n")),
            None => out.push_str("new:\n"),
        }
    }
    out.push_str(block.text.trim_end_matches('\n'));
    out.push('\n');
    out
}

/// The line that says which operations a cut receipt did not list.
fn ops_left_out(r: &Receipt, shown: usize) -> String {
    let left = r.operations.len().saturating_sub(shown);
    let first = r.operations.get(shown).map_or(0, |o| o.index);
    let last = r.operations.last().map_or(0, |o| o.index);
    let range = if left == 1 {
        format!("operation {first} is")
    } else {
        format!("operations {first} to {last} are")
    };
    format!(
        "and {}, not listed here to keep this short: {range} in the JSON receipt (--json), and `stetdoc view FILE --revisions` lists the revisions in the document, a page at a time.\n",
        plural(left, "more operation", "more operations")
    )
}

/// The line that says how many blocks a receipt did not show, and which command shows them.
fn blocks_left_out(r: &Receipt, shown: usize) -> String {
    let left = r.blocks.len().saturating_sub(shown) + r.blocks_omitted as usize;
    let from = r.blocks.iter().skip(shown).find_map(|b| b.r#ref.as_deref());
    let after = r
        .blocks
        .iter()
        .take(shown)
        .rev()
        .find_map(|b| b.r#ref.as_deref());
    let range = match (from, after) {
        (Some(from), _) => format!(
            ", from {from} on: `stetdoc view FILE --range {from}` shows the blocks from {from}, and"
        ),
        (None, Some(after)) => format!(
            ", after {after}: `stetdoc view FILE --range {after}` shows the blocks from {after}, and"
        ),
        (None, None) => ":".to_string(),
    };
    format!(
        "and {} not shown here{range} `stetdoc view FILE --revisions --context 1` shows every changed block, a page at a time.\n",
        plural(left, "more block", "more blocks")
    )
}

impl Receipt {
    /// The text of the receipt in about `cap` bytes, as `text` is in about 11,000: whole when it fits, and otherwise the
    /// first operations and blocks that fit, with what was left out and where to see it. The head line, the warnings and
    /// the check line are always whole, and a block is never cut, so a text may be longer than a small cap. A surface
    /// whose reader pays for every byte, such as an MCP server, asks for less than `text` holds.
    #[must_use]
    pub fn text_within(&self, cap: usize) -> String {
        if self.text.len() <= cap {
            self.text.clone()
        } else if self.text.starts_with(COMPARED) {
            compose_compared_within(self, cap)
        } else {
            compose_within(self, cap)
        }
    }
}

/// The text of a receipt. Under `TEXT_CAP` it lists every operation and shows every block it holds; over it, the first ones
/// that fit, and says what it left out.
pub(super) fn compose(r: &Receipt) -> String {
    compose_within(r, TEXT_CAP)
}

/// The room for the lines that say what a receipt cut to `cap` left out.
fn left_out_room(cap: usize) -> usize {
    LEFT_OUT_ROOM.min(cap / 4)
}

/// The text of a receipt in about `cap` bytes. The operation lines take at most the share of it that `OPS_CAP` is of
/// `TEXT_CAP`.
fn compose_within(r: &Receipt, cap: usize) -> String {
    let ops_cap = OPS_CAP.saturating_mul(cap) / TEXT_CAP;
    let mut out = head(r);
    let lines: Vec<String> = r.operations.iter().map(|op| op_line(r, op)).collect();
    let blocks: Vec<String> = r.blocks.iter().map(block_lines).collect();
    let mut end = String::new();
    tail(r, &mut end);
    let size = |items: &[String]| items.iter().map(String::len).sum::<usize>();
    let fits = out.len() + size(&lines) + size(&blocks) + end.len() <= cap;
    let ops_shown = if fits {
        lines.len()
    } else {
        fitting(&lines, ops_cap)
    };
    for line in lines.iter().take(ops_shown) {
        out.push_str(line);
    }
    if ops_shown < lines.len() {
        out.push_str(&ops_left_out(r, ops_shown));
    }
    let blocks_shown = if fits {
        blocks.len()
    } else {
        let budget = cap.saturating_sub(out.len() + end.len() + left_out_room(cap));
        fitting(&blocks, budget)
    };
    for block in blocks.iter().take(blocks_shown) {
        out.push_str(block);
    }
    if blocks_shown < blocks.len() || r.blocks_omitted > 0 {
        out.push_str(&blocks_left_out(r, blocks_shown));
    }
    out.push_str(&end);
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
            out.push_str(&format!(
                "validated {units}: no problems, and this file needs no separate check.\n"
            ));
        } else {
            // One line, not a list: a problem the input had is the same in every receipt, and the receipt is about what the
            // edit did. A new problem never reaches a receipt, because it refuses the edit (`validation_failed`). The JSON
            // keeps the first ones in `carried`.
            let was = plural(v.carried_total as usize, "problem was", "problems were");
            out.push_str(&format!(
                "validated {units}: no new problems, and this file needs no separate check. {was} already in the input: `stetdoc check --all` lists them.\n"
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
    compose_compared_within(r, TEXT_CAP)
}

/// How the text of the receipt of a comparison starts.
const COMPARED: &str = "compared: ";

/// The text of the receipt of a comparison in about `cap` bytes.
fn compose_compared_within(r: &Receipt, cap: usize) -> String {
    let mut out = String::new();
    let total: u32 = r
        .revisions
        .iter()
        .map(|a| a.counts.iter().map(|(_, n)| n).sum::<u32>())
        .sum();
    out.push_str(&format!(
        "{COMPARED}{}, tracked as {} {}. base {}
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
    let moves: Vec<String> = r
        .moves
        .iter()
        .map(|m| format!("moved {} to {}: {}\n", m.from, m.to, m.text))
        .collect();
    let mut end = String::new();
    tail(r, &mut end);
    let budget = cap.saturating_sub(out.len() + end.len() + left_out_room(cap));
    let shown = fitting(&moves, budget);
    for line in moves.iter().take(shown) {
        out.push_str(line);
    }
    if shown < moves.len() {
        out.push_str(&format!(
            "and {} not listed here to keep this short: the JSON receipt (--json) has every one, and `stetdoc view FILE --revisions` lists them, a page at a time.\n",
            plural(moves.len() - shown, "more move", "more moves")
        ));
    }
    out.push_str(&end);
    out
}
