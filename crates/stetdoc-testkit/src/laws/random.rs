//! Random edit batches over a document, for the edit checks: L2 to L5 on property tests.
//!
//! A run picks one seed ([`seed`]: `STETDOC_SEED` when it is set, else the clock) and prints it. Each document gets its own
//! stream, from the seed and a hash of its bytes, so the same seed gives the same batches on the same document, and a failure
//! names the seed and the batch. [`batches`] runs [`batch_count`] batches made by `laws::generate` from the document's view, each on a
//! fresh copy (now and then on a copy that a prelude batch of another author, or comments of the author, changed first, so that
//! `accept`, `reject` and the comment operations have something of both to act on). Each keeps the laws of
//! `edits::run_batch` (H14, atomic refusal, output validity, L4), and also:
//!
//! - **refusals**: an operation the generator made to be refused is, and in a batch of one with one of the codes it names;
//! - **L2 Locality** by package entry: no entry is lost, and every entry outside the parts an edit may touch (the story parts,
//!   the comment parts, styles, numbering, settings, relationships and content types) keeps its raw bytes;
//! - **L3 Edits stick**: the files of the saved package, opened again, are the files of the edited document (but for the refs,
//!   which a fresh open numbers anew), and a whole-paragraph `replace` or an `insert` of plain text is that text in the Final view
//!   (a `replace` of a note's paragraph keeps the note's own number, U+0003, as every text edit does: design `revisions.md`,
//!   3.6.4; `laws::generate::kept_note_marks`);
//! - **L5 Reversibility**: for a tracked batch of reversible operations, rejecting the author's revisions gives back the files
//!   of the original (but for the base), and accepting them leaves the Final view as the edit made it.
//!
//! `laws::random_files` does the same for the edit of files (`laws::files`): random lines changed, added and deleted in
//! `document.md`.

use super::blocks::per_block;
use super::edits::{Applied, input_codes, run_batch};
use super::generate::{Facts, Generated, Rng, fnv, generate};
use super::locality::locality;
use std::cell::OnceCell;
use std::io::Write;
use std::sync::OnceLock;
use stetdoc_core::{
    Batch, Budget, Document, Edit, ErrorCode, FindingCode, Limits, Mode, OpenOptions,
    RevisionFilter, Scope, Stamp, StoryName, ViewSpec,
};
use view_lines::{ViewLine, block_lines, restored};

#[cfg(test)]
mod tests;
mod view_lines;

/// The seed of this run: `STETDOC_SEED`, or one from the clock, printed once so that a failure can be run again.
pub fn seed() -> u64 {
    static SEED: OnceLock<u64> = OnceLock::new();
    *SEED.get_or_init(|| {
        let seed = std::env::var("STETDOC_SEED")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or_else(|| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64);
                fnv(&now.to_le_bytes())
            });
        // Written to the process's stderr, past the test harness's capture, so that every run shows its seed, and to the run
        // page's summary on GitHub Actions.
        let line =
            format!("random edit batches: seed {seed} (STETDOC_SEED={seed} runs the same batches)");
        let _ = writeln!(std::io::stderr(), "{line}");
        super::sweep::to_the_summary(&line);
        seed
    })
}

fn count(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(default)
}

/// How many random batches each document gets: `STETDOC_BATCHES`, 50 by default (the minimum the edit checks need).
pub fn batch_count() -> usize {
    count("STETDOC_BATCHES", 50)
}

/// How many random edits of files each document gets: `STETDOC_FILE_EDITS`, 10 by default.
pub fn file_edit_count() -> usize {
    count("STETDOC_FILE_EDITS", 10)
}

fn stamp(author: &str) -> Stamp {
    Stamp::new(author, "2026-10-06T10:00:00")
}

fn open(bytes: &[u8]) -> Result<Document, String> {
    Document::open(bytes.to_vec(), &OpenOptions::default()).map_err(|e| format!("open: {e}"))
}

