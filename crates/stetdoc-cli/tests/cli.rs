//! The `trycmd` cases of the `stetdoc` binary: `tests/cmd/*.toml` (one command each), `tests/cmd/*.trycmd` (sessions), and
//! the examples of the CLI reference in `doc/*.md`, which `docs/reference/cli.md` includes, so every example in it runs.
//!
//! A document may not be committed, so each case's `<name>.in/` gets `contract.docx` and `revised.docx`, built by the
//! testkit, before the cases run, and an empty `<name>.out/` so that every case runs in a sandbox. Both are ignored by git.
//! The date of every stamp is fixed with `STETDOC_DATE`. To update the expected output of a case after an intended
//! change, run `TRYCMD=overwrite cargo test -p stetdoc --test cli` and review the diff.
//!
//! The console blocks of `docs/quickstart.md` run too (`quickstart`), with the same conventions, on the testkit's
//! `contract::package()`, which is the same bytes as the sample a reader downloads, `site/public/samples/contract.docx`.
//! `site_sample_is_the_fixture` keeps that file equal to the fixture. The page is copied into a sandbox under
//! `CARGO_TARGET_TMPDIR`, with the fixture as its `quickstart.in/contract.docx`, and `TRYCMD=overwrite` writes that copy,
//! not the page. The home and config folders are the sandbox's, so `stetdoc init` touches nothing of the person running
//! the tests, and the saved author is Jane Doe, the name the page's examples sign with. The install and download commands
//! (`curl`, `brew`, `cargo` and the like) are skipped. While the page is missing, the test says so and passes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
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

/// The commands of the quickstart that are not run: installing stetdoc, downloading the sample, opening the result in Word.
const SKIPPED: [&str; 10] = [
    "curl", "wget", "brew", "cargo", "npm", "npx", "pipx", "uv", "open", "start",
];

/// The repository root, two levels above this crate's manifest.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf()
}

#[test]
fn quickstart() {
    run_page(
        &repo_root().join("docs/quickstart.md"),
        &contract::package(),
    );
}

/// The downloadable sample is the testkit fixture, byte for byte. Set `STETDOC_UPDATE_SAMPLE=1` to write the fixture to
/// the sample: `STETDOC_UPDATE_SAMPLE=1 cargo test -p stetdoc --test cli site_sample_is_the_fixture`.
#[test]
fn site_sample_is_the_fixture() {
    let path = repo_root().join("site/public/samples/contract.docx");
    let fixture = contract::package();
    if std::env::var("STETDOC_UPDATE_SAMPLE").as_deref() == Ok("1") {
        fs::write(&path, &fixture).unwrap();
        return;
    }
    let regenerate = "regenerate it with `STETDOC_UPDATE_SAMPLE=1 cargo test -p stetdoc --test cli site_sample_is_the_fixture`";
    let sample = fs::read(&path)
        .unwrap_or_else(|e| panic!("{} cannot be read ({e}): {regenerate}", path.display()));
    assert!(
        sample == fixture,
        "{} is not stetdoc_testkit::fixtures::contract::package(): {regenerate}",
        path.display()
    );
}

/// Runs the console blocks of a page on a sample `contract.docx` (the fixture's bytes), in a sandbox of its own, or says
/// why it cannot.
fn run_page(page: &Path, fixture: &[u8]) {
    if !page.exists() {
        eprintln!(
            "skipping the examples of {}: the page does not exist",
            page.display()
        );
        return;
    }
    let name = page.file_stem().unwrap().to_str().unwrap();
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("page-{name}"));
    let _ = fs::remove_dir_all(&dir);
    let input = dir.join(format!("{name}.in"));
    let home = dir.join("home");
    for d in [
        &input,
        &dir.join(format!("{name}.out")),
        &home.join(".config"),
    ] {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(input.join("contract.docx"), fixture).unwrap();
    fs::create_dir_all(home.join(".config/stetdoc")).unwrap();
    fs::write(
        home.join(".config/stetdoc/config.toml"),
        "author = \"Jane Doe\"\n",
    )
    .unwrap();
    let copy = dir.join(format!("{name}.md"));
    fs::copy(page, &copy).unwrap();
    eprintln!(
        "running the examples of {} from {}",
        page.display(),
        copy.display()
    );
    let cases = trycmd::TestCases::new();
    cases
        .register_bin("stetdoc", Path::new(env!("CARGO_BIN_EXE_stetdoc")))
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        .env("HOME", home.display().to_string())
        .env(
            "XDG_CONFIG_HOME",
            home.join(".config").display().to_string(),
        );
    for bin in SKIPPED {
        cases.register_bin(bin, trycmd::schema::Bin::Ignore);
    }
    cases.case(copy);
}
