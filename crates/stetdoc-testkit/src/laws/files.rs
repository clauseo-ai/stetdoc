//! The laws of an edit of readable files through `Document` (design `document-and-ops.md` 9.3, and T1, T6 and T8).
//!
//! For a document that opens: its own files say nothing new (T1, the file form of L1), a line changed in the files is that
//! change in the Final view and nothing else (T6), a line added is a paragraph with a ref of its own, the dry run of each is the
//! real edit's receipt (H14), and rejecting what the author wrote gives the files that were rendered at first (T8's reject-all
//! check and L5). A refusal with a code is no failure: a line in a cell, a field or a read-only region may be refused. What fails
//! is an `internal` error, a phantom edit, and a difference between what was written and what the document says.

use super::receipts::same_but_for_a_dry_run;
use stetdoc_core::{
    Batch, Budget, CheckReport, Class, Document, Edit, ErrorCode, FindingCode, Limits, Mode,
    OpenOptions, ReadableFiles, Scope, Stamp, StoryName, ViewSpec, WarningCode,
};

fn stamp() -> Stamp {
    Stamp::new("Clauseo", "2026-10-06T10:00:00")
}

pub(super) fn open(bytes: &[u8]) -> Result<Document, String> {
    Document::open(bytes.to_vec(), &OpenOptions::default()).map_err(|e| format!("open: {e}"))
}