/// Runs [`batch_count`] random batches on a document, with the run's seed.
pub fn batches(bytes: &[u8]) -> Result<(), String> {
    batches_with(bytes, seed(), batch_count())
}

/// Runs `n` random batches on a document from `seed`. A failure names the seed, the batch's number and the batch.
pub fn batches_with(bytes: &[u8], seed: u64, n: usize) -> Result<(), String> {
    let mut rng = Rng::new(seed ^ fnv(bytes));
    let plain = Base::new(bytes.to_vec())?;
    for k in 0..n {
        // One batch in five runs on a document that another author, or the author's comments, changed first.
        let (preluded, prelude) = if rng.chance(20) {
            prelude(bytes, &mut rng)?
        } else {
            (Vec::new(), None)
        };
        let fresh;
        let base = if prelude.is_some() {
            fresh = Base::new(preluded)?;
            &fresh
        } else {
            &plain
        };
        let g = generate(&mut rng, &base.facts);
        check(base, &g).map_err(|e| {
            let after = prelude
                .as_deref()
                .map(|p| format!(" after the prelude {}", cut(p)))
                .unwrap_or_default();
            format!(
                "seed {seed}, batch {k}{after}: {}: {e}",
                cut(&g.to_string())
            )
        })?;
    }
    Ok(())
}

/// The Original and the Final lines of a document's body, and whether it has revisions of the author.
type Original = (Vec<ViewLine>, Vec<ViewLine>, bool);

/// A document that batches run on, with what every batch on it needs, worked out once.
pub struct Base {
    /// The document.
    pub bytes: Vec<u8>,
    /// The violation codes it has as it opens.
    pub input: Vec<FindingCode>,
    /// What its view says.
    pub facts: Facts,
    /// Its Original and Final lines and whether it has revisions of the author, for L5, when a batch needs them.
    original: OnceCell<Result<Original, String>>,
}

impl Base {
    /// The base of a document.
    pub fn new(bytes: Vec<u8>) -> Result<Base, String> {
        Ok(Base {
            input: input_codes(&bytes)?,
            facts: Facts::of(&open(&bytes)?)?,
            bytes,
            original: OnceCell::new(),
        })
    }

    fn original(&self) -> Result<&Original, String> {
        self.original
            .get_or_init(|| {
                let doc = open(&self.bytes)?;
                let mine = RevisionFilter {
                    author: Some("Clauseo".to_string()),
                    kind: None,
                };
                let has_mine = !doc.revisions(&mine).map_err(|e| e.to_string())?.is_empty();
                Ok((
                    lines(&doc, Mode::Original)?,
                    lines(&doc, Mode::Final)?,
                    has_mine,
                ))
            })
            .as_ref()
            .map_err(Clone::clone)
    }
}

/// A batch as a failure shows it: the first 400 characters, since the seed gives the whole of it, or all of it with
/// `STETDOC_FULL_BATCHES` set.
fn cut(text: &str) -> String {
    let n = text.chars().count();
    if n <= 400 || std::env::var_os("STETDOC_FULL_BATCHES").is_some() {
        return text.to_string();
    }
    let head: String = text.chars().take(400).collect();
    format!("{head}... ({n} characters, the seed gives the rest)")
}

