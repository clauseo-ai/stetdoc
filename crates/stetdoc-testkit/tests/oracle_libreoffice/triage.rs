//! The triage of the LibreOffice oracle's mismatches, in four steps, each of which names what it found:
//!
//! 1. **The input reads differently too.** The output's corpus document (for a redline, the revised document for the Final side
//!    and the original for the Original side) is read by LibreOffice and by stetdoc. When the input has the same difference in
//!    the same section (the same first difference, or one on the same line), the edit did not bring it: it is LibreOffice's
//!    reading of that document, or stetdoc's view of it, and step 3 or [`KNOWN`] says which. A document whose input differs in
//!    the section only at another place (the edit moved the first difference) is named apart, after steps 2 and 3.
//! 2. **Ablation.** The output is read again with every revision given its own date, and again without comment ranges. When
//!    the mismatch is gone in one, it is a LibreOffice deviation that the minimal documents in the report reproduce: LibreOffice
//!    joins adjacent revisions of one author whose dates are a minute or less apart into one (and so takes an untracked paragraph
//!    mark between them with it), and it mishandles accepting and rejecting changes inside a comment range that spans
//!    paragraphs. A third ablation moves a note's own number out of a deletion: when that makes the mismatch go away, stetdoc's
//!    deletion of the number is the suspect.
//! 3. **Text rules.** A difference in white space only where the input has a `w:t` without `xml:space="preserve"` with white
//!    space at that place (`white_space`), and a paragraph LibreOffice joins to the next because it is a drop cap.
//! 4. **Known documents.** What inspection of the XML found, by document, and for one footnote of a document.

use super::{Verdict, mismatch, verdict};
use std::collections::BTreeMap;
use std::path::PathBuf;
use stetdoc_testkit::laws::libreoffice::{Output, text_of};
use stetdoc_testkit::oracle::libreoffice::{
    Reading, Variant, compare, normalize_all, read_all, read_variants,
};

/// What inspection of the XML found for documents whose input already reads differently: a piece of the document's id, and the
/// triage. A LibreOffice deviation starts with `LibreOffice`, a limit of stetdoc's that FIDELITY.md documents with `settled`, and a
/// stetdoc suspect, which fails the run, with `stetdoc suspect`.
pub const KNOWN: &[(&str, &str)] = &[
    (
        "preexisting-moves",
        "LibreOffice: loses a paragraph that a block-level w:moveFrom and w:moveTo wrap",
    ),
    (
        "RA001-Tracked-Revisions-02",
        "LibreOffice: reads the moves (w:moveFrom, w:moveTo) around a floating table in this document differently",
    ),
    (
        "n830205",
        "LibreOffice: keeps text that is deleted with a formatting change on the same run (w:rPrChange in a w:del) when accepting all",
    ),
    (
        "Comment042",
        "LibreOffice: keeps text that is deleted with a formatting change on the same run (w:rPrChange in a w:del) when accepting all",
    ),
    (
        "WC036",
        "LibreOffice: drops a table at the start of an endnote, and with it the note's text",
    ),
    (
        "raw-ooxml",
        "LibreOffice: makes a drop cap (a paragraph with frame properties) part of the next paragraph",
    ),
    (
        "tdf111550",
        "settled: stetdoc's views leave out a table placed directly in a paragraph (w:p/w:tbl, which the schema does not allow), and edits cannot reach it (FIDELITY.md)",
    ),
    (
        "Corrupted footnotes.docx",
        "LibreOffice: makes a note of its own for each of several references to one note (this corrupt input refers to one footnote five times), where stetdoc lists the note once",
    ),
    (
        "markup_compatibility_choice",
        "LibreOffice: reads the mc:Fallback of an mc:Choice that requires w15, where Word and stetdoc take the Choice",
    ),
];

/// What inspection found for one footnote of one document: a piece of the document's id, the start of the stetdoc side of a
/// first difference in the notes, and the triage. It names that footnote only, not the document.
pub const KNOWN_IN_NOTES: &[(&str, &str, &str)] = &[(
    "word-page-fields",
    "PF8 ",
    "LibreOffice: corrupts a footnote long enough to run onto the next page when it accepts or rejects a change in it (its last paragraphs run together and lose characters), where the same change in a short footnote resolves as it should",
)];

