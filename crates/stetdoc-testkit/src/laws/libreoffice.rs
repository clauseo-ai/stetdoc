//! The LibreOffice oracle over stetdoc's outputs: a seeded sample of what stetdoc writes from the curated corpus, and
//! stetdoc's own text of each, for `oracle::libreoffice` to hold LibreOffice's reading against.
//!
//! [`sample`] makes `n` outputs from a seed, the same ones for the same seed and corpus, stratified by kind and by family:
//!
//! - **tracked** random batches of the generator of `laws::generate`, in six equal groups by what they edit: text (`replace`,
//!   `insert`, `delete`), formatting (`format`, `style`, `paragraph`, `list`, `set`), paragraphs (`split`, `merge`), tables
//!   (`row`, and text edits of paragraphs in cells), comments (`comment`, `reply`, `resolve`, `edit-comment`,
//!   `delete-comment`) and notes (`delete-note`, and text edits of paragraphs in notes);
//! - **untracked** random batches of any operation;
//! - **accepted** and **rejected** results: `accept` or `reject` with `all`, on a corpus document with tracked changes, or on
//!   one that a tracked random batch of another author changed first;
//! - **compared** redlines of the curated compare pairs (`corpus/pairs.tsv`).
//!
//! The document-based kinds take the corpus tiers in turn (lab, word-authored, other-producers, public, edge), and the redlines
//! the pair classes in turn. Each output says its kind, family, document, seed and batch, so that a mismatch can be made again.
//! [`text_of`] is stetdoc's side of the comparison: its Final and Original views of the body and the notes, as lines in the
//! form `oracle::libreoffice::compare` takes.

use super::generate::{Facts, Rng, fnv, operation};
use crate::corpus::{Corpus, Entry};
use crate::oracle::libreoffice::{FIELD, OPAQUE, normalize};
use crate::pairs::Resolved;
use serde_json::Value;
use stetdoc_core::{
    Batch, Budget, CompareOptions, Document, Edit, FormatId, Limits, Mode, OpenOptions,
    RevisionFilter, Scope, Stamp, StoryName, ViewSpec, compare,
};

/// The families of the document-based kinds, taken in turn.
pub const TIERS: [&str; 5] = ["lab", "word-authored", "other-producers", "public", "edge"];

/// The groups of the tracked batches, with the operations of each.
pub const GROUPS: [(&str, &[&str]); 6] = [
    ("text", &["replace", "insert", "delete"]),
    (
        "formatting",
        &["format", "style", "paragraph", "list", "set"],
    ),
    ("paragraphs", &["split", "merge"]),
    ("tables", &["row", "replace", "insert", "delete"]),
    (
        "comments",
        &[
            "comment",
            "reply",
            "resolve",
            "edit-comment",
            "delete-comment",
        ],
    ),
    ("notes", &["delete-note", "replace", "insert", "delete"]),
];

/// One output of stetdoc in the sample.
#[derive(Clone, Debug)]
pub struct Output {
    /// `tracked/<group>`, `untracked`, `accepted`, `rejected` or `compared`.
    pub kind: String,
    /// The corpus tier of the document, or the class of the pair.
    pub family: String,
    /// The id of the document, or the label of the pair.
    pub id: String,
    /// The seed of this output's random stream.
    pub seed: u64,
    /// What was done: the batch as JSON, with its author and whether it was tracked.
    pub batch: String,
    /// The saved package.
    pub bytes: Vec<u8>,
    /// The inputs its Final and its Original text come from: the corpus document twice, or a pair's revised and original
    /// documents. Triage reads them to tell a difference the output brought from one that LibreOffice has on the input.
    pub inputs: [std::path::PathBuf; 2],
}