/// A prelude: a tracked batch by another author, or comments by the author, applied and saved. Returns the bytes and the
/// prelude, or the document unchanged when the prelude was refused.
fn prelude(bytes: &[u8], rng: &mut Rng) -> Result<(Vec<u8>, Option<String>), String> {
    let mut doc = open(bytes)?;
    let facts = Facts::of(&doc)?;
    let (author, names): (&str, &[&'static str]) = if rng.chance(50) {
        (
            "Reviewer",
            &["replace", "insert", "delete", "format", "comment"],
        )
    } else {
        ("Clauseo", &["comment"])
    };
    let ops: Vec<String> = (0..1 + rng.below(2))
        .map(|_| {
            let name = *rng.pick(names).unwrap_or(&"comment");
            super::generate::operation(rng, &facts, Some(name))
        })
        .filter(|o| o.refused.is_none())
        .map(|o| o.json)
        .collect();
    if ops.is_empty() {
        return Ok((bytes.to_vec(), None));
    }
    let ops = ops.join(",");
    let Ok(b) = Batch::parse(
        &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
        &Limits::default(),
    ) else {
        return Ok((bytes.to_vec(), None));
    };
    if doc.edit(Edit::ops(b, stamp(author))).is_err() {
        return Ok((bytes.to_vec(), None));
    }
    let saved = doc.save().map_err(|e| format!("prelude: save: {e}"))?;
    Ok((saved.bytes, Some(format!("[{ops}] by {author}"))))
}

/// The laws of one generated batch on a document.
pub fn check(base: &Base, g: &Generated) -> Result<(), String> {
    let applied = match run_batch(&base.bytes, &base.input, &g.json(), g.tracked)? {
        Err(e) => {
            // A target that cannot be resolved may refuse the operation before its payload is read.
            let target = [
                ErrorCode::NotFound,
                ErrorCode::AmbiguousMatch,
                ErrorCode::StaleRef,
                ErrorCode::ReadonlyRegion,
                ErrorCode::CrossBlock,
                ErrorCode::Unsupported,
            ];
            if let [only] = g.ops.as_slice()
                && let Some(codes) = only.refused
                && !codes.contains(&e.code())
                && !target.contains(&e.code())
            {
                return Err(format!(
                    "refused as {}, but the operation must be refused with one of {:?}: {e}",
                    e.code(),
                    codes.iter().map(|c| c.to_string()).collect::<Vec<_>>()
                ));
            }
            return Ok(());
        }
        Ok(applied) => applied,
    };
    // An operation made to be refused must be, when nothing before it in the batch changed what it names: a later operation's
    // offset past the end of a paragraph may fit the text an earlier one wrote.
    if let Some(i) = g.first_refused().filter(|i| *i == 0) {
        return Err(format!(
            "operation {i} must be refused with one of {:?}, and the batch went through",
            g.ops
                .get(i)
                .and_then(|o| o.refused)
                .unwrap_or_default()
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
        ));
    }
    super::receipts::stale_refs_are_retired(&base.facts, &applied.doc, &applied.receipt)?;
    locality(&base.bytes, &applied.saved, &base.facts.parts)?;
    per_block(&base.bytes, &applied.saved, &base.facts, g)?;
    sticks(g, &applied)?;
    reversible(base, g, applied)
}

/// The `json/1` Redline views of the body, the notes and the headers, with the base and every ref written as `#`. JSON and not
/// the files, because a file wraps a long line at a width that the length of its ref changes.
fn comparable(doc: &Document) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for story in [StoryName::Body, StoryName::Notes, StoryName::Headers] {
        let spec = ViewSpec {
            scope: Scope::Story(story),
            mode: Mode::Redline,
            budget: Budget(u32::MAX),
            format: stetdoc_core::FormatId::new(stetdoc_core::FormatId::JSON),
            ..ViewSpec::default()
        };
        let text = match doc.view(&spec) {
            Ok(page) => page.text.replace(&doc.base(), "BASE"),
            Err(e) => format!("{}", e.code()),
        };
        out.push((format!("the {story:?} view"), without_refs(&text)));
    }
    Ok(out)
}