/// A text of the readable form without the empty paragraph that ends a table cell right after a table in it. A cell must end
/// with a paragraph (Word's rule, `check`'s W9), so a cell whose last block is a table gets an empty one when the paragraph
/// that followed the table goes (a reject of its insertion, an accepted deletion). That paragraph is Word's required form, not
/// content, and reads as no line. It is a later block of the cell that holds the table, so its line carries `{+}`
/// (projection-and-index.md, "A cell"): an empty paragraph of the body after a table, or the first paragraph of the next
/// cell, has no `{+}` and is a line like any other.
pub(super) fn without_cell_end_stubs(text: &str) -> String {
    text.lines()
        .zip(cell_end_stubs(text))
        .filter(|(_, stub)| !stub)
        .map(|(line, _)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// For each line of a text of the readable form, whether it is the empty paragraph that ends a cell right after a table in it,
/// which [`without_cell_end_stubs`] leaves out.
pub(super) fn cell_end_stubs(text: &str) -> Vec<bool> {
    let lines: Vec<&str> = text.lines().collect();
    let continues_cell = |line: &str| leading_tags(line).contains(&"{+}");
    let mut out: Vec<bool> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let after_table = i
            .checked_sub(1)
            .and_then(|j| lines.get(j))
            .is_some_and(|l| l.trim() == "{/table}");
        let ends_cell = lines.get(i + 1).is_some_and(|next| {
            let next = next.trim();
            next == "{/table}"
                || (next.starts_with('r') && next.ends_with("{row}"))
                || (is_block_line(next) && !continues_cell(next))
        });
        let empty = line.starts_with('p') && is_block_line(line) && {
            let tags = leading_tags(line);
            let rest = line.split_once(' ').map_or("", |(_, rest)| rest);
            let shown: usize = tags.iter().map(|t| t.len() + 1).sum();
            tags.contains(&"{+}")
                && tags.iter().all(|t| *t == "{+}" || t.starts_with("{style "))
                && rest
                    .get(shown.min(rest.len())..)
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
        };
        out.push(after_table && ends_cell && empty);
    }
    out
}

/// The tags at the start of a block line, after its ref: `{style "Normal"}`, `{+}` and the others, each whole.
fn leading_tags(line: &str) -> Vec<&str> {
    let mut rest = line.split_once(' ').map_or("", |(_, rest)| rest);
    let mut tags = Vec::new();
    while rest.starts_with('{') {
        // A tag ends at the first `}` outside a quoted value.
        let mut quoted = false;
        let mut end = None;
        for (i, c) in rest.char_indices() {
            match c {
                '"' => quoted = !quoted,
                '}' if !quoted => {
                    end = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        tags.push(rest.get(..=end).unwrap_or_default());
        rest = rest.get(end + 1..).unwrap_or_default().trim_start();
    }
    tags
}

/// The block lines of the whole body in the Final view, over every page.
pub(super) fn final_lines(doc: &Document) -> Result<Vec<String>, String> {
    let mut spec = ViewSpec {
        scope: Scope::Story(StoryName::Body),
        mode: Mode::Final,
        budget: Budget(u32::MAX),
        ..ViewSpec::default()
    };
    let mut out = Vec::new();
    for _ in 0..10_000 {
        let page = doc.view(&spec).map_err(|e| format!("view: {e}"))?;
        out.extend(
            without_cell_end_stubs(&page.text)
                .lines()
                .filter(|l| is_block_line(l))
                .map(str::to_string),
        );
        match page.next {
            Some(next) => spec.after = Some(next),
            None => return Ok(out),
        }
    }
    Err("a view did not end".to_string())
}

/// `p12 text`, `t3 {table}` or `r4 {row}`.
fn is_block_line(line: &str) -> bool {
    let mut chars = line.chars();
    matches!(chars.next(), Some('p' | 't' | 'r'))
        && chars.next().is_some_and(|c| c.is_ascii_digit())
}

/// A block line without the tags that depend on the neighbours and not on the text: the style tag, which the file's common style
/// decides (one more paragraph in another style can change which style is common, and so which lines carry a tag), and `{+}`, which
/// says that a paragraph continues the cell above it, as a line added after a paragraph of a cell does.
pub(super) fn without_tags(line: &str) -> String {
    let Some((r, mut rest)) = line.split_once(' ') else {
        return line.to_string();
    };
    loop {
        if let Some(after) = rest.strip_prefix("{+} ") {
            rest = after;
        } else if let Some(tail) = rest.strip_prefix("{style \"") {
            // The tag of an empty paragraph ends the line.
            match tail.split_once("\"} ") {
                Some((_, after)) => rest = after,
                None if tail.ends_with("\"}") && !tail.contains("\"} ") => return r.to_string(),
                None => break,
            }
        } else {
            break;
        }
    }
    format!("{r} {rest}")
}

/// A paragraph line of plain text: a ref, one space, and words with no marker, escape or continuation.
pub(super) fn plain(line: &str) -> Option<(&str, &str)> {
    let (r, text) = line.split_once(' ')?;
    let is_ref = r
        .strip_prefix('p')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    let clean = !text.contains(['{', '}', '\\', '§']) && text.split_whitespace().count() >= 3;
    (is_ref && clean && text.chars().count() < 1000).then_some((r, text))
}

/// The header of a file names the base token, which differs after an edit and a reject.
pub(super) fn without_base(text: &str) -> String {
    text.lines()
        .map(|l| match l.find(" base=") {
            Some(at) if l.starts_with("{stet ") => {
                let rest = l.get(at + 6..).unwrap_or_default();
                let after = rest
                    .find(' ')
                    .map_or("", |n| rest.get(n..).unwrap_or_default());
                format!("{} base=#{after}", l.get(..at).unwrap_or_default())
            }
            _ => l.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The files, with one line of `document.md` changed.
pub(super) fn replaced(files: &ReadableFiles, line: &str, with: &str) -> ReadableFiles {
    let mut out = files.clone();
    for (name, text) in &mut out.files {
        if name == "document.md" {
            *text = text.replacen(&format!("\n{line}\n"), &format!("\n{with}\n"), 1);
        }
    }
    out
}

/// The codes of the violations in a report, each once. A note is legal: an edit may leave a comment with no anchor, as accepting
/// the deletion of what it was anchored to does, and the notes of that are what `reconcile` of step 6 is for.
fn codes(report: &CheckReport) -> Vec<FindingCode> {
    report
        .by_code
        .iter()
        .map(|(c, _)| *c)
        .filter(|c| c.class() == Class::Violation)
        .collect()
}

fn batch_reject(doc: &Document) -> Result<Batch, String> {
    Batch::parse(
        &format!(
            r#"{{"base":"{}","ops":[{{"op":"reject","author":"Clauseo"}}]}}"#,
            doc.base()
        ),
        &Limits::default(),
    )
    .map_err(|e| e.to_string())
}

/// What one mutation of one line must do. `expected` is the Final lines the document must show afterwards, or `None` when the
/// edit may be refused and then must leave everything as it was.
pub(super) fn one(
    bytes: &[u8],
    what: &str,
    edited: ReadableFiles,
    expected: &dyn Fn(&Receipt) -> Vec<String>,
) -> Result<(), String> {
    let mut doc = open(bytes)?;
    let original = doc.files().map_err(|e| e.to_string())?;
    let before = (
        doc.base(),
        doc.export().map_err(|e| e.to_string())?,
        final_lines(&doc)?,
    );
    let input = codes(&doc.check());
    let dry = doc.edit(Edit::files(edited.clone(), stamp()).dry_run());
    if (doc.base(), doc.export().map_err(|e| e.to_string())?)
        != (before.0.clone(), before.1.clone())
    {
        return Err(format!("{what}: a dry run changed the document"));
    }
    // H13: `check_pending` is that dry run, and changes nothing either.
    let pending = doc.check_pending(Some(edited.clone()), Some(stamp()), false);
    match (&dry, &pending) {
        (Ok(d), Ok(p)) if p.receipt.as_ref() == Some(d) && p.exit_status == 0 => {}
        (Err(d), Err(p))
            if (d.code(), d.file(), d.line(), d.message())
                == (p.code(), p.file(), p.line(), p.message()) => {}
        _ => {
            return Err(format!(
                "{what}: check_pending is not the dry run of the same files"
            ));
        }
    }
    if (doc.base(), doc.export().map_err(|e| e.to_string())?)
        != (before.0.clone(), before.1.clone())
    {
        return Err(format!("{what}: check_pending changed the document"));
    }
    let real = doc.edit(Edit::files(edited, stamp()));
    let receipt = match (dry, real) {
        (Ok(d), Ok(r)) => {
            same_but_for_a_dry_run(what, &d, &r)?;
            r
        }
        (Err(d), Err(r)) => {
            if matches!(r.code(), ErrorCode::Internal | ErrorCode::ValidationFailed) {
                return Err(format!("{what}: {r}"));
            }
            if (d.code(), d.file(), d.line(), d.message())
                != (r.code(), r.file(), r.line(), r.message())
            {
                return Err(format!(
                    "{what}: the dry run failed as {d}, and the real edit as {r}"
                ));
            }
            if (doc.base(), doc.export().map_err(|e| e.to_string())?) != (before.0, before.1) {
                return Err(format!("{what}: a refused edit changed the document"));
            }
            return Ok(());
        }
        (d, r) => {
            return Err(format!(
                "{what}: the dry run gave {:?} and the real edit {:?}",
                d.as_ref().err(),
                r.as_ref().err()
            ));
        }
    };
    // Output validity: nothing in the output that the input did not have.
    if let Some(code) = codes(&doc.check()).into_iter().find(|c| !input.contains(c)) {
        return Err(format!(
            "{what}: the output has a `{}` finding that the input lacked",
            code.as_str()
        ));
    }
    // A paragraph added after one whose mark is hidden takes the mark's formatting, as Word does when Enter is pressed there, so
    // its text is hidden. The receipt then says so (`hidden_text` on the new block), and that is the one way a line can differ from
    // what was written: the lines of such a block are left out of the comparison, and only when the receipt warned.
    let warned: Vec<&str> = receipt
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::HiddenText)
        .filter_map(|w| w.block.as_deref())
        .filter(|b| {
            receipt
                .operations
                .iter()
                .any(|o| o.added.iter().any(|a| a == b))
        })
        .collect();
    let kept = |lines: Vec<String>| -> Vec<String> {
        lines
            .into_iter()
            .filter(|l| !warned.iter().any(|b| l.split(' ').next() == Some(*b)))
            .collect()
    };
    let got: Vec<String> = kept(final_lines(&doc)?.iter().map(|l| without_tags(l)).collect());
    // `?p12` is a paragraph whose line was deleted: it goes, or stays empty when its mark cannot go (a section break, the mark
    // before a table or at the end of a cell or of the body), or stays empty and joins the next one.
    let want: Vec<String> = kept(
        expected(&receipt)
            .iter()
            .filter_map(|l| match l.strip_prefix('?') {
                Some(r) => {
                    // A paragraph that holds only the comment markers hoisted out of the deleted one reads as empty.
                    let stub = format!("{r} {{joins-next}}");
                    got.iter()
                        .find(|g| {
                            let g = super::random::without_comment_marks(g);
                            g == r || g == stub
                        })
                        .cloned()
                }
                None => Some(without_tags(l)),
            })
            .collect(),
    );
    if got != want {
        let at = got
            .iter()
            .zip(&want)
            .position(|(a, b)| a != b)
            .unwrap_or(got.len().min(want.len()));
        return Err(format!(
            "{what}: the Final view after the edit differs from what was written at line {at}: got {:?}, wanted {:?}",
            got.get(at),
            want.get(at)
        ));
    }
    // The files that were rendered again say nothing new.
    let again = doc.files().map_err(|e| e.to_string())?;
    let token = doc.base();
    let nothing = doc
        .edit(Edit::files(again, stamp()))
        .map_err(|e| format!("{what}: the files again: {e}"))?;
    if !nothing.operations.is_empty() || doc.base() != token {
        return Err(format!(
            "{what}: the files rendered after the edit came to {} operations",
            nothing.operations.len()
        ));
    }
    // Rejecting what the author wrote gives the files that were rendered at first.
    if receipt.tracked && !receipt.revisions.is_empty() {
        doc.edit(Edit::ops(batch_reject(&doc)?, stamp()))
            .map_err(|e| format!("{what}: reject: {e}"))?;
        let restored = doc.files().map_err(|e| e.to_string())?;
        let (a, b) = (original.files.iter(), restored.files.iter());
        for ((name, x), (_, y)) in a.zip(b) {
            let (x, y) = (
                without_cell_end_stubs(&without_base(x)),
                without_cell_end_stubs(&without_base(y)),
            );
            if x != y {
                let at = x.lines().zip(y.lines()).position(|(p, q)| p != q);
                return Err(format!(
                    "{what}: rejecting the edit did not restore {name}, first difference at line {at:?}"
                ));
            }
        }
        if original.files.len() != restored.files.len() {
            return Err(format!(
                "{what}: rejecting the edit changed the files that are written"
            ));
        }
    }
    Ok(())
}

use stetdoc_core::Receipt;

/// The laws over one document.
pub fn smoke(bytes: &[u8]) -> Result<(), String> {
    let mut doc = open(bytes)?;
    let files = doc.files().map_err(|e| format!("files: {e}"))?;
    // T1: the files of the document, edited by nobody, come to nothing, and nothing changes.
    let state = (doc.base(), doc.export().map_err(|e| e.to_string())?);
    let receipt = doc
        .edit(Edit::files(files.clone(), stamp()))
        .map_err(|e| format!("files of the document: {e}"))?;
    if !receipt.operations.is_empty()
        || (doc.base(), doc.export().map_err(|e| e.to_string())?) != state
    {
        return Err("the unedited files came to an edit".to_string());
    }
    let body = files
        .files
        .iter()
        .find(|(n, _)| n == "document.md")
        .map(|(_, t)| t.as_str())
        .unwrap_or_default();
    let shown = final_lines(&doc)?;
    let candidates: Vec<(&str, &str)> = body
        .lines()
        .filter_map(plain)
        .filter(|(r, text)| shown.iter().any(|l| l == &format!("{r} {text}")))
        .collect();
    let mut picks: Vec<usize> = vec![0, candidates.len().saturating_sub(1)];
    picks.dedup();
    for i in picks {
        let Some((r, text)) = candidates.get(i) else {
            continue;
        };
        let line = format!("{r} {text}");
        // T6: the last word of a line is changed.
        let (head, _) = text.rsplit_once(' ').unwrap_or((text, ""));
        let new_line = format!("{r} {head} stetdocword");
        let at = shown.iter().position(|l| *l == line).unwrap_or_default();
        let mut want = shown.clone();
        if let Some(slot) = want.get_mut(at) {
            *slot = new_line.clone();
        }
        one(
            bytes,
            &format!("{r}: a word changed"),
            replaced(&files, &line, &new_line),
            &|_| want.clone(),
        )?;
        // A line is added after it, with no ref.
        let added = format!("{line}\nA paragraph that stetdoc added.");
        let inserted = |receipt: &Receipt| -> Vec<String> {
            let mut lines = shown.clone();
            let new_ref = receipt
                .operations
                .first()
                .and_then(|o| o.added.first())
                .cloned()
                .unwrap_or_default();
            lines.insert(at + 1, format!("{new_ref} A paragraph that stetdoc added."));
            lines
        };
        one(
            bytes,
            &format!("{r}: a line added"),
            replaced(&files, &line, &added),
            &inserted,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::without_tags;

    #[test]
    fn the_tags_that_depend_on_the_neighbours_are_left_out_even_on_an_empty_paragraph() {
        assert_eq!(
            without_tags("p2 {style \"Normal\"} Some text"),
            "p2 Some text"
        );
        assert_eq!(without_tags("p2 {+} {style \"Body\"} text"), "p2 text");
        // An empty paragraph has the tag and nothing after it, and the line without the tag is the bare ref.
        assert_eq!(without_tags("p2 {style \"Normal\"}"), "p2");
        assert_eq!(without_tags("p2"), "p2");
        assert_eq!(
            without_tags("p2 {style \"Normal\"} {b}x{/b}"),
            "p2 {b}x{/b}"
        );
    }
}

#[cfg(test)]
mod cell_end_tests {
    use super::without_cell_end_stubs;

    #[test]
    fn an_empty_paragraph_that_ends_a_cell_after_a_table_reads_as_no_line() {
        let text = "t1 {table cols=1}\nr1 {row}\nt2 {table cols=1}\nr2 {row}\np1 x\n{/table}\np9 {style \"Normal\"} {+}\n{/table}";
        assert_eq!(
            without_cell_end_stubs(text),
            "t1 {table cols=1}\nr1 {row}\nt2 {table cols=1}\nr2 {row}\np1 x\n{/table}\n{/table}"
        );
        // A paragraph with text, one that does not end the cell, and one not right after a table stay.
        for kept in [
            "{/table}\np9 {+} words\n{/table}",
            "{/table}\np9 {+}\np10 {+} more\n{/table}",
            "p8 a\np9 {+}\n{/table}",
        ] {
            assert_eq!(without_cell_end_stubs(kept), kept);
        }
        // An empty paragraph of the body after a table, before another paragraph or at the end, is a line: losing or adding
        // one is a change. So is an empty first paragraph of the next cell, which has no `{+}` either.
        for kept in [
            "t1 {table cols=1}\nr1 {row}\np1 a\n{/table}\np2 {style \"Normal\"}\np3 b",
            "t1 {table cols=1}\nr1 {row}\np1 a\n{/table}\np2",
            "t1 {table cols=2}\nr1 {row}\nt2 {table cols=1}\nr2 {row}\np1 a\n{/table}\np2\np3 b\n{/table}",
        ] {
            assert_eq!(without_cell_end_stubs(kept), kept);
        }
    }
}
