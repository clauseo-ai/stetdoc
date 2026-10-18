//! H14, the part that compares receipts: a dry run is the real edit without its effect.
//!
//! The two receipts are equal field by field after three things are set aside: `dry_run`, the base token (a dry run shows the
//! current one), and the refs of new blocks. A dry run issues no ref, so `added` is empty, a revision in a new block has no
//! `block`, a warning about a new block has no `block` either, and a field whose target is in a new block has no arrow to it
//! (`{field REF "4.2" ->p57}` in the real edit's line is `{field REF "4.2"}` in the dry run's). The warnings about two new blocks that differ only in that ref
//! are then one warning, as the dry run's receipt lists them.

use stetdoc_core::{Receipt, Warning};

/// The warnings of the real edit as a dry run would list them: a warning about a block the batch added has no block, and such a
/// warning that is then the same as one before it is dropped (design H14). Two warnings about a block that existed stay two,
/// as the dry run lists them.
fn as_a_dry_run_lists(warnings: &[Warning], added: &[String]) -> Vec<Warning> {
    let mut out: Vec<Warning> = Vec::new();
    for w in warnings {
        let mut w = w.clone();
        if w.block.as_ref().is_some_and(|b| added.contains(b)) {
            w.block = None;
            if out.contains(&w) {
                continue;
            }
        }
        out.push(w);
    }
    out
}

