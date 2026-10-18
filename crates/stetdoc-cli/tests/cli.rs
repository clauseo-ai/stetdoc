//! The `trycmd` cases of the `stetdoc` binary: `tests/cmd/*.toml` (one command each), `tests/cmd/*.trycmd` (sessions), and
//! the examples of the CLI reference in `doc/*.md`, which `docs/reference/cli.md` includes, so every example in it runs.
//!
//! A document may not be committed, so each case's `<name>.in/` gets `contract.docx` and `revised.docx`, built by the
//! testkit, before the cases run, and an empty `<name>.out/` so that every case runs in a sandbox. Both are ignored by git.
//! The date of every stamp is fixed with `STETDOC_DATE`. To update the expected output of a case after an intended
//! change, run `TRYCMD=overwrite cargo test -p stetdoc --test cli` and review the diff.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use stetdoc_testkit::fixtures::{contract, parts, zip::Zip};

/// The revised version of the contract: two words changed in `p4` and a sentence added to `p5`.
fn revised() -> Vec<u8> {
    let body = contract::BODY.replace("thirty days", "sixty days").replace(
        "governed by English law.",
        "governed by English law. The courts of London decide.",
    );
    let doc = parts::Doc::new().body(&body).bytes();
    Zip::new(parts::entries_doc(&doc)).build()
}

/// A document with four tracked changes by two authors, one in each paragraph: an insertion by Jane Doe (`#1`), a deletion
/// by Eric White (`#2`), a format change by Jane Doe (`#3`) and an insertion by Eric White (`#4`).
fn tracked() -> Vec<u8> {
    let body = r#"<w:p><w:r><w:t xml:space="preserve">The Seller </w:t></w:r><w:ins w:id="1" w:author="Jane Doe" w:date="2026-01-02T10:00:00Z"><w:r><w:t xml:space="preserve">shall pay </w:t></w:r></w:ins></w:p><w:p><w:r><w:t xml:space="preserve">Price </w:t></w:r><w:del w:id="2" w:author="Eric White" w:date="2026-01-03T10:00:00Z"><w:r><w:delText xml:space="preserve">gross</w:delText></w:r></w:del><w:r><w:t xml:space="preserve"> net</w:t></w:r></w:p><w:p><w:r><w:rPr><w:b/><w:rPrChange w:id="3" w:author="Jane Doe" w:date="2026-01-04T10:00:00Z"><w:rPr/></w:rPrChange></w:rPr><w:t xml:space="preserve">Bold now</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">Governing law: </w:t></w:r><w:ins w:id="4" w:author="Eric White" w:date="2026-01-05T10:00:00Z"><w:r><w:t xml:space="preserve">English law</w:t></w:r></w:ins></w:p>"#;
    let doc = parts::Doc::new().body(body).bytes();
    Zip::new(parts::entries_doc(&doc)).build()
}

/// Gives every case its fixtures and a sandbox.
fn stage(root: &Path, extensions: &[&str]) {
    let contract = contract::package();
    let revised = revised();
    let tracked = tracked();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !extensions.contains(&ext) {
            continue;
        }
        let input = path.with_extension("in");
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("contract.docx"), &contract).unwrap();
        fs::write(input.join("revised.docx"), &revised).unwrap();
        fs::write(input.join("tracked.docx"), &tracked).unwrap();
        fs::create_dir_all(path.with_extension("out")).unwrap();
    }
}

#[test]
fn cli_cases() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.join("tests/cmd");
    let doc = crate_dir.join("doc");
    stage(&root, &["toml", "trycmd"]);
    stage(&doc, &["md"]);
    trycmd::TestCases::new()
        .register_bin("stetdoc", Path::new(env!("CARGO_BIN_EXE_stetdoc")))
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        // No saved author: the config file of the person running the tests must not count.
        .env(
            "XDG_CONFIG_HOME",
            crate_dir.join("tests/cmd/no-config").display().to_string(),
        )
        .case(root.join("*.toml"))
        .case(root.join("*.trycmd"))
        .case(doc.join("*.md"));
}
