use super::*;
use crate::repo::testutil::repo;

const TABLE: &str = "### Allowed dependencies\n\n| Module | May depend on |\n|---|---|\n| `api` | nothing |\n| `xml` | `api` |\n| `schema` | `api`, `xml` |\n| `ops` | `api`, `xml` |\n| `document` | any of the above |\n";

fn table() -> AllowedDeps {
    arch::parse_allowed_deps(TABLE).expect("table parses")
}

fn run_with(files: &[(&str, &str)], io: &[(&str, &str)]) -> Vec<Violation> {
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|(p, t)| (format!("{CORE_SRC}/{p}"), t.to_string()))
        .collect();
    let text: String = io
        .iter()
        .map(|(p, r)| format!("{CORE_SRC}/{p} | {r}\n"))
        .collect();
    let (io, problems) = exceptions::parse("io.txt", "why", &text);
    assert_eq!(problems, []);
    check_sources(&table(), &sources, &io)
}

fn run(files: &[(&str, &str)]) -> Vec<Violation> {
    run_with(files, &[])
}

/// Every module of the table present, so only the file under test can fail.
fn with_modules(extra: &[(&str, &str)]) -> Vec<Violation> {
    with_modules_io(extra, &[])
}

fn with_modules_io(extra: &[(&str, &str)], io: &[(&str, &str)]) -> Vec<Violation> {
    let mut files = vec![
        ("api/mod.rs", ""),
        ("xml/mod.rs", ""),
        ("schema/mod.rs", ""),
        ("ops/mod.rs", ""),
        ("document/mod.rs", ""),
    ];
    files.retain(|(p, _)| !extra.iter().any(|(e, _)| e == p));
    files.extend_from_slice(extra);
    run_with(&files, io)
}

#[test]
fn allowed_paths_pass() {
    let src = "use crate::api::error::StetError;\nuse crate::xml::{self, tree::SpanTree};\nfn f() -> crate::api::X { crate::ops::helper() }\n";
    assert_eq!(with_modules(&[("ops/diff.rs", src)]), []);
}

#[test]
fn a_use_statement_that_is_not_allowed_fails_with_file_and_line() {
    let src = "use crate::api::E;\nuse crate::schema::order_of;\n";
    let found = with_modules(&[("xml/tree.rs", src)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].file, "crates/stetdoc-core/src/xml/tree.rs");
    assert_eq!(found[0].line, 2);
    assert!(
        found[0]
            .message
            .contains("`xml` may not depend on `schema`")
    );
    assert!(found[0].fix.contains("Allowed dependencies"));
}

#[test]
fn a_grouped_use_is_checked_per_leaf() {
    let src = "use crate::{\n    api::E,\n    ops::apply,\n    schema::{self, order_of},\n};\n";
    let found = with_modules(&[("xml/tree.rs", src)]);
    let lines: Vec<usize> = found.iter().map(|v| v.line).collect();
    assert_eq!(lines, [3, 4], "{found:?}");
}

#[test]
fn an_inline_path_fails() {
    let src = "fn f() {\n    let x = crate::ops::apply();\n}\nstruct S(crate::ops::T);\n";
    let found = with_modules(&[("xml/tree.rs", src)]);
    let lines: Vec<usize> = found.iter().map(|v| v.line).collect();
    assert_eq!(lines, [2, 4]);
}

#[test]
fn a_path_inside_a_macro_invocation_fails() {
    let src = "fn f() {\n    let v = vec![crate::ops::apply()];\n    println!(\"{}\", crate::ops::N);\n}\n";
    let found = with_modules(&[("xml/tree.rs", src)]);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].line, 2);
}

#[test]
fn a_path_inside_a_macro_definition_fails() {
    let src = "macro_rules! m {\n    () => { use crate::ops::{apply, other}; };\n}\nmacro_rules! n {\n    () => { $crate::ops::apply() };\n}\n";
    let found = with_modules(&[("xml/tree.rs", src)]);
    let lines: Vec<usize> = found.iter().map(|v| v.line).collect();
    assert_eq!(lines, [2, 5], "{found:?}");
}