/// How many outputs of each kind a sample of `n` has: three fifths tracked (in six equal groups), and then untracked,
/// accepted, rejected and compared.
pub fn quotas(n: usize) -> Vec<(String, usize)> {
    let tracked = n * 3 / 5;
    let untracked = n * 3 / 25;
    let accepted = n * 7 / 100;
    let rejected = accepted;
    let mut out: Vec<(String, usize)> = GROUPS
        .iter()
        .enumerate()
        .map(|(i, (g, _))| {
            (
                format!("tracked/{g}"),
                tracked / 6 + usize::from(i < tracked % 6),
            )
        })
        .collect();
    out.push(("untracked".into(), untracked));
    out.push(("accepted".into(), accepted));
    out.push(("rejected".into(), rejected));
    let compared = n - out.iter().map(|(_, k)| k).sum::<usize>();
    out.push(("compared".into(), compared));
    out
}

fn stamp(author: &str) -> Stamp {
    Stamp::new(author, "2026-10-09T10:00:00")
}

fn open(bytes: &[u8]) -> Option<Document> {
    Document::open(bytes.to_vec(), &OpenOptions::default()).ok()
}

/// Applies `ops` to the document and saves it, or `None` when the batch is refused.
fn apply(doc: &mut Document, ops: &str, author: &str, tracked: bool) -> Option<Vec<u8>> {
    let b = Batch::parse(
        &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
        &Limits::default(),
    )
    .ok()?;
    let edit = Edit::ops(b, stamp(author));
    let edit = if tracked { edit } else { edit.untracked() };
    doc.edit(edit).ok()?;
    doc.save().ok().map(|s| s.bytes)
}

