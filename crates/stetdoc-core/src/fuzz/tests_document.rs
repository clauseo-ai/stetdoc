//! Tests of the entry points that go through `Document`: the small document opens, the batches of the conformance suite and the
//! readable file pass every choice, and what is refused is not a failure.

use super::document::{Outcome, batch, files};
use stetdoc_testkit::laws::conformance::{CASES, FORGERIES};

fn with_head(head: [u8; 4], text: &str) -> Vec<u8> {
    let mut data = head.to_vec();
    data.extend(text.as_bytes());
    data
}

#[test]
fn an_input_too_short_for_the_choices_does_nothing() {
    assert_eq!(batch(&[]), Outcome::Skipped);
    assert_eq!(batch(&[1, 2, 3]), Outcome::Skipped);
    assert_eq!(files(&[1, 2]), Outcome::Skipped);
}

#[test]
fn every_batch_of_the_conformance_suite_passes_every_choice() {
    let ops = CASES
        .iter()
        .flat_map(|c| [c.prelude, c.ops])
        .chain(FORGERIES.iter().map(|(ops, _)| *ops))
        .filter(|ops| !ops.is_empty());
    let token = crate::document::Document::open(
        super::document::docx(),
        &crate::api::options::OpenOptions::default(),
    )
    .expect("opens")
    .base();
    let (mut ran, mut applied, mut blocks) = (0, 0, 0);
    for ops in ops {
        let json = format!(r#"{{"base":"BASE_TOKEN","ops":[{ops}]}}"#);
        let mut texts = vec![json.clone()];
        // The same batch in block text, as the seeds have it.
        if let Ok(parsed) = crate::api::batch::Batch::parse(
            &json.replace("BASE_TOKEN", &token),
            &crate::api::limits::Limits::default(),
        ) && let Ok(block) = parsed.to_block()
        {
            texts.push(block.replace(&token, "BASE_TOKEN"));
            blocks += 1;
        }
        for text in &texts {
            for mode in [0u8, 1, 2, 4, 7] {
                applied +=
                    usize::from(batch(&with_head([mode, 0, 0, 0], text)) == Outcome::Applied);
                ran += 1;
            }
        }
    }
    assert!(blocks > 20, "{blocks}");
    assert!(ran > 100, "{ran}");
    assert!(
        applied * 4 > ran,
        "only {applied} of {ran} applied: the checks ran on refusals"
    );
}

#[test]
fn a_batch_that_applies_to_the_small_document_passes_tracked_untracked_and_dry() {
    let json = r#"{"base":"BASE_TOKEN","ops":[{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]}]}"#;
    for mode in 0..8u8 {
        assert_eq!(
            batch(&with_head([mode, 0, 0, 0], json)),
            Outcome::Applied,
            "{mode}"
        );
    }
    let block =
        "base: BASE_TOKEN\n---\n@replace\nref: p2\nfind: \u{201C}Losses\u{201D}\nwith: Damages\n";
    assert_eq!(batch(&with_head([0, 0, 0, 0], block)), Outcome::Applied);
    let refused = r#"{"base":"BASE_TOKEN","ops":[{"op":"replace","ref":"p999","with":"y"}]}"#;
    assert_eq!(batch(&with_head([0, 0, 0, 0], refused)), Outcome::Refused);
}

#[test]
fn what_is_not_a_batch_is_refused_and_that_is_not_a_failure() {
    for text in [
        "",
        "{",
        "null",
        r#"{"base":"BASE_TOKEN","ops":[]}"#,
        r#"{"base":"BASE_TOKEN","ops":[{"op":"nope"}]}"#,
        r#"{"base":"BASE_TOKEN","ops":[{"op":"replace","ref":"p999","with":"y"}]}"#,
        r#"{"base":"BASE_TOKEN","ops":[{"op":"delete","ref":"t1"}],"extra":1}"#,
        "base: BASE_TOKEN\n---\n@replace\nref: p1\nwith: <<END\nnever closed",
        "\u{FEFF}base: x\r\n",
    ] {
        assert_ne!(
            batch(&with_head([0, 0, 0, 0], text)),
            Outcome::Applied,
            "{text:?}"
        );
    }
    assert_eq!(batch(&[0, 0, 0, 0, 0xFF, 0xFE]), Outcome::Skipped);
}

#[test]
fn the_readable_file_passes_whole_spliced_and_appended() {
    let texts = [
        "",
        "p1 The Supplier shall indemnify the Customer.\n",
        "{++inserted++}",
        "{--",
        "| a | b |\n",
        "p4 Payment is due within sixty days.\n",
        "\u{1F600}\t\u{201C}",
    ];
    for text in texts {
        for how in 0..3u8 {
            for mode in [0u8, 1, 2] {
                files(&with_head([how, 0, 7, mode], text));
            }
        }
        files(&with_head([1, 0xFF, 0xFF, 0], text));
    }
    // The file as it was, with one paragraph changed, applies: the checks above ran on edits that did something.
    let doc = crate::document::Document::open(
        super::document::docx(),
        &crate::api::options::OpenOptions::default(),
    )
    .expect("opens");
    let md = doc
        .files()
        .expect("files")
        .files
        .into_iter()
        .find(|(n, _)| n == "document.md")
        .expect("document.md")
        .1;
    let changed = md.replace("thirty", "sixty");
    assert_ne!(changed, md);
    assert_eq!(files(&with_head([0, 0, 0, 0], &changed)), Outcome::Applied);
    assert_eq!(files(&with_head([0, 0, 0, 2], &changed)), Outcome::Applied);
}
