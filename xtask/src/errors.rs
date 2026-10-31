//! The error table and the generated error reference.
//!
//! `stetdoc_core::ErrorCode` is the one place error codes live. Two checks keep the
//! rest honest:
//! - the **error-table drift check** fails when the Errors table in `ARCHITECTURE.md`
//!   and the enum disagree about a code, its exit class, an exit class label, or the
//!   order the enum promises;
//! - the **generated-docs drift check** fails when `docs/reference/errors.md`, which
//!   `cargo xtask docs` writes from the enum, is stale.
//!
//! The enum is read from source (see [`crate::enumspec`]), never linked.

use crate::arch::{self, ErrorRow};
use crate::enumspec::{self, CodeSpec, ENUM_FILE, ErrorSpecs};
use crate::repo::{Repo, Res};
use crate::violation::Violation;
use std::collections::BTreeSet;

/// The generated reference.
pub const REFERENCE: &str = "docs/reference/errors.md";

fn specs(repo: &Repo) -> Res<Result<ErrorSpecs, Violation>> {
    Ok(enumspec::parse(&repo.read(ENUM_FILE)?))
}

/// The error-table drift check.
pub fn check_table(repo: &Repo) -> Res<Vec<Violation>> {
    let md = repo.read(arch::FILE)?;
    let rows = match arch::parse_error_table(&md) {
        Ok(rows) => rows,
        Err(problems) => return Ok(problems),
    };
    match specs(repo)? {
        Ok(specs) => Ok(compare(&rows, &specs)),
        Err(problem) => Ok(vec![problem]),
    }
}

/// Compares the table with the enum's specs.
pub fn compare(rows: &[ErrorRow], specs: &ErrorSpecs) -> Vec<Violation> {
    let mut problems = Vec::new();
    let mut listed: Vec<(&str, &ErrorRow)> = Vec::new();
    let mut seen_classes: BTreeSet<u8> = BTreeSet::new();
    for row in rows {
        if !seen_classes.insert(row.number) {
            problems.push(Violation::new(
                arch::FILE,
                row.line,
                format!("exit class {} has more than one row", row.number),
                "merge the rows: one row per exit class",
            ));
        }
        for code in &row.codes {
            if let Some((_, first)) = listed.iter().find(|(c, _)| c == code) {
                problems.push(Violation::new(
                    arch::FILE,
                    row.line,
                    format!(
                        "`{code}` is listed twice in the Errors table (also on line {})",
                        first.line
                    ),
                    "list each code under exactly one exit class",
                ));
            } else {
                listed.push((code.as_str(), row));
            }
        }
    }
    for spec in &specs.codes {
        match listed.iter().find(|(c, _)| *c == spec.wire) {
            None => problems.push(Violation::new(
                ENUM_FILE,
                spec.line,
                format!("the code `{}` is not in the Errors table in ARCHITECTURE.md", spec.wire),
                format!("add `{}` to the exit class {} row of the Errors table", spec.wire, spec.exit),
            )),
            Some((_, row)) if row.number != spec.exit => problems.push(Violation::new(
                arch::FILE,
                row.line,
                format!(
                    "`{}` is listed under exit class {}, but the enum says exit class {}",
                    spec.wire, row.number, spec.exit
                ),
                format!(
                    "move it to the exit class {} row. Codes and their exit classes are frozen from 1.0, so do not change the enum to match the table once released",
                    spec.exit
                ),
            )),
            Some(_) => {}
        }
    }
    for (name, row) in &listed {
        if !specs.codes.iter().any(|s| s.wire == *name) {
            problems.push(Violation::new(
                arch::FILE,
                row.line,
                format!("the Errors table lists `{name}`, which is not an ErrorCode"),
                format!("remove it, or add the entry to the `error_codes!` list in {ENUM_FILE}"),
            ));
        }
    }
    for row in rows {
        match specs.classes.iter().find(|c| c.number == row.number) {
            None => problems.push(Violation::new(
                arch::FILE,
                row.line,
                format!("exit class {} does not exist in ExitClass", row.number),
                "use the exit classes ExitClass defines",
            )),
            Some(class) if class.label != row.label => problems.push(Violation::new(
                arch::FILE,
                row.line,
                format!(
                    "exit class {} is labelled \"{}\" in the table but \"{}\" in ExitClass::label",
                    row.number, row.label, class.label
                ),
                format!("make the table say \"{} {}\"", class.number, class.label),
            )),
            Some(_) => {}
        }
    }
    problems.extend(check_order(rows, specs));
    problems
}

