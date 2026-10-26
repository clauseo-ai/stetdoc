//! The edit checks: L2 to L5 on random edit batches made from the view, over the fixtures and the curated corpus.
//!
//! Each run picks a seed and prints it (`laws::random::seed`); `STETDOC_SEED` runs the same batches again, `STETDOC_BATCHES` sets
//! how many each document gets (50 by default) and `STETDOC_FILE_EDITS` how many random edits of files (10 by default).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use stetdoc_core::{Batch, Document, Edit, ErrorCode, Limits, OpenOptions, Stamp, capabilities};
use stetdoc_testkit::fixtures::{self, contract};
use stetdoc_testkit::laws::generate::{
    Facts, GenOp, Generated, NAMES, Rng, generate, kept_note_marks,
};
use stetdoc_testkit::laws::random::{self, Base, batch_count, check, seed};
use stetdoc_testkit::laws::random_files;
use stetdoc_testkit::laws::sweep::{in_parallel, over_the_corpus, survey_of_the_corpus};

/// The random runs are for a release build: `cargo xtask check` runs this file again with `--release`, and a debug build keeps the
/// fixed smoke set of `laws_identity.rs`. A debug run says that it skips them.
fn debug_build() -> bool {
    if cfg!(debug_assertions) {
        eprintln!(
            "skipped in a debug build: cargo xtask check runs the random laws with --release"
        );
        return true;
    }
    false
}

#[test]
fn every_operation_has_a_generator() {
    let ops: Vec<String> = capabilities().ops.iter().map(|o| o.name.clone()).collect();
    for name in &ops {
        assert!(
            NAMES.contains(&name.as_str()),
            "the operation {name} has no generator"
        );
    }
    for name in NAMES {
        assert!(ops.iter().any(|o| o == name), "{name} is not an operation");
    }
    assert_eq!(NAMES.len(), 19);
}

