//! The corpus harness for the story scan and the text model (`docs/design/wordml.md` 10.2).
//!
//! The corpus is read from the directories in `STETDOC_CORPUS_DIR`, separated by `:`, with the rules
//! of `xml/corpus_laws.rs`: `sets/curated` (or `sets/<name>` from `STETDOC_CORPUS_SET`) when it
//! exists, else the directory itself; a set that is named has to be whole (a `COMPLETE` marker and at
//! least one document) and agree with its `INDEX.tsv`; a file that cannot be read is a failure. With
//! the variable unset the test skips and passes. `wordml` may not depend on `package`, so the parts of
//! a document are read with the small reader below (central directory, ZIP64 records, raw inflate).
//!
//! For a document that is expected to be good (`expect` is `ok` and it is not under `hostile`):
//! 1. `stories` lists the body first, and the body scans, and so does every other story.
//! 2. The block counts of every story agree with an independent count of the bytes
//!    (`tests_corpus_count.rs`), and the views agree with the scan about kinds and rows.
//! 3. Every paragraph's map passes the invariants of 5.2 (the full set for a small paragraph, the
//!    checks that read the XML for a large one), and mapping twice gives the same map.
//! 4. The field index is consistent: ids are dense, every field is begun before it is ended, in the
//!    paragraph the index names.
//! 5. A second scan and a scan of a re-parse give the same blocks and fingerprints, and a refresh after
//!    a no-op equals a scan, field index included.
//! 6. `anchors`, `revisions`, `join_run`, `joined`, `sections`, `text_boxes`, `StyleDefs::read`,
//!    `NumberingDefs::read` and the id readers never panic.
//!
//! A document that is not expected to be good is read as far as it parses, and the body scan may
//! refuse it only with `malformed_input`.

use super::tests_corpus_count::{Count, count_blocks, naive_count, reasons};
use super::tests_corpus_zip::{RawPart, find, main_name, parse, rels_of, xml_parts};
use super::tests_gen_data::Rng;
use super::tests_gen_laws::{check_items, check_laws};
use super::tests_text::{FINAL, ORIGINAL, check_invariants};
use super::*;
use crate::api::error::ErrorCode;
use crate::xml::SpanTree;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ---- the corpus -----------------------------------------------------------------------------------

#[derive(Debug)]
struct Expect {
    code: String,
}

struct CorpusSet {
    dir: PathBuf,
    docs: Vec<PathBuf>,
    index: Option<HashMap<PathBuf, Expect>>,
}

fn parse_index(text: &str) -> Result<HashMap<PathBuf, Expect>, String> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name);
    let (Some(path), Some(expect)) = (col("path"), col("expect")) else {
        return Err("INDEX.tsv has no `path` and `expect` columns in its header".to_owned());
    };
    let mut out = HashMap::new();
    for (n, line) in lines.enumerate().filter(|(_, l)| !l.is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        let (Some(p), Some(e)) = (cols.get(path), cols.get(expect)) else {
            return Err(format!("INDEX.tsv row {} has too few columns", n + 2));
        };
        if out
            .insert(
                PathBuf::from(p),
                Expect {
                    code: (*e).to_owned(),
                },
            )
            .is_some()
        {
            return Err(format!("INDEX.tsv lists {p} twice"));
        }
    }
    Ok(out)
}

#[cfg(test)]
fn documents(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|k| k.is_dir()) {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                ["docx", "docm", "dotx", "dotm"].contains(&e.to_ascii_lowercase().as_str())
            }) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