/// `ErrorCode::ALL` is documented as being in the order of the Errors table.
fn check_order(rows: &[ErrorRow], specs: &ErrorSpecs) -> Option<Violation> {
    let in_table: Vec<&str> = rows
        .iter()
        .flat_map(|r| r.codes.iter().map(String::as_str))
        .collect();
    let in_enum: Vec<&CodeSpec> = specs
        .codes
        .iter()
        .filter(|c| in_table.contains(&c.wire.as_str()))
        .collect();
    let in_table: Vec<&str> = in_table
        .into_iter()
        .filter(|c| specs.codes.iter().any(|s| s.wire == *c))
        .collect();
    let at = in_enum
        .iter()
        .zip(&in_table)
        .position(|(e, t)| e.wire != *t)?;
    Some(Violation::new(
        ENUM_FILE,
        in_enum[at].line,
        format!(
            "`{}` is out of order: the Errors table has `{}` at this position, and ErrorCode::ALL promises the order of the table",
            in_enum[at].wire, in_table[at]
        ),
        "reorder the `error_codes!` list to follow the table, exit class by exit class",
    ))
}

/// Renders the error reference from the enum.
pub fn render_reference(specs: &ErrorSpecs) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<!-- Generated by `cargo xtask docs` from `{ENUM_FILE}`. Do not edit by hand. -->\n\n"
    ));
    out.push_str("# Error reference\n\n");
    out.push_str(
        "Every failure is a `StetError`: a stable code, a one-line message, a `next` hint and,\n\
         for batches, the index of the failing operation. The CLI, `--json` output and the\n\
         library all use these codes. From 1.0, codes and their exit classes are frozen. New\n\
         codes may be added, and existing codes are never repurposed or removed outside a major\n\
         version.\n\n",
    );
    out.push_str("## Exit classes\n\n");
    out.push_str("Exit status 0 is success. CLI argument errors also exit with 2. They belong to the CLI and are not codes.\n\n");
    out.push_str("| Exit | Class | Codes |\n|---|---|---|\n");
    for class in &specs.classes {
        let codes: Vec<String> = specs
            .codes
            .iter()
            .filter(|c| c.exit == class.number)
            .map(|c| format!("`{}`", c.wire))
            .collect();
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            class.number,
            class.label,
            codes.join(", ")
        ));
    }
    out.push_str("\n## Codes\n");
    for code in &specs.codes {
        out.push_str(&format!(
            "\n### `{}`\n\nExit {} ({}). {}\n\n**Next:** {}\n",
            code.wire, code.exit, code.label, code.doc, code.next
        ));
    }
    out
}

/// Every generated document: path relative to the root, and its content.
pub fn generated_docs(repo: &Repo) -> Res<Vec<(&'static str, String)>> {
    let specs = specs(repo)?.map_err(|v| v.to_string())?;
    let ops =
        crate::opspec::parse(&repo.read(crate::opspec::SPEC_FILE)?).map_err(|v| v.to_string())?;
    let findings = crate::findingspec::parse(&repo.read(crate::findingspec::SPEC_FILE)?)
        .map_err(|v| v.to_string())?;
    Ok(vec![
        (REFERENCE, render_reference(&specs)),
        (crate::opspec::REFERENCE, crate::opspec::render(&ops)),
        (
            crate::findingspec::REFERENCE,
            crate::findingspec::render(&findings),
        ),
    ])
}