/// A block's text with its long lines joined again: a physical line that ends in an odd number of backslashes continues on the
/// next, and the reader removes the last backslash and the line feed. The ref at the start of a line moves where a long line
/// breaks, so lines are compared joined.
fn logical(text: &str) -> String {
    let mut out = String::new();
    for line in text.split('\n') {
        let trailing = line.chars().rev().take_while(|c| *c == '\\').count();
        if trailing % 2 == 1 {
            out.push_str(line.get(..line.len() - 1).unwrap_or(line));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Checks that `dry` is what the real edit's receipt `real` is, apart from the three things a dry run changes.
pub fn same_but_for_a_dry_run(what: &str, dry: &Receipt, real: &Receipt) -> Result<(), String> {
    let fail = |field: &str| {
        Err(format!(
            "{what}: the dry run and the real edit differ in {field}"
        ))
    };
    if !dry.dry_run || real.dry_run {
        return fail("dry_run");
    }
    if !dry.added.is_empty() || dry.operations.iter().any(|o| !o.added.is_empty()) {
        return Err(format!("{what}: a dry run issued a ref"));
    }
    if (dry.tracked, &dry.stamp) != (real.tracked, &real.stamp) {
        return fail("the stamp");
    }
    if dry.operations.len() != real.operations.len() {
        return fail("the number of operations");
    }
    // Every ref that the real edit minted, wherever a warning of it names one.
    let minted: Vec<String> = real.added.clone();
    for (d, r) in dry.operations.iter().zip(&real.operations) {
        if (d.index, &d.op, d.changed, &d.blocks, d.added_count)
            != (r.index, &r.op, r.changed, &r.blocks, r.added_count)
        {
            return fail("an operation");
        }
        if d.revisions.len() != r.revisions.len()
            || d.revisions
                .iter()
                .zip(&r.revisions)
                .any(|(a, b)| (a.kind, &a.ids, &a.date) != (b.kind, &b.ids, &b.date))
        {
            return fail("the revisions of an operation");
        }
        if (&d.removed_own, &d.folded, &d.swallowed, &d.resolution)
            != (&r.removed_own, &r.folded, &r.swallowed, &r.resolution)
        {
            return fail("what an operation removed");
        }
        if d.warnings != as_a_dry_run_lists(&r.warnings, &minted) {
            let show = |w: &[stetdoc_core::Warning]| {
                w.iter()
                    .map(|w| format!("{} {:?}: {}", w.code.as_str(), w.block, w.message))
                    .collect::<Vec<_>>()
            };
            return Err(format!(
                "{what}: the dry run and the real edit differ in the warnings of operation {}: {:?} and {:?}",
                d.index,
                show(&d.warnings),
                show(&r.warnings)
            ));
        }
    }
    if dry.revisions.len() != real.revisions.len()
        || dry
            .revisions
            .iter()
            .zip(&real.revisions)
            .any(|(a, b)| (&a.author, &a.counts) != (&b.author, &b.counts))
    {
        return fail("the revisions by author");
    }
    if dry.warnings != as_a_dry_run_lists(&real.warnings, &minted) {
        return fail("the warnings");
    }
    if dry.retired != real.retired || dry.comments != real.comments || dry.moves != real.moves {
        return fail("the retired refs, the comments or the moves");
    }
    if dry.added_count != real.added_count
        || dry.blocks.len() != real.blocks.len()
        || dry.blocks_omitted != real.blocks_omitted
    {
        return fail("the blocks or the count of added blocks");
    }
    for (i, (d, r)) in dry.blocks.iter().zip(&real.blocks).enumerate() {
        // A new block is anchored to the block with a ref before it, and has no anchor only at the start of its story, where
        // the receipt shows no block of that story with a ref before it. Headers and footers, the notes and the comments are
        // several parts under one name, so there the receipt cannot tell, and `dry_anchors` checks against the document.
        let first_of_story = several_parts(&d.story)
            || !dry
                .blocks
                .iter()
                .take(i)
                .any(|b| b.story == d.story && b.r#ref.is_some());
        match (&d.r#ref, &r.r#ref) {
            (Some(a), Some(b)) if a == b => {}
            (None, Some(b)) if minted.contains(b) && (d.anchor.is_some() || first_of_story) => {}
            _ => return fail("the refs of the blocks"),
        }
        // The dry run's line is the real one, without the ref that the real edit minted, as a label or as the target of a field.
        let real_text = match (&d.r#ref, &r.r#ref) {
            (None, Some(b)) => r
                .text
                .strip_prefix(b.as_str())
                .map_or(r.text.as_str(), |t| t.strip_prefix(' ').unwrap_or(t)),
            _ => r.text.as_str(),
        };
        let real_text = without_arrows_to(real_text, &minted);
        let dry_text = match (&d.r#ref, &r.r#ref) {
            (None, Some(_)) => without_label_escape(&d.text, &real_text),
            _ => d.text.clone(),
        };
        if logical(&dry_text) != logical(&real_text) {
            return Err(format!(
                "{what}: the dry run and the real edit differ in the text of a block: {:?} and {:?}",
                d.text, r.text
            ));
        }
    }
    Ok(())
}

/// A dry run's line of a new block, without the escape that only its missing label caused: a line with no ref whose text
/// begins like a ref (`p1`, `t2`, `r3`) has its first character escaped (`\u{70}1`), so that it is not read as a ref,
/// and the real line, which begins with its label, does not need it. `real` is the real line without its label.
fn without_label_escape(dry: &str, real: &str) -> String {
    for (escape, c) in [("\\u{70}", 'p'), ("\\u{74}", 't'), ("\\u{72}", 'r')] {
        if let Some(rest) = dry.strip_prefix(escape)
            && real.starts_with(c)
        {
            return format!("{c}{rest}");
        }
    }
    dry.to_string()
}

/// A line without the target arrows of fields (`{field REF "4.2" ->p57}`) that name one of `minted`: a dry run issues no ref,
/// so it writes such a field with no arrow. An arrow to any other block stays.
fn without_arrows_to(text: &str, minted: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(" ->") {
        let (head, tail) = rest.split_at(at);
        out.push_str(head);
        let after = tail.get(3..).unwrap_or_default();
        let end = after
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(after.len());
        let (target, next) = after.split_at(end);
        if !minted.iter().any(|m| m == target) {
            out.push_str(" ->");
            out.push_str(target);
        }
        rest = next;
    }
    out.push_str(rest);
    out
}

/// The stories that are several parts under one name: headers and footers, the notes, the comments.
fn several_parts(story: &str) -> bool {
    matches!(story, "headers" | "notes" | "comments")
}

/// Checks, against the document after the real edit, every block that a dry run created with no anchor in a story of several
/// parts. A block has no anchor only at the start of its part, so no block that was there before the edit may come before it
/// in its part. The receipt names the story and not the part, so the comparison of the receipts cannot tell. An anchor names
/// the block a new block follows (a paragraph, a table or a row), and is checked there.
pub fn dry_anchors(
    doc: &stetdoc_core::Document,
    dry: &Receipt,
    real: &Receipt,
) -> Result<(), String> {
    let lone: Vec<&String> = dry
        .blocks
        .iter()
        .zip(&real.blocks)
        .filter_map(|(d, r)| match (&d.r#ref, &d.anchor, &r.r#ref) {
            (None, None, Some(made)) if several_parts(&d.story) => Some(made),
            _ => None,
        })
        .collect();
    if lone.is_empty() {
        return Ok(());
    }
    let facts = super::facts::Facts::of(doc)?;
    for made in lone {
        let Some(at) = facts.paras.iter().position(|p| &p.r == made) else {
            continue;
        };
        let part = facts
            .paras
            .get(at)
            .map(|p| p.part.clone())
            .unwrap_or_default();
        if let Some(before) = facts
            .paras
            .iter()
            .take(at)
            .filter(|p| p.part == part)
            .find(|p| !real.added.contains(&p.r))
        {
            return Err(format!(
                "H14: the dry run gives the block that became {made} no anchor, and {} was before it in {part}",
                before.r
            ));
        }
    }
    Ok(())
}

/// The refs the views show: the paragraphs of the body, the notes and the headers, and the tables of the body and their rows.
fn shown_refs(facts: &super::facts::Facts) -> std::collections::BTreeSet<String> {
    let paras = facts.paras.iter().map(|p| p.r.clone());
    let tables = facts.tables.iter().flat_map(|t| {
        std::iter::once(t.r.clone()).chain(t.rows.iter().map(|(row, _)| row.clone()))
    });
    paras.chain(tables).collect()
}

/// H3, for refs: every ref that the document had before the batch and lacks after it went stale, and is in the receipt's
/// `retired`, whatever retired it (a deletion, a rejection, a merge that absorbed its block); and no ref in `retired` is still
/// in the document. `before` is what the views said before the batch.
pub fn stale_refs_are_retired(
    before: &super::facts::Facts,
    doc: &stetdoc_core::Document,
    receipt: &Receipt,
) -> Result<(), String> {
    let (was, is) = (
        shown_refs(before),
        shown_refs(&super::facts::Facts::of(doc)?),
    );
    let retired: std::collections::BTreeSet<&str> =
        receipt.retired.iter().map(|r| r.r#ref.as_str()).collect();
    let unsaid: Vec<&String> = was
        .difference(&is)
        .filter(|r| !retired.contains(r.as_str()))
        .collect();
    if !unsaid.is_empty() {
        return Err(format!(
            "H3: {unsaid:?} went stale, and the receipt's `retired` does not list them (it lists {retired:?})"
        ));
    }
    if let Some(r) = retired.iter().find(|r| is.contains(**r)) {
        return Err(format!(
            "H3: the receipt retires {r}, and the document still has it"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{without_arrows_to, without_label_escape};
    use crate::fixtures::parts::{Doc, entries_doc};
    use crate::fixtures::zip::Zip;
    use crate::laws::edits::{input_codes, run_batch};

    #[test]
    fn an_arrow_to_a_new_block_is_left_out_and_one_to_an_old_block_stays() {
        let minted = vec!["p2".to_string()];
        assert_eq!(
            without_arrows_to(r#"{field ASK "bar"}B{field REF "bar" ->p2}C"#, &minted),
            r#"{field ASK "bar"}B{field REF "bar"}C"#
        );
        assert_eq!(
            without_arrows_to(
                r#"{field REF "x" ->p2 dirty} and {field REF "y" ->p23}"#,
                &minted
            ),
            r#"{field REF "x" dirty} and {field REF "y" ->p23}"#
        );
        assert_eq!(without_arrows_to("a -> b ->p1", &minted), "a -> b ->p1");
    }

    #[test]
    fn only_the_escape_that_the_missing_label_caused_is_undone() {
        assert_eq!(without_label_escape("\\u{70}1 rest", "p1 rest"), "p1 rest");
        assert_eq!(without_label_escape("\\u{74}2", "t2"), "t2");
        // An escape the real line has too, or of another character, stays.
        assert_eq!(without_label_escape("\\u{70}1", "\\u{70}1"), "\\u{70}1");
        assert_eq!(without_label_escape("\\u{a0}x", "\u{a0}x"), "\\u{a0}x");
    }

    #[test]
    fn a_new_paragraph_whose_text_begins_like_a_ref_is_its_dry_run() {
        // DML unique ID compare.docx, seed 18269767483392059150, batch 22: the new paragraph's text begins "p1", so the dry
        // run's line, which has no ref label, escapes its first character, and the real line, labelled, does not.
        for text in ["abc p1 rest", "abc t2", "abc r3"] {
            let body = format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>");
            let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
            let input = input_codes(&bytes).unwrap_or_default();
            for tracked in [true, false] {
                let applied = run_batch(
                    &bytes,
                    &input,
                    r#"{"op":"split","ref":"p1","at":4}"#,
                    tracked,
                );
                assert!(
                    matches!(applied, Ok(Ok(_))),
                    "{text} tracked {tracked}: {:?}",
                    applied.map(|a| a.map(|a| a.receipt.text))
                );
            }
        }
    }

    #[test]
    fn a_split_that_moves_a_reference_target_into_the_new_paragraph_is_its_dry_run() {
        // n751017: an ASK field sets the bookmark that a REF field after it names, both after the first character. Splitting
        // there moves both into the new paragraph, whose ref only the real edit issues, and the REF field's arrow names it.
        let field = |instr: &str, result: &str| {
            format!(
                r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {instr} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{result}<w:r><w:fldChar w:fldCharType="end"/></w:r>"#
            )
        };
        let ask = field(
            r#"ASK foo "What is foo?""#,
            r#"<w:bookmarkStart w:id="0" w:name="foo"/><w:r><w:t>bar</w:t></w:r><w:bookmarkEnd w:id="0"/>"#,
        );
        let reference = field("REF foo", "<w:r><w:t>bar</w:t></w:r>");
        let body = format!(
            "<w:p><w:r><w:t>A</w:t></w:r>{ask}<w:r><w:t>B</w:t></w:r>{reference}<w:r><w:t>C</w:t></w:r></w:p>"
        );
        let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
        let input = input_codes(&bytes).unwrap_or_default();
        for tracked in [true, false] {
            let applied = run_batch(
                &bytes,
                &input,
                r#"{"op":"split","ref":"p1","at":1}"#,
                tracked,
            );
            let receipt = match applied {
                Ok(Ok(applied)) => applied.receipt,
                other => panic!(
                    "tracked {tracked}: {:?}",
                    other.map(|a| a.map(|a| a.receipt.text))
                ),
            };
            assert!(receipt.text.contains("->p2"), "{}", receipt.text);
        }
    }
}