fn load_from(dirs: &str, set: &str) -> Result<Vec<CorpusSet>, String> {
    let named: Vec<&str> = dirs.split(':').filter(|s| !s.is_empty()).collect();
    if named.is_empty() {
        return Err("STETDOC_CORPUS_DIR is set but names no directory".to_owned());
    }
    let mut sets = Vec::new();
    for d in named {
        let curated = Path::new(d).join("sets").join(set);
        let dir = if curated.is_dir() {
            curated
        } else {
            PathBuf::from(d)
        };
        if !dir.join("COMPLETE").is_file() {
            return Err(format!(
                "{} has no COMPLETE marker: the corpus is missing or was not fetched whole",
                dir.display()
            ));
        }
        let docs = documents(&dir);
        if docs.is_empty() {
            return Err(format!("{} holds no documents", dir.display()));
        }
        let index = match std::fs::read_to_string(dir.join("INDEX.tsv")) {
            Ok(text) => Some(parse_index(&text).map_err(|e| format!("{}: {e}", dir.display()))?),
            Err(_) => None,
        };
        if let Some(index) = &index {
            let on_disk: HashSet<&Path> = docs
                .iter()
                .filter_map(|p| p.strip_prefix(&dir).ok())
                .collect();
            let unlisted = on_disk.iter().filter(|p| !index.contains_key(**p)).count();
            let missing = index
                .keys()
                .filter(|p| !on_disk.contains(p.as_path()))
                .count();
            if unlisted + missing > 0 {
                return Err(format!(
                    "{}: {unlisted} documents are not in INDEX.tsv and {missing} files it lists are not there",
                    dir.display()
                ));
            }
        }
        sets.push(CorpusSet { dir, docs, index });
    }
    Ok(sets)
}

// ---- the laws ---------------------------------------------------------------------------------------

/// What the corpus held, for the report.
#[derive(Default)]
pub(super) struct Stats {
    documents: usize,
    skipped: usize,
    stories: usize,
    blocks: [usize; 3],
    opaque: usize,
    paragraphs: usize,
    items: usize,
    full_checks: usize,
    fields: usize,
    simple_fields: usize,
    fields_without_separate: usize,
    fields_across_paragraphs: usize,
    anomalies: BTreeMap<String, usize>,
    /// The kind `object_info` gives to each `Object` item, and the text boxes found in them.
    objects: BTreeMap<String, usize>,
    boxes_in_objects: usize,
    refused: Vec<String>,
    naive_differences: BTreeMap<String, usize>,
    pub differences: Vec<String>,
}

type Failure = String;

/// True if the root is a `w:document` with a `w:body`, read by local name.
fn has_body(tree: &SpanTree) -> bool {
    let root = tree.root();
    root.local_name() == "document" && root.child_elements().any(|c| c.local_name() == "body")
}

