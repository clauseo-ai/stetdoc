//! The dependency-table check.
//!
//! Reads the Allowed dependencies table in `ARCHITECTURE.md` and fails on any
//! `crate::` path in `stetdoc-core` that the table does not allow. Paths are found
//! with `syn`, not a regular expression (see [`finder`]). The check also fails on:
//! - an alias or glob of the crate root, which would hide paths from it;
//! - `std` file, network, process, environment, standard stream and clock APIs anywhere
//!   in the core, unless a test is listed in `xtask/core-io-exceptions.txt` with a reason,
//!   and aliases or globs of `std`, `std::time` and `std::io`, which would hide them.
//!   The check works on paths. Methods such as `Path::exists`, `read_dir` and `metadata`
//!   touch the file system without naming `std::fs`, so a reviewer checks for those;
//! - a `lib.rs` that holds anything but crate docs, attributes, `mod x;`, `pub use` and the one
//!   public module, `#[cfg(feature = "fuzzing")] pub mod fuzz;`.

mod finder;
#[cfg(test)]
mod tests;

use crate::arch::{self, AllowedDeps, Deps};
use crate::exceptions::{self, Entry};
use crate::repo::{Repo, Res};
use crate::violation::Violation;
use finder::{Finder, Kind};
use std::collections::{BTreeMap, BTreeSet};
use syn::visit::Visit;

/// Where the core's source lives.
pub const CORE_SRC: &str = "crates/stetdoc-core/src";

/// Tests that may use `std` I/O, one `path | reason` per line.
pub const IO_EXCEPTIONS: &str = "xtask/core-io-exceptions.txt";

/// Runs the check against the repository.
pub fn check(repo: &Repo) -> Res<Vec<Violation>> {
    let md = repo.read(arch::FILE)?;
    let table = match arch::parse_allowed_deps(&md) {
        Ok(table) => table,
        Err(problems) => return Ok(problems),
    };
    let sources: Vec<(String, String)> = repo
        .files()?
        .into_iter()
        .filter(|f| f.starts_with(CORE_SRC) && f.ends_with(".rs"))
        .map(|f| repo.read(&f).map(|text| (f, text)))
        .collect::<Res<_>>()?;
    let text = if repo.exists(IO_EXCEPTIONS) {
        repo.read(IO_EXCEPTIONS)?
    } else {
        String::new()
    };
    let (io, mut problems) =
        exceptions::parse(IO_EXCEPTIONS, "why this test may use std I/O", &text);
    problems.extend(check_sources(&table, &sources, &io));
    Ok(problems)
}

/// Checks `(path, source)` pairs, with paths relative to the repository root.
pub fn check_sources(
    table: &AllowedDeps,
    sources: &[(String, String)],
    io: &BTreeMap<String, Entry>,
) -> Vec<Violation> {
    let mut problems = Vec::new();
    let mut present: BTreeSet<String> = BTreeSet::new();
    let mut used_io: BTreeSet<&str> = BTreeSet::new();
    for (path, text) in sources {
        let Some(rel) = path
            .strip_prefix(CORE_SRC)
            .map(|r| r.trim_start_matches('/'))
        else {
            continue;
        };
        if rel == "lib.rs" {
            problems.extend(check_lib_rs(path, text));
            continue;
        }
        let Some(mod_path) = module_path(rel) else {
            continue;
        };
        let Some(top) = mod_path.first().cloned() else {
            continue;
        };
        present.insert(top.clone());
        let file = match syn::parse_file(text) {
            Ok(file) => file,
            Err(err) => {
                problems.push(Violation::new(
                    path,
                    err.span().start().line,
                    format!("cannot parse this file: {err}"),
                    "fix the syntax error so the dependency check can read the file",
                ));
                continue;
            }
        };
        let mut finder = Finder::new(mod_path);
        finder.visit_file(&file);
        let mut seen = BTreeSet::new();
        for found in finder.found {
            if !seen.insert((found.line, found.target.clone(), found.kind as u8)) {
                continue;
            }
            match found.kind {
                Kind::RootAlias => problems.push(Violation::new(
                    path,
                    found.line,
                    format!(
                        "`{}` aliases or globs the crate root, which hides module paths from the dependency check",
                        found.shown
                    ),
                    "name the module path; do not alias or glob the crate root",
                )),
                Kind::StdAlias => problems.push(Violation::new(
                    path,
                    found.line,
                    format!(
                        "`{}` is aliased or globbed, which hides std file, network, process, environment, stream and clock paths from the check",
                        found.shown
                    ),
                    "name the items you use by their full path, for example `std::time::Duration`; do not alias or glob `std`, `std::time` or `std::io`",
                )),
                Kind::Io => {
                    if found.in_test && io.contains_key(path) {
                        used_io.insert(path);
                    } else {
                        problems.push(Violation::new(
                            path,
                            found.line,
                            format!(
                                "`{}` is not allowed in the core, which does no file, network, process, environment or clock access",
                                found.shown
                            ),
                            format!(
                                "take the data as bytes or arguments instead (ARCHITECTURE.md, cross-cutting rule 6). A test that must read a file may be listed, with a reason, in {IO_EXCEPTIONS}"
                            ),
                        ));
                    }
                }
                Kind::Path if found.target == top => {}
                Kind::Path => {
                    if table.row(&found.target).is_none() {
                        problems.push(Violation::new(
                            path,
                            found.line,
                            format!(
                                "`{}` names no module of the core (`{}`), so the layering cannot be checked",
                                found.shown, found.target
                            ),
                            "reach the item through its module, for example `crate::api::...`, instead of a re-export at the crate root",
                        ));
                    } else if !table.allows(&top, &found.target) {
                        problems.push(not_allowed(table, path, found.line, &top, &found.target, &found.shown));
                    }
                }
            }
        }
    }
    for module in &present {
        if table.row(module).is_none() {
            problems.push(Violation::new(
                format!("{CORE_SRC}/{module}"),
                0,
                format!("module `{module}` has no row in the Allowed dependencies table"),
                "add a row for it to ARCHITECTURE.md (Allowed dependencies and Module contracts), or move the code into an existing module",
            ));
        }
    }
    for row in &table.rows {
        if !present.contains(&row.module) {
            problems.push(Violation::new(
                arch::FILE,
                row.line,
                format!(
                    "the table lists `{}`, but {CORE_SRC}/{} does not exist",
                    row.module, row.module
                ),
                "create the module with its `//!` contract, or remove its row",
            ));
        }
    }
    for (path, entry) in io {
        if !used_io.contains(path.as_str()) {
            let known = sources.iter().any(|(p, _)| p == path);
            problems.push(Violation::new(
                IO_EXCEPTIONS,
                entry.line,
                if known {
                    format!(
                        "`{path}` is listed ({}), but no test code in it uses std I/O any more",
                        entry.reason
                    )
                } else {
                    format!("`{path}` is listed, but is not a source file of the core")
                },
                "remove the entry",
            ));
        }
    }
    problems
}