/// What inspection found for this output, by its document (`KNOWN`) or by its document and a footnote (`KNOWN_IN_NOTES`).
fn known(o: &Output, v: &Verdict) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|(id, _)| o.id.contains(id))
        .map(|(_, t)| *t)
        .or_else(|| {
            let notes = v
                .notes
                .as_deref()
                .filter(|_| v.accepted.is_none() && v.rejected.is_none())?;
            let (s, _) = sides(notes)?;
            KNOWN_IN_NOTES
                .iter()
                .find(|(id, start, _)| o.id.contains(id) && s.starts_with(start))
                .map(|(_, _, t)| *t)
        })
}

/// The label of a LibreOffice deviation found by ablation.
pub const SAME_DATE: &str = "LibreOffice: joins adjacent revisions of one author whose dates are a minute or less apart, and the untracked paragraph mark between them";
/// The label of a stetdoc suspect found by ablation.
pub const NOTE_MARK: &str = "stetdoc suspect: a deletion of a note's whole paragraph takes the note's own number (w:footnoteRef, w:endnoteRef) with it, and LibreOffice then ignores the deletion";
/// The label of a LibreOffice deviation found by ablation.
pub const COMMENT_RANGE: &str =
    "LibreOffice: accepts and rejects wrongly inside a comment range that spans tracked paragraphs";

fn line_of(e: &str) -> Option<&str> {
    e.split_once(": ").map(|(l, _)| l)
}

fn strip_line(e: &str) -> &str {
    e.split_once(": ").map_or(e, |(_, rest)| rest)
}

/// Triage of every mismatch, in place.
pub fn all(outputs: &[Output], paths: &[PathBuf], readings: &[Reading], verdicts: &mut [Verdict]) {
    let todo: Vec<usize> = (0..outputs.len())
        .filter(|i| mismatch(&verdicts[*i]))
        .collect();
    // Step 1: the inputs.
    let mut inputs: Vec<PathBuf> = todo
        .iter()
        .flat_map(|i| outputs[*i].inputs.iter().cloned())
        .collect();
    inputs.sort();
    inputs.dedup();
    let read = read_all(&inputs).unwrap().unwrap();
    let of_input: BTreeMap<PathBuf, Verdict> = inputs
        .iter()
        .zip(&read)
        .map(|(p, r)| (p.clone(), verdict(&text_of(&std::fs::read(p).unwrap()), r)))
        .collect();
    for i in &todo {
        let o = &outputs[*i];
        let v = &verdicts[*i];
        let (fin, orig) = (&of_input[&o.inputs[0]], &of_input[&o.inputs[1]]);
        // The same first difference, or a first difference on the same line (the edit may have changed its text).
        let same = |out: &Option<String>, inp: &Option<String>| match (out, inp) {
            (None, _) => true,
            (Some(a), Some(b)) => strip_line(a) == strip_line(b) || line_of(a) == line_of(b),
            (Some(_), None) => false,
        };
        let notes_same =
            v.notes.is_none() || same(&v.notes, &fin.notes) || same(&v.notes, &orig.notes);
        if same(&v.accepted, &fin.accepted) && same(&v.rejected, &orig.rejected) && notes_same {
            let known = known(o, v);
            let rule = rules(v, o);
            verdicts[*i].triage = Some(format!(
                "the input reads differently too: {}",
                known.or(rule).unwrap_or("not yet explained")
            ));
        }
    }
    // Step 2: ablation of what is left.
    let left: Vec<usize> = todo
        .iter()
        .copied()
        .filter(|i| verdicts[*i].triage.is_none())
        .collect();
    for (variant, label) in [
        (Variant::UniqueDates, SAME_DATE),
        (Variant::NoCommentRanges, COMMENT_RANGE),
        (Variant::NoteMarksKept, NOTE_MARK),
    ] {
        let jobs: Vec<(PathBuf, Option<Variant>)> = left
            .iter()
            .filter(|i| verdicts[**i].triage.is_none())
            .map(|i| (paths[*i].clone(), Some(variant)))
            .collect();
        let idx: Vec<usize> = left
            .iter()
            .copied()
            .filter(|i| verdicts[*i].triage.is_none())
            .collect();
        if jobs.is_empty() {
            continue;
        }
        let read = read_variants(&jobs).unwrap().unwrap();
        for (i, r) in idx.iter().zip(&read) {
            let again = verdict(&text_of(&outputs[*i].bytes), r);
            if again.load && !mismatch(&again) {
                verdicts[*i].triage = Some(label.to_string());
            } else {
                let first = again.accepted.or(again.rejected).or(again.notes);
                verdicts[*i].after.push(format!(
                    "{}: {}",
                    variant.name(),
                    first.unwrap_or_else(|| "does not load".into())
                ));
            }
        }
    }
    // Steps 3 and 4 for the rest.
    for i in todo {
        let v = &verdicts[i];
        if v.triage.is_none()
            && v.accepted.is_none()
            && v.rejected.is_none()
            && notes_dropped(&outputs[i], &readings[i])
        {
            verdicts[i].triage = Some(NOTES_DROPPED.to_string());
            continue;
        }
        if verdicts[i].triage.is_some() {
            continue;
        }
        let o = &outputs[i];
        let v = &verdicts[i];
        let known = known(o, v).or_else(|| rules(v, o));
        // The input differs in the same section, at another place: a document LibreOffice reads differently anyway (moves,
        // floating tables), where the first difference moved with the edit.
        let (fin, orig) = (&of_input[&o.inputs[0]], &of_input[&o.inputs[1]]);
        let elsewhere = (v.accepted.is_none() || fin.accepted.is_some())
            && (v.rejected.is_none() || orig.rejected.is_some())
            && (v.notes.is_none() || fin.notes.is_some() || orig.notes.is_some());
        verdicts[i].triage = match (known, elsewhere) {
            (_, true) => Some(format!(
                "the input reads differently too, at another place: {}",
                known.unwrap_or("not yet explained")
            )),
            (Some(k), false) => Some(k.to_string()),
            (None, false) => None,
        };
    }
}