/// Checks one story of a document.
fn check_story(
    spec: &StorySpec,
    tree: &SpanTree,
    bytes: &[u8],
    rels: &Rels,
    stats: &mut Stats,
) -> Result<(), Failure> {
    let fail = |what: &str| format!("{} ({:?}): {what}", spec.part, spec.kind);
    let scanned = Story::scan(&spec.part, spec.kind, tree)
        .map_err(|e| fail(&format!("does not scan: {e}")))?;
    stats.stories += 1;
    for kind in [BlockKind::Paragraph, BlockKind::Table, BlockKind::Row] {
        let n = scanned.blocks().iter().filter(|e| e.kind == kind).count();
        stats.blocks[kind as usize] += n;
    }
    stats.opaque += scanned.opaque().len();
    for a in scanned.anomalies() {
        *stats.anomalies.entry(format!("{:?}", a.kind)).or_default() += 1;
    }
    // the independent count of the bytes
    let want = count_blocks(bytes, spec.kind);
    let of = |k| scanned.blocks().iter().filter(|e| e.kind == k).count();
    let got = Count {
        paragraphs: of(BlockKind::Paragraph),
        tables: of(BlockKind::Table),
        rows: of(BlockKind::Row),
    };
    let naive = naive_count(bytes);
    if naive != want {
        let why = reasons(bytes).join("; ");
        *stats.naive_differences.entry(why).or_default() += 1;
    }
    if got != want {
        stats.differences.push(format!(
            "{} ({:?}): scan {got:?}, count {want:?}",
            spec.part, spec.kind
        ));
    }
    // determinism: a second scan, and a scan of a re-parse
    let again = Story::scan(&spec.part, spec.kind, tree).map_err(|e| fail(&e.to_string()))?;
    let reparsed = parse(&spec.part, bytes).map_err(|e| fail(&e.to_string()))?;
    let third = Story::scan(&spec.part, spec.kind, &reparsed).map_err(|e| fail(&e.to_string()))?;
    if !scanned.same_as(&again) || !scanned.same_as(&third) {
        return Err(fail("two scans of the same bytes differ"));
    }
    let prints = scanned.fingerprints(tree);
    if prints != again.fingerprints(tree)
        || prints != third.fingerprints(&reparsed)
        || prints.len() != scanned.blocks().len()
    {
        return Err(fail("the fingerprints are not deterministic"));
    }
    // the views agree with the scan
    for (i, e) in scanned.blocks().iter().enumerate() {
        match scanned.block(tree, i) {
            Some(b) if b.kind() == e.kind => {}
            _ => {
                return Err(fail(&format!(
                    "the view of block {i} disagrees with the scan"
                )));
            }
        }
    }
    // every paragraph
    let mut rng = Rng::new(scanned.blocks().len() as u64);
    for i in 0..scanned.blocks().len() {
        let Some(p) = scanned.paragraph(tree, i) else {
            continue;
        };
        let m = p.map();
        stats.paragraphs += 1;
        stats.items += m.items().len();
        if scanned.blocks()[i].mark_revs() != p.mark_revs() {
            return Err(fail(&format!(
                "paragraph {i}: the scan and the paragraph read the mark differently"
            )));
        }
        for item in m.items().iter().filter(|x| x.kind == ItemKind::Object) {
            let Some(info) = scanned.object_info(tree, item.node) else {
                return Err(fail(&format!("paragraph {i}: an object with no info")));
            };
            *stats.objects.entry(format!("{:?}", info.kind)).or_default() += 1;
            stats.boxes_in_objects += info.text_boxes.len();
            if info.text_boxes.iter().any(|b| b.host != i) {
                return Err(fail(&format!("paragraph {i}: a box of another host")));
            }
        }
        for a in m.anomalies() {
            *stats.anomalies.entry(format!("{:?}", a.kind)).or_default() += 1;
        }
        // the whole set of laws costs about a millisecond for a paragraph, so plain text is sampled
        let plain = !m.has_revisions() && m.items().iter().all(|x| x.kind == ItemKind::Text);
        let small = m.items().len() <= 60 && m.len(Mode::Redline) <= 200 && (!plain || i % 8 == 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if small {
                check_laws(tree, p.id(), &m, &mut rng);
            } else {
                check_invariants(&m);
                check_items(tree, p.id(), &m);
            }
            let again = p.map();
            assert!(
                again.items() == m.items() && again.text(FINAL) == m.text(FINAL),
                "mapping twice differs"
            );
        }));
        if let Err(e) = result {
            let why = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default();
            return Err(fail(&format!("paragraph {i}: {why}")));
        }
        stats.full_checks += usize::from(small);
        for mode in [FINAL, ORIGINAL] {
            let run = scanned.join_run(tree, i, mode);
            if run.start != i || run.is_empty() {
                return Err(fail(&format!("join_run({i}) is {run:?}")));
            }
        }
    }
    // the field index
    let fields = scanned.fields(tree);
    for (n, f) in fields.iter().enumerate() {
        if f.id as usize != n {
            return Err(fail(&format!("field ids are not dense: {} at {n}", f.id)));
        }
        let begin = scanned
            .index_of(f.paragraph_begin)
            .ok_or_else(|| fail("a field begins outside every block"))?;
        if let Some(end) = f.paragraph_end {
            let end = scanned
                .index_of(end)
                .ok_or_else(|| fail("a field ends outside every block"))?;
            if end < begin {
                return Err(fail(&format!("field {n} ends before it begins")));
            }
            stats.fields_across_paragraphs += usize::from(end > begin);
        }
        if let Some(parent) = f.parent
            && parent >= f.id
        {
            return Err(fail(&format!("field {n} has the parent {parent}")));
        }
        if f.end.is_some() && f.separate.is_none() && !f.simple {
            stats.fields_without_separate += 1;
        }
        stats.simple_fields += usize::from(f.simple);
    }
    stats.fields += fields.len();
    // a refresh after a no-op is a scan
    let refreshed = Story::refresh(&scanned, tree, &[]).map_err(|e| fail(&e.to_string()))?;
    if !refreshed.story.same_as(&scanned)
        || refreshed.story.field_index(tree) != scanned.field_index(tree)
    {
        return Err(fail("a refresh after a no-op differs from a scan"));
    }
    // readers that must not panic
    let _ = (
        scanned.anchors(tree),
        scanned.revisions(tree),
        scanned.field_anomalies(tree),
        scanned.text_boxes(tree),
    );
    if spec.kind == StoryKind::Body {
        let _ = scanned.sections(tree, rels);
    }
    for i in 0..scanned.blocks().len() {
        let _ = (scanned.own_parts(tree, i), scanned.joined(tree, i, FINAL));
        if let Some(p) = scanned.paragraph(tree, i) {
            let _ = enclosing_controls(p.element());
        }
    }
    let mut ids = Vec::new();
    for kind in [
        IdKind::ParaId,
        IdKind::TextId,
        IdKind::CommentDurableId,
        IdKind::AnnotationId,
    ] {
        existing_ids(kind, tree, &mut ids);
    }
    Ok(())
}