#[test]
fn super_paths_resolve_to_the_module_they_name() {
    let ok = "use super::xml::tree::SpanTree;\n";
    assert_eq!(with_modules(&[("ops/mod.rs", ok)]), []);
    let bad = "use super::document::Document;\n";
    let found = with_modules(&[("ops/mod.rs", bad)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .message
            .contains("`ops` may not depend on `document`")
    );
    assert_eq!(with_modules(&[("ops/diff.rs", "use super::helper;\n")]), []);
    let up = "use super::super::document::Document;\n";
    assert_eq!(with_modules(&[("ops/diff.rs", up)]).len(), 1);
}

#[test]
fn self_before_super_is_resolved_too() {
    let src = "fn f() { let _ = self::super::super::document::X; }\n";
    let found = with_modules(&[("ops/diff.rs", src)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .message
            .contains("`ops` may not depend on `document`")
    );
    let use_form = "use self::super::super::document::X;\n";
    assert_eq!(with_modules(&[("ops/diff.rs", use_form)]).len(), 1);
    assert_eq!(with_modules(&[("ops/diff.rs", "use self::helper;\n")]), []);
}

#[test]
fn super_inside_an_inline_test_module_stays_in_the_module() {
    let src = "#[cfg(test)]\nmod tests {\n    use super::*;\n    use super::super::api::E;\n}\n";
    assert_eq!(with_modules(&[("xml/tree.rs", src)]), []);
}

#[test]
fn using_a_crate_root_re_export_fails() {
    let found = with_modules(&[("xml/tree.rs", "use crate::StetError;\n")]);
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("names no module"), "{found:?}");
    assert!(found[0].fix.contains("crate::api"));
}

#[test]
fn document_may_use_everything_and_api_nothing() {
    assert_eq!(
        with_modules(&[(
            "document/mod.rs",
            "use crate::ops::apply;\nuse crate::xml::X;\n"
        )]),
        []
    );
    assert_eq!(
        with_modules(&[("api/error.rs", "use crate::xml::X;\n")]).len(),
        1
    );
}

#[test]
fn restricted_visibility_paths_are_checked() {
    assert_eq!(
        with_modules(&[("xml/tree.rs", "pub(in crate::ops) struct S;\n")]).len(),
        1
    );
    assert_eq!(
        with_modules(&[("xml/tree.rs", "pub(crate) struct S;\n")]),
        []
    );
}

fn assert_root_alias(file: &str, src: &str, expect_line: usize) {
    let found = with_modules(&[(file, src)]);
    assert_eq!(found.len(), 1, "{src}: {found:?}");
    assert_eq!(found[0].line, expect_line, "{src}");
    assert!(
        found[0].message.contains("aliases or globs the crate root"),
        "{src}: {found:?}"
    );
    assert_eq!(
        found[0].fix,
        "name the module path; do not alias or glob the crate root"
    );
}

#[test]
fn aliasing_the_crate_root_fails() {
    assert_root_alias("xml/tree.rs", "use crate as c;\n", 1);
}

#[test]
fn aliasing_self_in_a_crate_group_fails() {
    assert_root_alias("xml/tree.rs", "use crate::{self as top};\n", 1);
}

#[test]
fn extern_crate_self_fails() {
    assert_root_alias("xml/tree.rs", "extern crate self as me;\n", 1);
}

#[test]
fn globbing_the_crate_root_fails() {
    assert_root_alias("xml/tree.rs", "use crate::*;\n", 1);
    assert_root_alias("xml/tree.rs", "use crate::{*};\n", 1);
}

#[test]
fn a_glob_that_reaches_the_root_through_super_fails() {
    assert_root_alias("ops/diff.rs", "use super::super::*;\n", 1);
    assert_root_alias("ops/mod.rs", "use super::*;\n", 1);
    // From an inline test module, `super::*` is the enclosing module, not the root.
    assert_eq!(
        with_modules(&[("ops/mod.rs", "mod t { use super::*; }\n")]),
        []
    );
    assert_eq!(with_modules(&[("ops/diff.rs", "use super::*;\n")]), []);
}

#[test]
fn aliases_inside_macros_fail_too() {
    assert_root_alias(
        "xml/tree.rs",
        "macro_rules! m { () => { use crate as c; }; }\n",
        1,
    );
    assert_root_alias(
        "xml/tree.rs",
        "macro_rules! m { () => { use crate::*; }; }\n",
        1,
    );
    assert_root_alias(
        "xml/tree.rs",
        "macro_rules! m { () => { extern crate self as me; }; }\n",
        1,
    );
    assert_root_alias(
        "xml/tree.rs",
        "macro_rules! m { () => { use crate::{self as top}; }; }\n",
        1,
    );
}

#[test]
fn a_module_glob_or_rename_is_an_ordinary_dependency() {
    assert_eq!(
        with_modules(&[("ops/diff.rs", "use crate::xml::*;\nuse crate::api as a;\n")]),
        []
    );
    let found = with_modules(&[(
        "xml/tree.rs",
        "use crate::ops::*;\nuse crate::schema as s;\n",
    )]);
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn std_file_system_network_process_env_and_clock_are_forbidden() {
    for (src, shown) in [
        ("use std::fs;\n", "std::fs"),
        ("use std::fs::File;\n", "std::fs::File"),
        ("use std::{env, io};\n", "std::env"),
        (
            "fn f() { let _ = std::net::TcpStream::connect(\"x\"); }\n",
            "std::net::TcpStream::connect",
        ),
        ("use std::process::Command;\n", "std::process::Command"),
        (
            "use std::time::{Duration, Instant};\n",
            "std::time::Instant",
        ),
        ("use std::time::SystemTime;\n", "std::time::SystemTime"),
        ("use std::time;\n", "std::time"),
        (
            "use std::os::unix::fs::symlink;\n",
            "std::os::unix::fs::symlink",
        ),
        (
            "fn f() { let _ = std::time::UNIX_EPOCH.elapsed(); }\n",
            "std::time::UNIX_EPOCH",
        ),
        ("use std::io::StdoutLock;\n", "std::io::StdoutLock"),
        (
            "fn f() { let _ = vec![std::fs::read(\"x\")]; }\n",
            "std::fs::read",
        ),
    ] {
        let found = with_modules(&[("xml/tree.rs", src)]);
        assert_eq!(found.len(), 1, "{src}: {found:?}");
        assert!(found[0].message.contains(shown), "{src}: {found:?}");
        assert!(found[0].fix.contains("core-io-exceptions"));
    }
    assert_eq!(
        with_modules(&[(
            "xml/tree.rs",
            "use std::time::Duration;\nuse std::io::Read;\nuse std::time::TryFromFloatSecsError;\n"
        )]),
        []
    );
}

fn assert_std_alias(src: &str, shown: &str) {
    let found = with_modules(&[("xml/tree.rs", src)]);
    assert_eq!(found.len(), 1, "{src}: {found:?}");
    assert_eq!(found[0].line, 1, "{src}");
    assert!(
        found[0]
            .message
            .contains(&format!("`{shown}` is aliased or globbed")),
        "{src}: {found:?}"
    );
    assert!(
        found[0].fix.contains("do not alias or glob"),
        "{src}: {found:?}"
    );
}

#[test]
fn aliasing_std_fails() {
    assert_std_alias(
        "use std as s;\nfn f() { let _ = s::fs::read(\"x\"); }\n",
        "std",
    );
}

#[test]
fn aliasing_std_time_through_self_fails() {
    assert_std_alias(
        "use std::time::{self as t};\nfn f() { let _ = t::SystemTime::now(); }\n",
        "std::time",
    );
    assert_std_alias("use std::time as t;\n", "std::time");
}

#[test]
fn renaming_std_with_extern_crate_fails() {
    assert_std_alias("extern crate std as s;\n", "std");
}

#[test]
fn importing_std_time_as_itself_fails() {
    assert_std_alias("use std::time::{self};\n", "std::time");
}

#[test]
fn globbing_std_or_std_time_fails() {
    assert_std_alias("use std::*;\n", "std");
    assert_std_alias("use std::{*};\n", "std");
    assert_std_alias("use std::time::*;\n", "std::time");
}

#[test]
fn renaming_or_globbing_std_io_fails_but_ordinary_io_imports_do_not() {
    assert_std_alias("use std::io as i;\n", "std::io");
    assert_std_alias("use std::io::{self as i};\n", "std::io");
    assert_std_alias("use std::io::*;\n", "std::io");
    let ok = "use std::io::{self, Read, Write};\nuse std::io;\nuse std::io::prelude::*;\nuse std::collections::*;\n";
    assert_eq!(with_modules(&[("xml/tree.rs", ok)]), []);
}

#[test]
fn std_aliases_inside_macros_fail_too() {
    assert_std_alias("macro_rules! m { () => { use std as s; }; }\n", "std");
    assert_std_alias("macro_rules! m { () => { use std::*; }; }\n", "std");
    assert_std_alias(
        "macro_rules! m { () => { use std::time::{self as t}; }; }\n",
        "std::time",
    );
    assert_std_alias(
        "macro_rules! m { () => { use std::time::*; }; }\n",
        "std::time",
    );
}

#[test]
fn the_standard_streams_are_forbidden() {
    for (src, shown) in [
        ("fn f() { let _ = std::io::stdin(); }\n", "std::io::stdin"),
        ("use std::io::stdout;\n", "std::io::stdout"),
        ("use std::io::{self, stderr};\n", "std::io::stderr"),
        ("fn f(_: std::io::Stdin) {}\n", "std::io::Stdin"),
        (
            "macro_rules! m { () => { std::io::stdout() }; }\n",
            "std::io::stdout",
        ),
    ] {
        let found = with_modules(&[("xml/tree.rs", src)]);
        assert_eq!(found.len(), 1, "{src}: {found:?}");
        assert!(found[0].message.contains(shown), "{src}: {found:?}");
    }
    assert_eq!(
        with_modules(&[("xml/tree.rs", "use std::io::{Read, Write, Result};\n")]),
        []
    );
}

#[test]
fn a_std_alias_is_not_excused_by_an_io_exception() {
    let src = "#[cfg(test)]\nmod tests {\n    use std as s;\n    fn f() { let _ = s::fs::read(\"x\"); }\n}\n";
    let io = [("xml/tree.rs", "reads the corpus")];
    let found = with_modules_io(&[("xml/tree.rs", src)], &io);
    assert!(
        found
            .iter()
            .any(|v| v.message.contains("aliased or globbed")),
        "{found:?}"
    );
}

#[test]
fn std_io_in_a_test_module_is_still_forbidden_without_an_exception() {
    let src = "#[cfg(test)]\nmod tests {\n    fn read() { let _ = std::fs::read(\"x\"); }\n}\n";
    let found = with_modules(&[("xml/tree.rs", src)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 3);
}

#[test]
fn a_listed_file_may_use_std_io_in_test_code_only() {
    let test_only = "#[cfg(test)]\nmod tests {\n    fn read() { let _ = std::fs::read(\"x\"); }\n}\n#[test]\nfn t() { let _ = std::fs::read(\"y\"); }\n";
    let io = [("xml/tree.rs", "reads the corpus")];
    assert_eq!(with_modules_io(&[("xml/tree.rs", test_only)], &io), []);
    let library = "pub fn load() { let _ = std::fs::read(\"x\"); }\n";
    let found = with_modules_io(&[("xml/tree.rs", library)], &io);
    assert!(
        found
            .iter()
            .any(|v| v.message.contains("not allowed in the core")),
        "{found:?}"
    );
}

#[test]
fn an_unused_or_misplaced_io_exception_fails() {
    let io = [("xml/tree.rs", "reads the corpus")];
    let found = with_modules_io(&[("xml/tree.rs", "fn f() {}\n")], &io);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].file, IO_EXCEPTIONS);
    assert!(found[0].message.contains("no test code"));
    let gone = with_modules_io(&[], &[("xml/gone.rs", "x")]);
    assert!(
        gone.iter().any(|v| v.message.contains("not a source file")),
        "{gone:?}"
    );
}

#[test]
fn lib_rs_may_hold_only_docs_attributes_mod_declarations_and_pub_use() {
    let good = "//! docs\n#![deny(missing_docs)]\nmod xml;\n#[cfg(test)]\nmod api;\npub use api::error::E;\n";
    assert_eq!(with_modules(&[("lib.rs", good)]).len(), 0);
    let bad = "mod xml;\nmod sneaky {\n    use crate::ops::apply;\n}\nuse crate::api::E;\nfn helper() {}\n";
    let found = with_modules(&[("lib.rs", bad)]);
    let lines: Vec<usize> = found.iter().map(|v| v.line).collect();
    assert_eq!(lines, [2, 5, 6], "{found:?}");
    assert!(found[0].message.contains("lib.rs holds only"));
}

#[test]
fn lib_rs_makes_public_only_the_fuzz_module_behind_the_fuzzing_feature() {
    let fuzz = "#[cfg(feature = \"fuzzing\")]\npub mod fuzz;\n";
    assert_eq!(with_modules(&[("lib.rs", fuzz)]), []);
    let private = "#[cfg(feature = \"fuzzing\")]\nmod fuzz;\n";
    assert_eq!(
        with_modules(&[("lib.rs", private)]),
        [],
        "a private module is always fine"
    );
    for (what, bad) in [
        ("no cfg", "pub mod fuzz;\n"),
        (
            "another feature",
            "#[cfg(feature = \"other\")]\npub mod fuzz;\n",
        ),
        ("a test cfg", "#[cfg(test)]\npub mod fuzz;\n"),
        (
            "another module",
            "#[cfg(feature = \"fuzzing\")]\npub mod xml;\n",
        ),
        ("pub(crate)", "pub(crate) mod xml;\n"),
        ("pub xml", "pub mod xml;\n"),
    ] {
        let found = with_modules(&[("lib.rs", bad)]);
        assert_eq!(found.len(), 1, "{what}: {found:?}");
        assert!(found[0].message.contains("lib.rs holds only"), "{what}");
    }
}

#[test]
fn a_module_directory_without_a_table_row_fails() {
    let found = with_modules(&[("extra/mod.rs", "")]);
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("`extra` has no row"));
}