/// The label of a stetdoc suspect in its views of the notes.
pub const NOTES_DROPPED: &str = "stetdoc suspect: the Final (or Original) view of the notes keeps a note whose reference is a pending deletion (or insertion), which accepting (or rejecting) takes out";

/// Whether LibreOffice's notes are stetdoc's with some notes left out, on a side where they differ.
fn notes_dropped(o: &Output, r: &Reading) -> bool {
    let Ok(t) = text_of(&o.bytes) else {
        return false;
    };
    let subsequence = |s: &[String], l: &[String]| {
        let mut it = s.iter();
        l.len() < s.len() && l.iter().all(|x| it.any(|y| y == x))
    };
    let sides = [
        (&t.final_notes, normalize_all(&r.notes_accept)),
        (&t.original_notes, normalize_all(&r.notes_reject)),
    ];
    sides.iter().any(|(s, l)| compare(s, l).is_err())
        && sides
            .iter()
            .all(|(s, l)| compare(s, l).is_ok() || subsequence(s, l))
}

/// The text rules: what a first difference says, with the input it comes from. A difference in white space only is LibreOffice's
/// only where the input has a `w:t` without `preserve` with white space at that place (`white_space::edge_space_at`).
fn rules(v: &Verdict, o: &Output) -> Option<&'static str> {
    let (first, inputs): (&str, Vec<&std::path::Path>) = match (&v.accepted, &v.rejected, &v.notes)
    {
        (Some(a), _, _) => (a, vec![o.inputs[0].as_path()]),
        (None, Some(r), _) => (r, vec![o.inputs[1].as_path()]),
        (None, None, Some(n)) => (n, o.inputs.iter().map(|p| p.as_path()).collect()),
        _ => return None,
    };
    let (s, l) = sides(first)?;
    let squeeze = |x: &str| x.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    if squeeze(&s) == squeeze(&l)
        && inputs
            .iter()
            .any(|p| super::white_space::edge_space_at(p, &s, &l))
    {
        return Some(
            "LibreOffice: drops the white space at the ends of a w:t without xml:space=\"preserve\", which Word shows (hand check 37)",
        );
    }
    if s.chars().count() <= 2 && l.starts_with(&s) && l.chars().count() > 2 {
        return Some(
            "LibreOffice: makes a drop cap (a paragraph with frame properties) part of the next paragraph",
        );
    }
    None
}

/// The two texts of a first difference, as `compare` writes it: `stetdoc "...", LibreOffice "..."`.
fn sides(e: &str) -> Option<(String, String)> {
    let s = e.split_once("stetdoc \"")?.1;
    let (s, rest) = s.split_once("\", LibreOffice \"")?;
    let l = rest.strip_suffix('"')?;
    let clean = |x: &str| {
        x.trim_start_matches("...")
            .trim_end_matches("...")
            .to_string()
    };
    Some((clean(s), clean(l)))
}
