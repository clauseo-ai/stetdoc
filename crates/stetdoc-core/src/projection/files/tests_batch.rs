//! `to_batch` over a real state: the base check, the baseline, and the way out for what the drafts cannot say
//! (design 6.1).

use crate::api::error::ErrorCode;
use crate::api::files::{PlannedOp, ReadableFiles};
use crate::api::limits::Limits;
use crate::api::view::FormatId;
use crate::index::testing::*;
use crate::projection::files::{to_batch, to_batch_in};
use crate::projection::formats::by_id;
use crate::projection::render::{render_files, render_files_in};
use crate::state::State;

fn doc() -> Fixture {
    let body = format!(
        "{}{}{}",
        px("<w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>DEFINITIONS</w:t></w:r>"),
        p("The Seller shall pay the Price."),
        p("Notices go to the Buyer.")
    );
    Fixture::new(&body).styles(
        "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/></w:style>",
    )
}

fn edited(files: &ReadableFiles, edit: impl Fn(&str) -> String) -> ReadableFiles {
    ReadableFiles {
        files: files
            .files
            .iter()
            .map(|(name, text)| (name.clone(), edit(text)))
            .collect(),
        ..files.clone()
    }
}

/// The SHA-256 of each file, by name: what `Planned.baseline` holds.
fn hashes_of(files: &ReadableFiles) -> Vec<(String, [u8; 32])> {
    files
        .files
        .iter()
        .map(|(name, text)| (name.clone(), crate::refs::sha256(text.as_bytes())))
        .collect()
}

fn batch(
    state: &State,
    files: &ReadableFiles,
) -> Result<crate::api::files::Planned, crate::api::error::StetError> {
    let format = by_id(files.format.as_str())?;
    to_batch_in(state, files, format, &Limits::default())
}

/// The readable files in any registered format, `json/1` included, for the tests that run on both.
fn render_in(
    state: &State,
    format: &FormatId,
) -> Result<ReadableFiles, crate::api::error::StetError> {
    render_files_in(state, by_id(format.as_str())?)
}