/// A random batch of one to three operations from `names`, on the paragraphs `keep` allows, without those made to be refused.
fn batch_of(
    rng: &mut Rng,
    facts: &Facts,
    names: &[&'static str],
    keep: impl Fn(&super::facts::Para) -> bool,
) -> Option<String> {
    let mut facts = facts.clone();
    facts.paras.retain(|p| keep(p));
    if facts.paras.is_empty() {
        return None;
    }
    let ops: Vec<String> = (0..1 + rng.below(3))
        .map(|_| {
            let name = *rng.pick(names).unwrap_or(&"replace");
            operation(rng, &facts, Some(name))
        })
        .filter(|o| o.refused.is_none())
        .map(|o| o.json)
        .collect();
    (!ops.is_empty()).then(|| ops.join(","))
}

fn has_revisions(doc: &Document) -> bool {
    doc.revisions(&RevisionFilter::default())
        .is_ok_and(|r| !r.is_empty())
}

/// One output of a document-based kind on `entry`, or `None` when this try made none (the batch was refused, or the document
/// has nothing for the group).
fn on_document(kind: &str, entry: &Entry, rng: &mut Rng) -> Option<(String, Vec<u8>)> {
    let bytes = entry.bytes();
    let mut doc = open(&bytes)?;
    let facts = Facts::of(&doc).ok()?;
    if let Some(group) = kind.strip_prefix("tracked/") {
        let (_, names) = GROUPS.iter().find(|(g, _)| *g == group)?;
        let ops = match group {
            "tables" => batch_of(rng, &facts, names, |p| p.in_table && p.story == "body"),
            "notes" => batch_of(rng, &facts, names, |p| p.story == "notes"),
            _ => batch_of(rng, &facts, names, |_| true),
        }?;
        let saved = apply(&mut doc, &ops, "Clauseo", true)?;
        return Some((format!("[{ops}] by Clauseo, tracked"), saved));
    }
    if kind == "untracked" {
        let names: Vec<&'static str> = GROUPS.iter().flat_map(|(_, n)| n.iter().copied()).collect();
        let ops = batch_of(rng, &facts, &names, |_| true)?;
        let saved = apply(&mut doc, &ops, "Clauseo", false)?;
        return Some((format!("[{ops}] by Clauseo, untracked"), saved));
    }
    let action = if kind == "accepted" {
        "accept"
    } else {
        "reject"
    };
    let review = format!(r#"{{"op":"{action}","all":true}}"#);
    // Half the time on the document's own tracked changes, when it has some; else after a tracked batch of another author.
    if has_revisions(&doc) && rng.chance(50) {
        let saved = apply(&mut doc, &review, "Clauseo", true)?;
        return Some((format!("[{review}] by Clauseo"), saved));
    }
    let ops = batch_of(
        rng,
        &facts,
        &["replace", "insert", "delete", "split", "merge", "format"],
        |_| true,
    )?;
    apply(&mut doc, &ops, "Reviewer", true)?;
    let saved = apply(&mut doc, &review, "Clauseo", true)?;
    Some((
        format!("[{ops}] by Reviewer, tracked; then [{review}] by Clauseo"),
        saved,
    ))
}

/// The redline of a pair, or `None` when `compare` refuses it.
fn on_pair(pair: &Resolved) -> Option<Vec<u8>> {
    let options = CompareOptions {
        limits: Limits::default(),
        stamp: stamp("Clauseo"),
        format: FormatId::stet(),
    };
    let compared = compare(pair.original.bytes(), pair.revised.bytes(), &options).ok()?;
    compared.document.save().ok().map(|s| s.bytes)
}

/// A seeded sample of `n` outputs of stetdoc from the corpus and the pairs. The same seed, corpus and pairs give the same
/// sample. A kind that cannot fill its quota (too few pairs that compare, say) leaves it short, and the caller sees the count.
pub fn sample(corpus: &Corpus, pairs: &[Resolved], seed: u64, n: usize) -> Vec<Output> {
    let by_tier: Vec<(&str, Vec<&Entry>)> = TIERS
        .iter()
        .map(|t| {
            (
                *t,
                corpus
                    .expected_ok()
                    .filter(|e| e.tier == *t)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, e)| !e.is_empty())
        .collect();
    let mut classes: Vec<&str> = pairs.iter().map(|p| p.pair.class.as_str()).collect();
    classes.sort_unstable();
    classes.dedup();
    let mut out = Vec::new();
    for (kind, quota) in quotas(n) {
        let mut made = 0;
        let mut k: u64 = 0;
        let mut used_pairs: Vec<usize> = Vec::new();
        // Tries that made nothing in this stratum: after eight, the next family takes the slot, so that a family with nothing
        // for the kind (no notes in its documents, no pairs left in its class) does not hold the sample up.
        let mut fails = 0;
        // Each try has its own stream, so that one output can be made again from its seed alone.
        while made < quota && k < (quota as u64) * 40 + 40 {
            let item_seed = seed ^ fnv(kind.as_bytes()) ^ k.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            let slot = made + fails / 8;
            k += 1;
            let mut rng = Rng::new(item_seed);
            if kind == "compared" {
                let Some(class) = classes.get(slot % classes.len().max(1)) else {
                    break;
                };
                let of_class: Vec<usize> = (0..pairs.len())
                    .filter(|i| pairs[*i].pair.class == *class && !used_pairs.contains(i))
                    .collect();
                let Some(&i) = rng.pick(&of_class) else {
                    fails += 8;
                    continue;
                };
                used_pairs.push(i);
                let pair = &pairs[i];
                if let Some(bytes) = on_pair(pair) {
                    out.push(Output {
                        kind: kind.clone(),
                        family: pair.pair.class.clone(),
                        id: pair.pair.label.clone(),
                        seed: item_seed,
                        batch: "compare".into(),
                        bytes,
                        inputs: [pair.revised.path.clone(), pair.original.path.clone()],
                    });
                    made += 1;
                    fails = 0;
                } else {
                    fails += 1;
                }
                continue;
            }
            let Some((tier, entries)) = by_tier.get(slot % by_tier.len().max(1)) else {
                break;
            };
            let Some(entry) = rng.pick(entries) else {
                fails += 1;
                continue;
            };
            if let Some((batch, bytes)) = on_document(&kind, entry, &mut rng) {
                out.push(Output {
                    kind: kind.clone(),
                    family: (*tier).to_string(),
                    id: entry.id.clone(),
                    seed: item_seed,
                    batch,
                    bytes,
                    inputs: [entry.path.clone(), entry.path.clone()],
                });
                made += 1;
                fails = 0;
            } else {
                fails += 1;
            }
        }
    }
    out
}

/// Remakes the output of one seed: the kind, the document and the seed that a report names. For a redline, `id` is the pair's
/// label.
pub fn remake(kind: &str, entry: &Entry, seed: u64) -> Option<(String, Vec<u8>)> {
    on_document(kind, entry, &mut Rng::new(seed))
}

/// stetdoc's text of a document, as lines for `oracle::libreoffice::compare`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Text {
    /// The body in the Final view.
    pub final_body: Vec<String>,
    /// The notes in the Final view.
    pub final_notes: Vec<String>,
    /// The body in the Original view.
    pub original_body: Vec<String>,
    /// The notes in the Original view.
    pub original_notes: Vec<String>,
}

/// stetdoc's Final and Original text of a saved package, opened afresh.
pub fn text_of(bytes: &[u8]) -> Result<Text, String> {
    let doc = Document::open(bytes.to_vec(), &OpenOptions::default())
        .map_err(|e| format!("open: {e}"))?;
    // Each view numbers the notes as Word does once its changes are resolved, so the references are read in the same mode.
    let final_refs = referenced_notes(&doc, Mode::Final).unwrap_or_default();
    let original_refs = referenced_notes(&doc, Mode::Original).unwrap_or_default();
    Ok(Text {
        final_body: lines(&doc, StoryName::Body, Mode::Final, &final_refs)?,
        final_notes: lines(&doc, StoryName::Notes, Mode::Final, &final_refs).unwrap_or_default(),
        original_body: lines(&doc, StoryName::Body, Mode::Original, &original_refs)?,
        original_notes: lines(&doc, StoryName::Notes, Mode::Original, &original_refs)
            .unwrap_or_default(),
    })
}

/// The notes that a reference shown in the body's view of `mode` points to, by kind and the ordinal of that mode. A note
/// that none points to has no lines: Word and LibreOffice show no note without a reference.
fn referenced_notes(doc: &Document, mode: Mode) -> Result<Vec<(String, u64)>, String> {
    let page = doc
        .view(&ViewSpec {
            scope: Scope::Story(StoryName::Body),
            mode,
            budget: Budget(u32::MAX),
            format: FormatId::new(FormatId::JSON),
            ..ViewSpec::default()
        })
        .map_err(|e| format!("view: {e}"))?;
    let v: Value = serde_json::from_str(&page.text).map_err(|e| format!("json/1: {e}"))?;
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<(String, u64)>) {
        match v {
            Value::Object(m) => {
                if let Some(r) = m.get("note_ref") {
                    let kind = r.get("kind").and_then(Value::as_str).unwrap_or_default();
                    let n = r.get("ordinal").and_then(Value::as_u64).unwrap_or_default();
                    out.push((kind.to_string(), n));
                }
                m.values().for_each(|x| walk(x, out));
            }
            Value::Array(a) => a.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    walk(&v, &mut out);
    Ok(out)
}

/// The paragraphs of a story in a mode, normalized, without the empty ones, each field as [`FIELD`].
fn lines(
    doc: &Document,
    story: StoryName,
    mode: Mode,
    refs: &[(String, u64)],
) -> Result<Vec<String>, String> {
    let mut spec = ViewSpec {
        scope: Scope::Story(story),
        mode,
        budget: Budget(u32::MAX),
        format: FormatId::new(FormatId::JSON),
        ..ViewSpec::default()
    };
    let mut paras: Vec<(String, bool)> = Vec::new();
    for _ in 0..10_000 {
        let page = doc.view(&spec).map_err(|e| format!("view: {e}"))?;
        let v: Value = serde_json::from_str(&page.text).map_err(|e| format!("json/1: {e}"))?;
        for s in v
            .pointer("/body/stories")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for unit in s
                .get("units")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let kind = unit.get("kind").and_then(Value::as_str).unwrap_or_default();
                let ordinal = unit
                    .get("ordinal")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                if matches!(kind, "footnote" | "endnote")
                    && !refs.iter().any(|(k, n)| k == kind && *n == ordinal)
                {
                    continue;
                }
                blocks(unit.get("blocks"), &mut paras, story == StoryName::Body);
            }
        }
        match page.next {
            Some(next) => spec.after = Some(next),
            None => {
                // A paragraph whose mark is deleted joins the next one in the Final view, as accepting makes it.
                let mut out: Vec<String> = Vec::new();
                let mut join = false;
                for (text, joins) in paras {
                    let pieces: Vec<&str> = text.split('\n').collect();
                    for (i, piece) in pieces.iter().enumerate() {
                        match out.last_mut() {
                            Some(last) if join && i == 0 => last.push_str(piece),
                            _ => out.push((*piece).to_string()),
                        }
                    }
                    join = joins;
                }
                return Ok(out
                    .iter()
                    .map(|l| normalize(l))
                    .filter(|l| !l.is_empty())
                    .collect());
            }
        }
    }
    Err("a view did not end".to_string())
}

/// The paragraphs of blocks, a table's cells row by row; each paragraph's text has `\n` where a page or column break ends a
/// line, and `\u{2028}` for a line break inside it.
fn blocks(blocks: Option<&Value>, out: &mut Vec<(String, bool)>, body: bool) {
    for b in blocks.and_then(Value::as_array).into_iter().flatten() {
        if let Some(p) = b.get("paragraph") {
            let mut text = String::new();
            inlines(p.get("inlines"), &mut text, body);
            let joins = p
                .pointer("/flags/joins_next")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            out.push((text, joins));
        }
        if let Some(t) = b.get("table") {
            for row in t
                .get("rows")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                for cell in row
                    .get("cells")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    // A cell that continues a vertical merge is drawn as part of the cell above, with that cell's text.
                    if cell.get("v_merge").and_then(Value::as_str) == Some("continue") {
                        continue;
                    }
                    self::blocks(cell.get("blocks"), out, body);
                }
            }
        }
        if let Some(o) = b.get("opaque") {
            // Content the view collapses (a table of contents): its blocks when the view gives them, else a marker that
            // stands for any lines.
            match o.get("blocks").and_then(Value::as_array) {
                Some(inner) if !inner.is_empty() => self::blocks(o.get("blocks"), out, body),
                _ => out.push((OPAQUE.to_string(), false)),
            }
        }
    }
}

fn inlines(inlines: Option<&Value>, out: &mut String, body: bool) {
    for i in inlines.and_then(Value::as_array).into_iter().flatten() {
        if i.get("field").is_some() {
            out.push(FIELD);
        } else if let Some(atom) = i.get("atom") {
            // LibreOffice splits a body paragraph at a page or column break; in a note it keeps one paragraph.
            if body && matches!(atom.as_str(), Some("page_break" | "column_break")) {
                out.push('\n');
            } else if atom.get("auto").is_some() {
                out.push(FIELD);
            }
        } else if let Some(t) = i.get("text").and_then(Value::as_str) {
            // A line break inside a paragraph is white space on both sides.
            out.push_str(&t.replace('\n', "\u{2028}"));
        } else if i
            .pointer("/control/state")
            .and_then(Value::as_str)
            .is_some_and(|s| s == "data_bound")
        {
            // A content control bound to data shows what the data says when a reader opens the document (Word and
            // LibreOffice fill it again), so it is a field.
            out.push(FIELD);
        } else if let Some(obj) = i.as_object() {
            for v in obj.values() {
                self::inlines(v.get("inlines"), out, body);
            }
        }
    }
}