fn not_allowed(
    table: &AllowedDeps,
    path: &str,
    line: usize,
    top: &str,
    target: &str,
    shown: &str,
) -> Violation {
    Violation::new(
        path,
        line,
        format!("module `{top}` may not depend on `{target}` (found `{shown}`)"),
        format!(
            "`{top}` may depend on {}. Use one of those, move the shared code to a lower module, or change the Allowed dependencies table in ARCHITECTURE.md together with the code and say why",
            allowed_list(table, top)
        ),
    )
}

fn allowed_list(table: &AllowedDeps, module: &str) -> String {
    match table.row(module).map(|r| &r.deps) {
        Some(Deps::Any) => "any module".to_string(),
        Some(Deps::Only(set)) if !set.is_empty() => set
            .iter()
            .map(|d| format!("`{d}`"))
            .collect::<Vec<_>>()
            .join(", "),
        _ => "nothing else".to_string(),
    }
}

/// `#[cfg(feature = "fuzzing")] pub mod fuzz;`: the only module `lib.rs` makes public, and only
/// with the feature that the fuzz targets in `fuzz/` are built with. They reach `package` and `xml`
/// through it.
fn is_fuzz_entry(m: &syn::ItemMod) -> bool {
    m.ident == "fuzz"
        && matches!(m.vis, syn::Visibility::Public(_))
        && m.attrs.iter().any(|a| {
            a.path().is_ident("cfg")
                && matches!(&a.meta, syn::Meta::List(l)
                    if l.tokens.to_string().replace(' ', "") == "feature=\"fuzzing\"")
        })
}

/// `lib.rs` is the crate root, so an inline module there would escape the table. It may
/// hold only crate docs and attributes, `mod x;`, `pub use` and the `fuzz` module behind the
/// `fuzzing` feature.
fn check_lib_rs(path: &str, text: &str) -> Vec<Violation> {
    let file = match syn::parse_file(text) {
        Ok(file) => file,
        Err(err) => {
            return vec![Violation::new(
                path,
                err.span().start().line,
                format!("cannot parse this file: {err}"),
                "fix the syntax error so the dependency check can read the file",
            )];
        }
    };
    let mut problems = Vec::new();
    for item in &file.items {
        let allowed = match item {
            syn::Item::Mod(m) => {
                m.content.is_none()
                    && (matches!(m.vis, syn::Visibility::Inherited) || is_fuzz_entry(m))
            }
            syn::Item::Use(u) => matches!(u.vis, syn::Visibility::Public(_)),
            _ => false,
        };
        if !allowed {
            let line = syn::spanned::Spanned::span(item).start().line;
            problems.push(Violation::new(
                path,
                line,
                "lib.rs holds only crate docs, attributes, `mod x;`, `pub use` and `#[cfg(feature = \"fuzzing\")] pub mod fuzz;`",
                "move this code into a module (with a row in the Allowed dependencies table) and re-export what is public; a module is not `pub` unless it is the fuzz entry",
            ));
        }
    }
    problems
}

/// The module path of a source file relative to `src/`: `ops/mod.rs` is `[ops]`,
/// `ops/diff.rs` is `[ops, diff]`. `lib.rs` is the crate root and has none.
fn module_path(rel: &str) -> Option<Vec<String>> {
    let mut parts: Vec<&str> = rel.split('/').collect();
    let last = parts.pop()?;
    if parts.is_empty() && last == "lib.rs" {
        return None;
    }
    let mut path: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
    match last.strip_suffix(".rs") {
        Some("mod") => {}
        Some(stem) => path.push(stem.to_string()),
        None => return None,
    }
    if path.is_empty() { None } else { Some(path) }
}