#[test]
fn json_is_never_a_working_copy_format_and_is_refused_by_render_files_and_to_batch() {
    let state = doc().state();
    let json = FormatId::new(FormatId::JSON);
    let e = render_files(&state, &json).unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(e.message().contains("not a working-copy format"), "{e}");
    let files = render_in(&state, &json).unwrap();
    let e = to_batch(&state, &files, &Limits::default()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    // stet/1 goes through both
    let files = render_files(&state, &FormatId::stet()).unwrap();
    assert!(to_batch(&state, &files, &Limits::default()).is_ok());
}

#[test]
fn unedited_files_give_no_operation_and_the_baseline_is_the_files() {
    let state = doc().state();
    for format in [FormatId::stet(), FormatId::new(FormatId::JSON)] {
        let files = render_in(&state, &format).unwrap();
        assert_eq!(files.format, format);
        assert_eq!(files.base, state.base_token().to_string());
        let plan = batch(&state, &files).unwrap();
        assert!(plan.ops.is_empty() && plan.origins.is_empty() && plan.notes.is_empty());
        assert_eq!(plan.baseline, hashes_of(&files), "{}", format.as_str());
    }
}

#[test]
fn a_changed_paragraph_is_a_replace_with_the_line_it_came_from() {
    let state = doc().state();
    let files = render_files(&state, &FormatId::stet()).unwrap();
    let plan = batch(&state, &edited(&files, |t| t.replace("Buyer", "Purchaser"))).unwrap();
    assert_eq!(plan.ops.len(), 1);
    let PlannedOp::Replace {
        block,
        start,
        end,
        with,
    } = &plan.ops[0]
    else {
        panic!("{:?}", plan.ops[0]);
    };
    // a changed slot is one replace of the whole old slot: `revisions` finds the words that changed
    assert_eq!(
        (block.as_str(), *start, *end, with.as_str()),
        ("p3", 0, 24, "Notices go to the Purchaser.")
    );
    assert_eq!(plan.origins[0].file, "document.md");
    assert!(plan.origins[0].line > 0);
}

#[test]
fn files_of_another_version_or_another_document_are_stale_before_anything_is_read() {
    let state = doc().state();
    let files = render_files(&state, &FormatId::stet()).unwrap();
    let other = Fixture::new(&p("Something else entirely.")).state();
    let e = batch(&other, &files).unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleFiles);
    let mut older = files.clone();
    let (ns, rest) = older.base.split_once(':').unwrap();
    let (_, lineage) = rest.split_once(':').unwrap();
    older.base = format!("{ns}:9:{lineage}");
    assert_eq!(
        batch(&state, &older).unwrap_err().code(),
        ErrorCode::StaleFiles
    );
    // a base that is no token at all, and a format that is not registered
    let mut junk = files.clone();
    junk.base = "banana".into();
    assert_eq!(
        batch(&state, &junk).unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
    let mut unknown = files;
    unknown.format = FormatId::new("stet/9");
    assert_eq!(
        batch(&state, &unknown).unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
}

#[test]
fn a_ref_that_never_existed_is_not_found_and_says_what_the_files_hold() {
    let state = doc().state();
    let files = render_files(&state, &FormatId::stet()).unwrap();
    let e = batch(&state, &edited(&files, |t| format!("{t}p77 text\n"))).unwrap_err();
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert!(e.message().contains("p1 to p3"), "{}", e.message());
}

#[test]
fn a_new_line_after_a_heading_takes_the_style_that_the_document_says_follows_it() {
    let state = doc().state();
    let files = render_files(&state, &FormatId::stet()).unwrap();
    let plan = batch(
        &state,
        &edited(&files, |t| {
            t.replace("DEFINITIONS\n", "DEFINITIONS\nBrand new line\n")
        }),
    )
    .unwrap();
    assert_eq!(plan.ops.len(), 1, "{:?}", plan.ops);
    let PlannedOp::Insert {
        anchor, paragraphs, ..
    } = &plan.ops[0]
    else {
        panic!("{:?}", plan.ops[0]);
    };
    assert_eq!(anchor, "p1");
    assert_eq!(paragraphs[0].text, "Brand new line");
    assert_eq!(
        paragraphs[0].style.as_deref(),
        Some("Normal"),
        "w:next of heading 1"
    );
}

/// T1 through the real path: the unedited files of every document of the corpus, written by each format and
/// given back, are no operation, and the baseline that `to_batch` made is the files it was given.
#[test]
fn t1_on_every_document_of_the_corpus_through_to_batch() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use stetdoc_testkit::corpus::Corpus;
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut failures: Vec<String> = Vec::new();
    let mut documents = 0usize;
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect.is_ok() && e.tier != "hostile")
    {
        let state = State::open(entry.bytes(), &Limits::default())
            .unwrap_or_else(|e| panic!("{} must open and does not: {e}", entry.id));
        for format in [FormatId::stet(), FormatId::new(FormatId::JSON)] {
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                let files = render_in(&state, &format).map_err(|e| e.message().to_string())?;
                let plan = batch(&state, &files).map_err(|e| e.message().to_string())?;
                if !plan.ops.is_empty() {
                    return Err(format!(
                        "{} phantom operations: {:?}",
                        plan.ops.len(),
                        plan.ops.first()
                    ));
                }
                if plan.baseline != hashes_of(&files) {
                    return Err("the baseline is not the files".to_string());
                }
                Ok(())
            }));
            match outcome {
                Ok(Ok(())) => {}
                Ok(Err(why)) => failures.push(format!("{} {}: {why}", entry.id, format.as_str())),
                Err(_) => failures.push(format!("{} {}: a panic", entry.id, format.as_str())),
            }
        }
        documents += 1;
    }
    eprintln!("T1 through to_batch held on {documents} documents in two formats");
    assert!(
        failures.is_empty(),
        "{} fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
