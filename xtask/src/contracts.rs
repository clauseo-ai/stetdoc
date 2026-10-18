//! The module-contract check.
//!
//! Each core module's `mod.rs` must open with a `//!` contract that has the sections
//! `ARCHITECTURE.md` promises, and its "Depends on" section must match the Allowed
//! dependencies table, so the two cannot drift apart.

use crate::arch::{self, AllowedDeps, Deps, backticked};
use crate::layering::CORE_SRC;
use crate::repo::{Repo, Res};
use crate::violation::Violation;
use std::collections::BTreeSet;

/// The sections every contract has, in the form they are written.
pub const SECTIONS: &[&str] = &[
    "**Owns.**",
    "**Interface.**",
    "**Invariants.**",
    "**Depends on.**",
    "**Start reading.**",
];

/// Runs the check against the repository.
pub fn check(repo: &Repo) -> Res<Vec<Violation>> {
    let md = repo.read(arch::FILE)?;
    let table = match arch::parse_allowed_deps(&md) {
        Ok(table) => table,
        Err(problems) => return Ok(problems),
    };
    let mut problems = Vec::new();
    for module in table.modules() {
        let path = format!("{CORE_SRC}/{module}/mod.rs");
        match repo.read(&path) {
            Ok(text) => {
                let exists = |rel: &str| repo.exists(&format!("{CORE_SRC}/{rel}"));
                problems.extend(check_contract(&table, module, &path, &text, &exists));
            }
            Err(_) => problems.push(Violation::new(
                path,
                0,
                format!("module `{module}` has no mod.rs"),
                "create it, opening with the module's `//!` contract",
            )),
        }
    }
    Ok(problems)
}

/// The text of a section: its first line and the lines after it up to a blank line.
fn section(doc: &[(usize, &str)], marker: &str) -> Option<(usize, String)> {
    let start = doc.iter().position(|(_, l)| l.starts_with(marker))?;
    let mut text = String::new();
    for (_, l) in &doc[start..] {
        if l.is_empty() {
            break;
        }
        text.push_str(l);
        text.push(' ');
    }
    Some((doc[start].0, text))
}

fn describe(deps: &Deps) -> String {
    match deps {
        Deps::Nothing => "nothing".to_string(),
        Deps::Any => "any module".to_string(),
        Deps::Only(set) => set
            .iter()
            .map(|d| format!("`{d}`"))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Checks one module's `mod.rs` text. `exists` says whether a path relative to the
/// core's `src/` exists.
pub fn check_contract(
    table: &AllowedDeps,
    module: &str,
    path: &str,
    text: &str,
    exists: &dyn Fn(&str) -> bool,
) -> Vec<Violation> {
    let mut problems = Vec::new();
    let doc: Vec<(usize, &str)> = text
        .lines()
        .enumerate()
        .take_while(|(_, l)| l.starts_with("//!"))
        .map(|(i, l)| (i + 1, l.trim_start_matches("//!").trim()))
        .collect();
    if doc.is_empty() {
        return vec![Violation::new(
            path,
            1,
            format!("module `{module}` does not open with a `//!` contract"),
            "start the file with `//!` lines: Owns, Interface, Invariants, Depends on, Start reading",
        )];
    }
    for marker in SECTIONS {
        if !doc.iter().any(|(_, l)| l.starts_with(marker)) {
            problems.push(Violation::new(
                path,
                1,
                format!("the contract of `{module}` has no {marker} section"),
                format!("add a `//! {marker} ...` paragraph, as the other modules have"),
            ));
        }
    }
    if let Some((line, body)) = section(&doc, "**Depends on.**") {
        let claimed = if body.contains("any module") {
            Deps::Any
        } else if backticked(&body).is_empty() && body.contains("nothing") {
            Deps::Nothing
        } else {
            Deps::Only(backticked(&body).into_iter().collect::<BTreeSet<_>>())
        };
        if let Some(expected) = table.row(module).map(|r| &r.deps)
            && *expected != claimed
        {
            problems.push(Violation::new(
                path,
                line,
                format!(
                    "\"Depends on\" in the contract of `{module}` disagrees with the Allowed dependencies table: the table expects {}, the contract lists {}",
                    describe(expected),
                    describe(&claimed)
                ),
                "make the two match; the table in ARCHITECTURE.md is the source of truth",
            ));
        }
    }
    if let Some((line, body)) = section(&doc, "**Start reading.**") {
        let missing: Vec<String> = backticked(&body)
            .into_iter()
            .filter(|p| p.ends_with(".rs") && !p.contains('<') && !exists(p))
            .collect();
        if !missing.is_empty() {
            problems.push(Violation::new(
                path,
                line,
                format!(
                    "`{module}` says to start reading {}, which does not exist",
                    missing
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                "name a file that exists, or fix the path",
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::testutil::repo;

    const TABLE: &str = "### Allowed dependencies\n\n| Module | May depend on |\n|---|---|\n| `api` | nothing |\n| `xml` | `api` |\n| `document` | any of the above |\n";

    fn contract(depends: &str) -> String {
        format!(
            "//! Title.\n//!\n//! **Owns.** x.\n//!\n//! **Interface.** y.\n//!\n//! **Invariants.**\n//! - z\n//!\n//! **Depends on.** {depends}\n//!\n//! **Start reading.** `a`.\n"
        )
    }

    fn run(module: &str, text: &str) -> Vec<Violation> {
        let table = arch::parse_allowed_deps(TABLE).expect("table");
        check_contract(&table, module, "p/mod.rs", text, &|path| {
            path == "api/mod.rs"
        })
    }

    #[test]
    fn a_complete_contract_with_matching_dependencies_passes() {
        assert_eq!(run("api", &contract("nothing.")), []);
        assert_eq!(run("xml", &contract("`api`.")), []);
        assert_eq!(run("document", &contract("any module in the core.")), []);
    }

    #[test]
    fn a_file_with_no_contract_fails() {
        let found = run("xml", "pub fn f() {}\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("does not open with"));
    }

    #[test]
    fn a_missing_section_fails() {
        let text = contract("`api`.").replace("//! **Owns.** x.\n//!\n", "");
        let found = run("xml", &text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("**Owns.**"));
    }

    #[test]
    fn dependencies_that_disagree_with_the_table_fail_at_the_section_line() {
        let found = run("xml", &contract("`api`, `document`."));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 10);
        assert!(
            found[0]
                .message
                .contains("the table expects `api`, the contract lists `api`, `document`")
        );
        let found = run("api", &contract("`xml`."));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_start_reading_file_that_does_not_exist_fails() {
        let text = contract("`api`.").replace(
            "**Start reading.** `a`.",
            "**Start reading.** `xml/tree.rs`.",
        );
        let found = run("xml", &text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("`xml/tree.rs`"));
        assert!(found[0].fix.contains("exists"));
        let exists = contract("`api`.").replace(
            "**Start reading.** `a`.",
            "**Start reading.** `api/mod.rs`.",
        );
        assert_eq!(run("xml", &exists), []);
        let pattern = contract("`api`.").replace(
            "**Start reading.** `a`.",
            "**Start reading.** `ops/<name>.rs`.",
        );
        assert_eq!(run("xml", &pattern), []);
    }

    #[test]
    fn check_reports_a_module_without_a_mod_rs() {
        let (_dir, repo) = repo(&[
            (arch::FILE, TABLE),
            ("crates/stetdoc-core/src/api/mod.rs", &contract("nothing.")),
            ("crates/stetdoc-core/src/xml/mod.rs", &contract("`api`.")),
        ]);
        let found = check(&repo).expect("runs");
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("`document` has no mod.rs"));
    }
}