/// The generated-docs drift check.
pub fn check_generated(repo: &Repo) -> Res<Vec<Violation>> {
    let specs = match specs(repo)? {
        Ok(specs) => specs,
        Err(problem) => return Ok(vec![problem]),
    };
    let ops = match crate::opspec::parse(&repo.read(crate::opspec::SPEC_FILE)?) {
        Ok(ops) => ops,
        Err(problem) => return Ok(vec![problem]),
    };
    let findings = match crate::findingspec::parse(&repo.read(crate::findingspec::SPEC_FILE)?) {
        Ok(findings) => findings,
        Err(problem) => return Ok(vec![problem]),
    };
    Ok(compare_generated(
        repo,
        &[
            (REFERENCE, render_reference(&specs)),
            (crate::opspec::REFERENCE, crate::opspec::render(&ops)),
            (
                crate::findingspec::REFERENCE,
                crate::findingspec::render(&findings),
            ),
        ],
    ))
}

/// Compares files on disk with their expected content.
pub fn compare_generated(repo: &Repo, expected: &[(&str, String)]) -> Vec<Violation> {
    let mut problems = Vec::new();
    for (path, content) in expected {
        match repo.read(path) {
            Err(_) => problems.push(Violation::new(
                *path,
                0,
                "this generated file is missing",
                "run `cargo xtask docs` and commit the result",
            )),
            Ok(on_disk) if on_disk != *content => {
                let line = on_disk
                    .lines()
                    .zip(content.lines())
                    .position(|(a, b)| a != b)
                    .unwrap_or_else(|| on_disk.lines().count().min(content.lines().count()))
                    + 1;
                problems.push(Violation::new(
                    *path,
                    line,
                    "this generated file is out of date",
                    "run `cargo xtask docs` and commit the result; never edit generated files by hand",
                ));
            }
            Ok(_) => {}
        }
    }
    problems
}