#[test]
fn the_generator_is_seeded_and_covers_every_operation_both_ways() {
    // The rich contract with a comment of the author's and a change and a comment of another's, so that every operation has
    // a target that exists.
    let mut doc = Document::open(contract::package_rich(), &OpenOptions::default()).unwrap();
    for (author, ops) in [
        ("Clauseo", r#"{"op":"comment","ref":"p1","text":"Mine."}"#),
        (
            "Other",
            r#"{"op":"comment","ref":"p2","text":"Theirs."},{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
        ),
    ] {
        let batch = Batch::parse(
            &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
            &Limits::default(),
        )
        .unwrap();
        doc.edit(Edit::ops(batch, Stamp::new(author, "2026-10-06T10:00:00")))
            .unwrap();
    }
    let facts = Facts::of(&doc).unwrap();
    assert!(
        facts
            .comments
            .iter()
            .any(|(_, a)| a.as_deref() == Some("Clauseo"))
    );
    assert!(!facts.revisions.is_empty() && facts.parts.contains(&"word/document.xml".to_string()));
    assert!(!facts.paras.is_empty() && !facts.tables.is_empty() && !facts.notes.is_empty());
    let draw = |seed| {
        let mut rng = Rng::new(seed);
        (0..3000)
            .map(|_| generate(&mut rng, &facts))
            .collect::<Vec<_>>()
    };
    let batches = draw(7);
    assert_eq!(
        batches.iter().map(Generated::to_string).collect::<Vec<_>>(),
        draw(7).iter().map(Generated::to_string).collect::<Vec<_>>(),
        "the same seed gives the same batches"
    );
    assert_ne!(batches[0].to_string(), draw(8)[0].to_string());
    for name in NAMES {
        let of = |pred: &dyn Fn(&Generated, &GenOp) -> bool| {
            batches
                .iter()
                .any(|b| b.ops.iter().any(|o| o.name == *name && pred(b, o)))
        };
        assert!(
            of(&|b, o| b.tracked && o.refused.is_none()),
            "{name} tracked"
        );
        assert!(
            of(&|b, o| !b.tracked && o.refused.is_none()),
            "{name} untracked"
        );
        assert!(of(&|_, o| o.refused.is_some()), "{name} refused");
    }
    assert!(batches.iter().any(|b| b.ops.len() == 1));
    assert!(batches.iter().any(|b| b.ops.len() == 4));
}

#[test]
fn an_operation_that_must_be_refused_and_goes_through_fails_with_the_batch() {
    let bytes = contract::package();
    let ok = GenOp {
        name: "replace",
        json: r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#.to_string(),
        refused: None,
        sticks: None,
    };
    let base = Base::new(bytes.clone()).unwrap();
    let good = Generated {
        ops: vec![ok.clone()],
        tracked: true,
    };
    check(&base, &good).unwrap();
    let wrong = Generated {
        ops: vec![GenOp {
            refused: Some(&[ErrorCode::InvalidPayload]),
            ..ok.clone()
        }],
        tracked: true,
    };
    let e = check(&base, &wrong).unwrap_err();
    assert!(e.contains("must be refused"), "{e}");
    // A refusal with another code than the operation names fails too.
    let missing = Generated {
        ops: vec![GenOp {
            json: r#"{"op":"replace","ref":"p4","with":"x","colour":"red"}"#.to_string(),
            refused: Some(&[ErrorCode::NotFound]),
            ..ok.clone()
        }],
        tracked: false,
    };
    let e = check(&base, &missing).unwrap_err();
    assert!(e.contains("refused as"), "{e}");
    // A plain replace that does not stick is caught: the text it says it wrote is not the text the view shows.
    let lying = Generated {
        ops: vec![GenOp {
            json: r#"{"op":"replace","ref":"p5","with":"Governed by Scots law"}"#.to_string(),
            sticks: Some(("p5".to_string(), vec!["Something else".to_string()])),
            ..ok
        }],
        tracked: true,
    };
    let e = check(&base, &lying).unwrap_err();
    assert!(e.contains("L3"), "{e}");
}

#[test]
fn a_replace_of_a_note_paragraph_keeps_the_notes_own_number_for_l3() {
    // The rich contract's note paragraph is the note's own number (U+0003) and its text; a whole-paragraph replace keeps the
    // number, tracked and untracked (design `revisions.md`, 3.6.4), and L3 expects it. Found by seed 3404882508102420448 on
    // tdf129659.docx.
    let doc = Document::open(contract::package_rich(), &OpenOptions::default()).unwrap();
    let note = Facts::of(&doc)
        .unwrap()
        .paras
        .into_iter()
        .find(|p| p.text.first() == Some(&'\u{3}'))
        .expect("the note paragraph");
    let with = "days Customer shall shall liability {braces} and \\ backslash";
    assert_eq!(
        kept_note_marks("\u{3} Scots law is excluded.", with),
        format!("\u{3}{with}")
    );
    assert_eq!(kept_note_marks("a\u{3}b", "c"), "c\u{3}");
    assert_eq!(kept_note_marks("\u{3}a", "\u{3}c"), "\u{3}c");
    let base = Base::new(contract::package_rich()).unwrap();
    let op = |sticks: String| GenOp {
        name: "replace",
        json: format!(
            r#"{{"op":"replace","ref":"{}","with":{}}}"#,
            note.r,
            serde_json::to_string(with).unwrap()
        ),
        refused: None,
        sticks: Some((note.r.clone(), vec![sticks])),
    };
    for tracked in [true, false] {
        let kept = Generated {
            ops: vec![op(format!("\u{3}{with}"))],
            tracked,
        };
        check(&base, &kept).unwrap();
        let dropped = Generated {
            ops: vec![op(with.to_string())],
            tracked,
        };
        let e = check(&base, &dropped).unwrap_err();
        assert!(e.contains("L3"), "{e}");
    }
}

#[test]
fn random_batches_on_the_contract_keep_their_laws() {
    if debug_build() {
        return;
    }
    for bytes in [contract::package(), contract::package_rich()] {
        random::batches_with(&bytes, seed(), batch_count().max(200)).unwrap();
        random_files::file_edits_with(&bytes, seed(), 20).unwrap();
    }
}

#[test]
fn random_batches_on_every_fixture_that_opens_keep_their_laws() {
    if debug_build() {
        return;
    }
    let all: Vec<_> = fixtures::all().into_iter().collect();
    let n = (batch_count() / 5).max(5);
    let failures = in_parallel(&all, |f| {
        let bytes = f.bytes();
        if Document::open(bytes.clone(), &OpenOptions::default()).is_err() {
            return None;
        }
        random::batches_with(&bytes, seed(), n)
            .err()
            .map(|why| format!("{}: {why}", f.name))
    });
    assert!(
        failures.is_empty(),
        "{} failures (STETDOC_SEED={} runs them again):\n{}",
        failures.len(),
        seed(),
        failures.join("\n")
    );
}

#[test]
fn random_batches_on_the_curated_documents_keep_their_laws() {
    if debug_build() {
        return;
    }
    let seed = seed();
    let Some(swept) = over_the_corpus("random batches", random::batches) else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    if let Err(e) = swept.unwrap().verdict(200) {
        panic!(
            "{e}\nSTETDOC_SEED={seed} runs the same batches again, and STETDOC_ONLY=<a piece of the document's id> runs only that document"
        );
    }
}

#[test]
fn random_edits_of_files_on_the_curated_documents_keep_their_laws() {
    if debug_build() {
        return;
    }
    let seed = seed();
    let Some(swept) = over_the_corpus("random edits of files", random_files::file_edits) else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    if let Err(e) = swept.unwrap().verdict(200) {
        panic!(
            "{e}\nSTETDOC_SEED={seed} runs the same edits again, and STETDOC_ONLY=<a piece of the document's id> runs only that document"
        );
    }
}

#[test]
fn the_body_of_most_curated_documents_lines_up_with_the_view_for_l2_by_block() {
    use stetdoc_testkit::laws::blocks::lines_up;
    let Some(swept) = survey_of_the_corpus("L2 by block", |bytes| {
        let doc =
            Document::open(bytes.to_vec(), &OpenOptions::default()).map_err(|e| e.to_string())?;
        let facts = Facts::of(&doc)?;
        if lines_up(bytes, &facts) {
            Ok(())
        } else {
            Err(format!(
                "does not line up: {} top-level blocks in the view",
                facts.top.len()
            ))
        }
    }) else {
        return;
    };
    let swept = swept.unwrap();
    let missed = swept.failures.len();
    eprintln!(
        "L2 by block runs on {} of {} documents",
        swept.checked - missed,
        swept.checked
    );
    if let Some(problem) = swept.reach().or_else(|| swept.slow()) {
        panic!("{problem}");
    }
    assert!(
        missed * 10 <= swept.checked,
        "only {} of {} documents line up:\n{}",
        swept.checked - missed,
        swept.checked,
        swept.failures.join("\n")
    );
}

#[test]
fn a_block_the_batch_does_not_name_must_keep_its_xml() {
    use stetdoc_testkit::laws::blocks::per_block;
    let bytes = contract::package();
    let doc = Document::open(bytes.clone(), &OpenOptions::default()).unwrap();
    let facts = Facts::of(&doc).unwrap();
    let edited = |ops: &str| {
        let mut doc = Document::open(bytes.clone(), &OpenOptions::default()).unwrap();
        let batch = Batch::parse(
            &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
            &Limits::default(),
        )
        .unwrap();
        doc.edit(Edit::ops(
            batch,
            Stamp::new("Clauseo", "2026-10-06T10:00:00"),
        ))
        .unwrap();
        doc.save().unwrap().bytes
    };
    let p1 = r#"{"op":"replace","ref":"p1","with":"Changed."}"#;
    let g = Generated {
        ops: vec![GenOp {
            name: "replace",
            json: p1.to_string(),
            refused: None,
            sticks: None,
        }],
        tracked: true,
    };
    per_block(&bytes, &edited(p1), &facts, &g).unwrap();
    // The same batch, against a package in which p5 changed instead: p5 is named as the block that changed.
    let other = edited(r#"{"op":"replace","ref":"p5","with":"Changed."}"#);
    let e = per_block(&bytes, &other, &facts, &g).unwrap_err();
    assert!(e.contains("L2: p5"), "{e}");
}

#[test]
fn accepting_a_merge_across_rebound_prefixes_keeps_their_meaning() {
    // The fixture binds a prefix to different namespaces in sibling paragraphs. Accepting a tracked merge moves the second
    // paragraph's content into the first, and the xml editor declares the prefix on what it moves, where it was refused before
    // (seed 16493474821369967218, batch 2 of the fixture run).
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "xml-ns-prefix-rebound-in-siblings")
        .unwrap();
    let bytes = f.bytes();
    let mut doc = Document::open(bytes.clone(), &OpenOptions::default()).unwrap();
    let run = |doc: &mut Document, ops: &str| {
        let batch = Batch::parse(
            &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
            &Limits::default(),
        )
        .unwrap();
        doc.edit(Edit::ops(
            batch,
            Stamp::new("Clauseo", "2026-10-06T10:00:00"),
        ))
        .unwrap()
    };
    let input: Vec<_> = doc.check().by_code.iter().map(|(c, _)| *c).collect();
    run(&mut doc, r#"{"op":"merge","ref":"p1","second":"p2"}"#);
    run(&mut doc, r#"{"op":"accept","author":"Clauseo"}"#);
    // The fixture's q:a elements are in a namespace the schema does not know, before and after: nothing new.
    let after: Vec<_> = doc.check().by_code.iter().map(|(c, _)| *c).collect();
    assert!(
        after.iter().all(|c| input.contains(c)),
        "{input:?} then {after:?}"
    );
    let saved = doc.save().unwrap().bytes;
    Document::open(saved, &OpenOptions::default()).unwrap();
    // The seed's batch no longer stops at the move. What it meets next, while revisions writes the paragraph's w:pPr after an
    // element the schema does not know (so the reader misses the deleted mark), is the revisions module's to fix.
    if let Err(e) = random::batches_with(&bytes, 16_493_474_821_369_967_218, 10) {
        assert!(!e.contains("would mean something else"), "{e}");
    }
}

#[test]
fn a_field_that_a_pending_deletion_leaves_alone_is_collapsed_in_the_final_view_as_after_accepting()
{
    // Complex 3.docx, seed 303, batch 40: an index field spans paragraphs, and the text before its start in the first one is
    // deleted, tracked. The Final view collapsed the field only once the deletion was accepted, so accepting changed it (L5).
    let field = |kind: &str| format!(r#"<w:r><w:fldChar w:fldCharType="{kind}"/></w:r>"#);
    let body = format!(
        r#"<w:p><w:r><w:t>Before.</w:t></w:r></w:p><w:p><w:r><w:t>Index item here</w:t></w:r>{}<w:r><w:instrText xml:space="preserve"> INDEX </w:instrText></w:r>{}</w:p><w:p><w:r><w:t>Index item here; 7</w:t></w:r></w:p><w:p><w:r><w:t>Other item; 9</w:t></w:r>{}</w:p><w:p><w:r><w:t>After.</w:t></w:r></w:p>"#,
        field("begin"),
        field("separate"),
        field("end")
    );
    let bytes = fixtures::zip::Zip::new(fixtures::parts::entries_doc(
        &fixtures::parts::Doc::new().body(&body).bytes(),
    ))
    .build();
    let delete = r#"{"op":"delete","ref":"p2","start":0,"end":15}"#;
    let g = Generated {
        ops: vec![GenOp {
            name: "delete",
            json: delete.to_string(),
            refused: None,
            sticks: None,
        }],
        tracked: true,
    };
    check(&Base::new(bytes.clone()).unwrap(), &g).unwrap();
    // At the interface: the pending deletion's Final view is the accepted one, and Redline keeps the field open, so that the
    // deletion stays in view.
    let final_view = |doc: &Document| {
        doc.view(&stetdoc_core::ViewSpec {
            scope: stetdoc_core::Scope::Story(stetdoc_core::StoryName::Body),
            mode: stetdoc_core::Mode::Final,
            budget: stetdoc_core::Budget(u32::MAX),
            ..stetdoc_core::ViewSpec::default()
        })
        .unwrap()
        .text
    };
    let mut doc = Document::open(bytes, &OpenOptions::default()).unwrap();
    assert!(
        !final_view(&doc).contains("{opaque index"),
        "{}",
        final_view(&doc)
    );
    let run = |doc: &mut Document, ops: &str| {
        let batch = Batch::parse(
            &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
            &Limits::default(),
        )
        .unwrap();
        doc.edit(Edit::ops(
            batch,
            Stamp::new("Clauseo", "2026-10-06T10:00:00"),
        ))
        .unwrap();
    };
    run(&mut doc, delete);
    let pending = final_view(&doc);
    assert!(pending.contains("{opaque index"), "{pending}");
    let redline = doc
        .view(&stetdoc_core::ViewSpec {
            scope: stetdoc_core::Scope::Story(stetdoc_core::StoryName::Body),
            mode: stetdoc_core::Mode::Redline,
            budget: stetdoc_core::Budget(u32::MAX),
            ..stetdoc_core::ViewSpec::default()
        })
        .unwrap()
        .text;
    assert!(redline.contains("{--Index item here--}"), "{redline}");
    run(&mut doc, r#"{"op":"accept","author":"Clauseo"}"#);
    assert!(final_view(&doc).contains("{opaque index"));
}