/// A text with every ref in it (at the start of a line, and in a warning or a marker) written as `#`.
fn without_refs(text: &str) -> String {
    text.lines()
        .map(|l| {
            let mut out = String::new();
            let mut word = String::new();
            for c in l.chars().chain(std::iter::once('\n')) {
                if c.is_ascii_alphanumeric() {
                    word.push(c);
                    continue;
                }
                out.push_str(if is_ref(&word) { "#" } else { &word });
                word.clear();
                if c != '\n' {
                    out.push(c);
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_ref(word: &str) -> bool {
    let mut chars = word.chars();
    matches!(chars.next(), Some('p' | 't' | 'r'))
        && !chars.as_str().is_empty()
        && chars.all(|c| c.is_ascii_digit())
}

/// L3.
fn sticks(g: &Generated, applied: &Applied) -> Result<(), String> {
    let reopened = open(&applied.saved).map_err(|e| format!("L3: {e}"))?;
    let (a, b) = (comparable(&applied.doc)?, comparable(&reopened)?);
    for ((name, x), (_, y)) in a.iter().zip(&b) {
        if x != y {
            return Err(format!(
                "L3: the saved package, opened again, shows another {name}: {}",
                first_difference(x, y)
            ));
        }
    }
    if a.len() != b.len() {
        return Err("L3: the saved package, opened again, has other files".to_string());
    }
    // A plain whole-paragraph replace or insert in a batch of one is that text in the Final view.
    let [only] = g.ops.as_slice() else {
        return Ok(());
    };
    let Some((target, texts)) = &only.sticks else {
        return Ok(());
    };
    if applied
        .receipt
        .warnings
        .iter()
        .chain(
            applied
                .receipt
                .operations
                .iter()
                .flat_map(|o| o.warnings.iter()),
        )
        .any(|w| w.code.as_str() == "hidden_text")
    {
        return Ok(());
    }
    let refs: Vec<String> = match target.strip_prefix('+') {
        Some(_) => applied
            .receipt
            .operations
            .iter()
            .flat_map(|o| o.added.clone())
            .collect(),
        None => vec![target.clone()],
    };
    let facts = Facts::of(&applied.doc)?;
    let shown: Vec<String> = refs
        .iter()
        .map(|r| {
            facts
                .paras
                .iter()
                .find(|p| &p.r == r)
                .map(|p| p.text.iter().collect::<String>())
                .unwrap_or_default()
        })
        .collect();
    if &shown != texts {
        return Err(format!(
            "L3: the Final text of {refs:?} is {shown:?}, and the batch wrote {texts:?}"
        ));
    }
    Ok(())
}

/// Where two texts first differ: the line, and 60 characters around the first character that differs on it.
fn first_difference(x: &str, y: &str) -> String {
    let at = x.lines().zip(y.lines()).position(|(p, q)| p != q);
    let Some(i) = at else {
        return "one has more lines".to_string();
    };
    let (p, q): (Vec<char>, Vec<char>) = (
        x.lines().nth(i).unwrap_or_default().chars().collect(),
        y.lines().nth(i).unwrap_or_default().chars().collect(),
    );
    let c = p
        .iter()
        .zip(&q)
        .position(|(a, b)| a != b)
        .unwrap_or(p.len().min(q.len()));
    let around = |s: &[char]| {
        s.iter()
            .skip(c.saturating_sub(30))
            .take(60)
            .collect::<String>()
    };
    format!(
        "line {i}, at character {c}: {:?} and then {:?}",
        around(&p),
        around(&q)
    )
}

/// L5.
fn reversible(base: &Base, g: &Generated, applied: Applied) -> Result<(), String> {
    let Applied {
        mut doc,
        receipt,
        mut resumed,
        ..
    } = applied;
    if !g.tracked || !g.reversible() || receipt.revisions.is_empty() {
        return Ok(());
    }
    let (original_lines, final_lines, has_mine) = base.original()?;
    if *has_mine {
        return Ok(());
    }
    // Accepting gives the intended text: the Final view as the edit made it.
    // The Final texts of the body's paragraphs. A paragraph whose mark the author deleted joins the next one until it is accepted,
    // so the two are joined, and spaces are left out, because the view does not show a space at the end of a paragraph.
    let shown = |d: &Document| -> Result<Vec<String>, String> {
        let mut out: Vec<String> = Vec::new();
        let mut join = false;
        for p in Facts::in_mode(d, Mode::Final)?
            .paras
            .iter()
            .filter(|p| p.story == "body")
        {
            let text: String = p.text.iter().filter(|c| !c.is_whitespace()).collect();
            match out.last_mut() {
                Some(last) if join => last.push_str(&text),
                _ => out.push(text),
            }
            join = p.joins_next;
        }
        Ok(lone_empty_is_none(out))
    };
    let intended = shown(&resumed)?;
    review(&mut resumed, "accept")?;
    let accepted = shown(&resumed)?;
    if accepted != intended {
        let at = accepted
            .iter()
            .zip(&intended)
            .position(|(p, q)| p != q)
            .unwrap_or(accepted.len().min(intended.len()));
        return Err(format!(
            "L5: accepting the author's revisions changed the Final view (compared without spaces), first at line {at}: {}",
            first_difference(
                intended.get(at).map_or("(none)", |s| s.as_str()),
                accepted.get(at).map_or("(none)", |s| s.as_str())
            )
        ));
    }
    // Rejecting gives back the original: the same Original and Final views, but for the refs. The redline files are not compared,
    // because a revision of another author that the edit cut in two stays two revisions with the same text after the reject.
    review(&mut doc, "reject")?;
    for (mode, a) in [(Mode::Original, original_lines), (Mode::Final, final_lines)] {
        let b = lines(&doc, mode)?;
        if let Err(at) = restored(a, &b) {
            return Err(format!(
                "L5: rejecting the author's revisions did not restore the {mode:?} view, first at line {at}: {:?} and then {:?}",
                a.get(at).map(|l| &l.text),
                b.get(at).map(|l| &l.text)
            ));
        }
    }
    Ok(())
}

/// A line without its comment markers (`{c3}`, `{/c3}`): a comment survives the removal of its text, and its markers are hoisted
/// into a neighbour (comments-and-notes.md item 7), so a paragraph that holds only them reads as empty.
pub(super) fn without_comment_marks(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find('{') {
        out.push_str(rest.get(..at).unwrap_or_default());
        let tail = rest.get(at..).unwrap_or_default();
        let marker = tail.find('}').filter(|&end| {
            let inner = tail.get(1..end).unwrap_or_default();
            let id = inner
                .trim_start_matches('/')
                .strip_prefix('c')
                .unwrap_or_default();
            !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())
        });
        match marker {
            Some(end) => rest = tail.get(end + 1..).unwrap_or_default(),
            None => {
                out.push('{');
                rest = tail.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out.trim_end().to_string()
}

/// The block lines of the body in a mode, one per block, without refs and without comment markers.
fn lines(doc: &Document, mode: Mode) -> Result<Vec<ViewLine>, String> {
    let mut spec = ViewSpec {
        scope: Scope::Story(StoryName::Body),
        mode,
        budget: Budget(u32::MAX),
        ..ViewSpec::default()
    };
    let mut out = Vec::new();
    for _ in 0..10_000 {
        let page = doc.view(&spec).map_err(|e| format!("view: {e}"))?;
        out.extend(block_lines(&page.text));
        match page.next {
            Some(next) => spec.after = Some(next),
            None => return Ok(view_lines::lone_empty_is_none(out)),
        }
    }
    Err("a view did not end".to_string())
}

/// A story whose only paragraph is empty has no lines: a story is never left with no paragraph, so removing everything in it
/// (accepting the deletion of the body's only table) keeps one empty paragraph, which Word shows as well.
fn lone_empty_is_none(lines: Vec<String>) -> Vec<String> {
    match lines.as_slice() {
        [only] if only.trim().is_empty() || only.trim() == "#" => Vec::new(),
        _ => lines,
    }
}

fn review(doc: &mut Document, action: &str) -> Result<(), String> {
    let b = Batch::parse(
        &format!(
            r#"{{"base":"{}","ops":[{{"op":"{action}","author":"Clauseo"}}]}}"#,
            doc.base()
        ),
        &Limits::default(),
    )
    .map_err(|e| e.to_string())?;
    doc.edit(Edit::ops(b, stamp("Clauseo")))
        .map(|_| ())
        .map_err(|e| format!("L5: {action}: {e}"))
}