/// Checks one document. `good` says it is expected to read whole.
fn check_document(zip: &[u8], good: bool, stats: &mut Stats) -> Result<(), Failure> {
    let parts = match xml_parts(zip) {
        Ok(p) => p,
        Err(why) if good => return Err(format!("this test cannot read the container: {why}")),
        Err(_) => {
            stats.skipped += 1;
            return Ok(());
        }
    };
    let Some(main) = main_name(&parts) else {
        if good {
            return Err("the container has no main part".into());
        }
        stats.skipped += 1;
        return Ok(());
    };
    let main_bytes = match find(&parts, &main).map(|p| &p.bytes) {
        Some(Ok(b)) => b,
        _ if good => return Err(format!("the main part {main} cannot be read here")),
        _ => {
            stats.skipped += 1;
            return Ok(());
        }
    };
    let main_tree = match parse(&main, main_bytes) {
        Ok(t) => t,
        Err(_) if !good => {
            stats.skipped += 1;
            return Ok(());
        }
        Err(e) => return Err(format!("the main part does not parse: {e}")),
    };
    let rels = rels_of(&parts, &main);
    let specs = stories(&main, &main_tree, &rels);
    if specs.first().map(|s| (&*s.part, s.kind)) != Some((main.as_str(), StoryKind::Body)) {
        return Err("stories() does not start with the body".into());
    }
    let mut seen = HashSet::new();
    for spec in &specs {
        if !seen.insert(spec.part.clone()) {
            return Err(format!("{} is listed twice", spec.part));
        }
    }
    let name: Arc<str> = Arc::from(main.as_str());
    match Story::scan(&name, StoryKind::Body, &main_tree) {
        Ok(_) => {}
        Err(e) if !good && e.code() == ErrorCode::MalformedInput => {
            stats.skipped += 1;
            return Ok(());
        }
        // a main part that is not a `w:document` with a `w:body` is the one refusal of `wordml`, at the
        // `document` stage (3.2). It is expected only when the bytes say so
        Err(e) if e.code() == ErrorCode::MalformedInput && !has_body(&main_tree) => {
            stats.refused.push(format!("{main}: {e}"));
            return Ok(());
        }
        Err(e) => return Err(format!("the body does not scan: {e}")),
    }
    stats.documents += 1;
    for spec in &specs {
        let part = find(&parts, &spec.part)
            .ok_or_else(|| format!("{} is a story but not a part", spec.part))?;
        let Ok(bytes) = &part.bytes else {
            if good {
                return Err(format!("the story {} cannot be read here", spec.part));
            }
            continue;
        };
        let tree = match parse(&spec.part, bytes) {
            Ok(t) => t,
            Err(e) if !good => {
                let _ = e;
                continue;
            }
            Err(e) => return Err(format!("the story {} does not parse: {e}", spec.part)),
        };
        check_story(spec, &tree, bytes, &rels, stats)?;
    }
    // the definitions
    for (suffix, styles) in [("/styles", true), ("/numbering", false)] {
        let Some(RelTarget::Part(name)) = rels
            .iter()
            .find(|r| r.rel_type.ends_with(suffix))
            .map(|r| r.target.clone())
        else {
            continue;
        };
        let Some(tree) = find(&parts, &name)
            .and_then(|p| p.bytes.as_ref().ok())
            .and_then(|b| parse(&name, b).ok())
        else {
            continue;
        };
        if styles {
            let defs = StyleDefs::read(&tree);
            for s in defs.styles() {
                let _ = defs.chain(&s.id).count();
            }
            let _ = defs.issues();
        } else {
            let defs = NumberingDefs::read(&tree);
            for n in defs.nums() {
                let _ = (defs.level(n.id, 0), defs.abstract_of(n.id));
            }
            let _ = defs.issues();
        }
    }
    Ok(())
}

