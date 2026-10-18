//! The part of the testkit that the core's own tests use exposes plain data only.
//!
//! The core's tests may depend on this crate for the corpus and the fixtures. That compiles the
//! core twice, once under test and once as this crate's dependency, so a core type in the API
//! would be two different types. `corpus`, `expect` and `fixtures` therefore name no core type;
//! the one file that looks at the core is `codes.rs`, and it hands out `&'static str`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn names_the_core(path: &Path) -> bool {
    let needle = concat!("stetdoc", "_core");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .any(|l| l.contains(needle))
}

#[test]
fn only_codes_rs_names_the_core_in_the_modules_the_core_tests_use() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![src.join("corpus.rs"), src.join("expect.rs")];
    rust_files(&src.join("fixtures"), &mut files);
    assert!(files.len() > 10, "{files:?}");
    for f in &files {
        assert!(
            !names_the_core(f),
            "{} names the core: keep core types out of the testkit API, and look at the core only in codes.rs",
            f.display()
        );
    }
    assert!(names_the_core(&src.join("codes.rs")));
}