/// `cargo xtask docs`: writes every generated document.
pub fn write_generated(repo: &Repo) -> Res<Vec<&'static str>> {
    let mut written = Vec::new();
    for (path, content) in generated_docs(repo)? {
        repo.write(path, &content)?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enumspec::tests::real;
    use crate::repo::testutil::repo as temp_repo;

    fn table_for(specs: &ErrorSpecs) -> String {
        let mut md = String::from("## Errors\n\n| Exit class | Codes |\n|---|---|\n");
        for class in &specs.classes {
            let codes: Vec<String> = specs
                .codes
                .iter()
                .filter(|s| s.exit == class.number)
                .map(|s| format!("`{}`", s.wire))
                .collect();
            md.push_str(&format!(
                "| {} {} | {} |\n",
                class.number,
                class.label,
                codes.join(", ")
            ));
        }
        md
    }

    fn violations_for(md: &str, specs: &ErrorSpecs) -> Vec<Violation> {
        let rows = arch::parse_error_table(md).expect("table parses");
        compare(&rows, specs)
    }

    #[test]
    fn a_table_generated_from_the_enum_agrees_with_it() {
        let specs = real();
        assert_eq!(violations_for(&table_for(&specs), &specs), []);
    }

    #[test]
    fn a_code_missing_from_the_table_fails_at_the_enum() {
        let specs = real();
        let md = table_for(&specs).replace("`unsupported`, ", "");
        let found = violations_for(&md, &specs);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].file, ENUM_FILE);
        assert!(found[0].line > 1);
        assert!(found[0].message.contains("`unsupported`"));
    }

    #[test]
    fn a_code_under_the_wrong_exit_class_fails_at_the_table_line() {
        let specs = real();
        let md = table_for(&specs)
            .replace("`unsupported`, ", "")
            .replace("| 2 usage | ", "| 2 usage | `unsupported`, ");
        let found = violations_for(&md, &specs);
        assert!(
            found.iter().any(|v| {
                v.file == arch::FILE
                    && v.message.contains("exit class 2")
                    && v.message.contains("enum says exit class 1")
                    && v.fix.contains("frozen from 1.0")
            }),
            "{found:?}"
        );
    }

    #[test]
    fn a_code_the_enum_lacks_fails() {
        let specs = real();
        let md = table_for(&specs).replace("| 3 target | ", "| 3 target | `made_up`, ");
        let found = violations_for(&md, &specs);
        assert!(
            found.iter().any(|v| v.message.contains("`made_up`")),
            "{found:?}"
        );
    }

    #[test]
    fn a_wrong_exit_class_label_fails() {
        let specs = real();
        let md = table_for(&specs).replace("3 target", "3 goal");
        let found = violations_for(&md, &specs);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("labelled"));
    }

    #[test]
    fn a_duplicate_code_or_exit_class_row_fails() {
        let specs = real();
        let md = table_for(&specs).replace("| 3 target | ", "| 3 target | `unsupported`, ");
        assert!(!violations_for(&md, &specs).is_empty());
        let twice = format!("{}| 3 target | `other` |\n", table_for(&specs));
        let found = violations_for(&twice, &specs);
        assert!(
            found
                .iter()
                .any(|v| v.message.contains("more than one row")),
            "{found:?}"
        );
    }

    #[test]
    fn the_enum_order_must_follow_the_table() {
        let specs = real();
        let mut swapped = specs.clone();
        swapped.codes.swap(0, 1);
        let found = violations_for(&table_for(&specs), &swapped);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("out of order"));
        assert_eq!(found[0].file, ENUM_FILE);
    }

    #[test]
    fn check_table_reads_the_real_enum_from_a_repository() {
        let specs = real();
        let source = Repo::discover()
            .and_then(|r| r.read(ENUM_FILE))
            .expect("read");
        let (_dir, repo) = temp_repo(&[(arch::FILE, &table_for(&specs)), (ENUM_FILE, &source)]);
        assert_eq!(check_table(&repo).expect("runs"), []);
        let (_dir, repo) = temp_repo(&[
            (arch::FILE, &table_for(&specs).replace("`not_found`, ", "")),
            (ENUM_FILE, &source),
        ]);
        assert_eq!(check_table(&repo).expect("runs").len(), 1);
    }

    #[test]
    fn an_unreadable_enum_file_is_reported_not_a_panic() {
        let (_dir, repo) = temp_repo(&[
            (
                arch::FILE,
                "## Errors\n\n| Exit class | Codes |\n|---|---|\n",
            ),
            (ENUM_FILE, "fn broken( {"),
        ]);
        let found = check_table(&repo).expect("runs");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("cannot parse"));
    }

    #[test]
    fn the_reference_lists_every_code_with_its_hint() {
        let specs = real();
        let doc = render_reference(&specs);
        for code in &specs.codes {
            assert!(
                doc.contains(&format!("### `{}`", code.wire)),
                "{}",
                code.wire
            );
            assert!(doc.contains(&code.next), "{}", code.wire);
        }
        assert!(doc.contains(
            "| 4 | stale | `stale_ref`, `stale_files`, `pending_edits`, `stale_target` |"
        ));
    }

    #[test]
    fn a_stale_or_missing_generated_file_fails() {
        let specs = real();
        let source = Repo::discover()
            .and_then(|r| r.read(ENUM_FILE))
            .expect("read");
        let spec_source = Repo::discover()
            .and_then(|r| r.read(crate::opspec::SPEC_FILE))
            .expect("read");
        let finding_source = Repo::discover()
            .and_then(|r| r.read(crate::findingspec::SPEC_FILE))
            .expect("read");
        let (_dir, repo) = temp_repo(&[
            (ENUM_FILE, &source),
            (crate::opspec::SPEC_FILE, &spec_source),
            (crate::findingspec::SPEC_FILE, &finding_source),
        ]);
        let expected = generated_docs(&repo).expect("render");
        let missing = compare_generated(&repo, &expected);
        assert_eq!(missing.len(), 3);
        assert!(missing[0].fix.contains("cargo xtask docs"));

        write_generated(&repo).expect("write");
        assert_eq!(compare_generated(&repo, &expected), []);
        assert_eq!(check_generated(&repo).expect("runs"), []);

        let stale = render_reference(&specs).replace("### `not_found`", "### `not_found_edited`");
        repo.write(REFERENCE, &stale).expect("write");
        let found = compare_generated(&repo, &expected);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, REFERENCE);
        assert!(found[0].line > 1);
        assert!(found[0].message.contains("out of date"));
        assert_eq!(check_generated(&repo).expect("runs").len(), 1);
    }
}