#[test]
fn the_story_scan_and_the_text_model_hold_for_every_document_of_the_corpus() {
    let Ok(dirs) = std::env::var("STETDOC_CORPUS_DIR") else {
        eprintln!("STETDOC_CORPUS_DIR is not set: skipping the wordml laws over the corpus");
        return;
    };
    let set = std::env::var("STETDOC_CORPUS_SET").unwrap_or_else(|_| "curated".to_owned());
    let sets =
        load_from(&dirs, &set).unwrap_or_else(|why| panic!("the corpus cannot be used: {why}"));
    let mut stats = Stats::default();
    let mut failures: Vec<String> = Vec::new();
    for set in &sets {
        for path in &set.docs {
            let zip = std::fs::read(path).unwrap_or_default();
            let expect = set
                .index
                .as_ref()
                .and_then(|ix| path.strip_prefix(&set.dir).ok().and_then(|r| ix.get(r)));
            let hostile = path.components().any(|c| c.as_os_str() == "hostile");
            let good = expect.map_or(!hostile, |e| e.code == "ok") && !hostile;
            if zip.is_empty() {
                failures.push(format!("{}: the file cannot be read", path.display()));
                continue;
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                check_document(&zip, good, &mut stats)
            }));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(why)) => failures.push(format!("{}: {why}", path.display())),
                Err(_) => failures.push(format!("{}: panicked", path.display())),
            }
        }
    }
    eprintln!(
        "wordml corpus: {} documents read, {} skipped, {} stories; blocks: {} paragraphs, {} tables, {} rows, {} opaque; {} paragraph maps ({} items, {} fully checked); fields: {} ({} simple, {} complex without separate, {} across paragraphs)",
        stats.documents,
        stats.skipped,
        stats.stories,
        stats.blocks[0],
        stats.blocks[1],
        stats.blocks[2],
        stats.opaque,
        stats.paragraphs,
        stats.items,
        stats.full_checks,
        stats.fields,
        stats.simple_fields,
        stats.fields_without_separate,
        stats.fields_across_paragraphs
    );
    eprintln!("anomalies: {:?}", stats.anomalies);
    eprintln!(
        "objects by kind: {:?}, with {} text boxes in them",
        stats.objects, stats.boxes_in_objects
    );
    eprintln!(
        "stories where the naive tag count differs from the rules (by what the bytes hold): {:?}",
        stats.naive_differences
    );
    for r in &stats.refused {
        eprintln!("refused at the document stage: {r}");
    }
    for d in &stats.differences {
        eprintln!("count difference: {d}");
    }
    assert!(
        failures.is_empty(),
        "the wordml laws failed on {} documents:\n{}",
        failures.len(),
        failures
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        stats.differences.is_empty(),
        "the block counts differ in {} stories",
        stats.differences.len()
    );
}