#[test]
fn a_table_row_without_a_module_fails_at_the_table_line() {
    let found = run(&[
        ("api/mod.rs", ""),
        ("xml/mod.rs", ""),
        ("schema/mod.rs", ""),
        ("document/mod.rs", ""),
    ]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].file, "ARCHITECTURE.md");
    assert!(found[0].message.contains("`ops`"));
}

#[test]
fn a_syntax_error_is_reported_with_its_line() {
    let found = with_modules(&[("xml/tree.rs", "fn ok() {}\nfn broken( {\n")]);
    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("cannot parse"), "{found:?}");
    assert!(found[0].line >= 2);
}

#[test]
fn check_reads_the_table_sources_and_exceptions_from_a_repository() {
    let src = "use crate::ops::apply;\n";
    let (_dir, repo) = repo(&[
        ("ARCHITECTURE.md", TABLE),
        ("crates/stetdoc-core/src/lib.rs", "mod xml;\n"),
        ("crates/stetdoc-core/src/api/mod.rs", ""),
        ("crates/stetdoc-core/src/xml/mod.rs", src),
        ("crates/stetdoc-core/src/schema/mod.rs", ""),
        ("crates/stetdoc-core/src/ops/mod.rs", ""),
        ("crates/stetdoc-core/src/document/mod.rs", ""),
        (IO_EXCEPTIONS, "crates/stetdoc-core/src/xml/mod.rs\n"),
    ]);
    let found = check(&repo).expect("runs");
    assert!(
        found
            .iter()
            .any(|v| v.line == 1 && v.message.contains("may not depend")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|v| v.file == IO_EXCEPTIONS && v.message.contains("no reason")),
        "{found:?}"
    );
}
