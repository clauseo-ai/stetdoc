//! Reads the tables in `ARCHITECTURE.md` that checks enforce.
//!
//! The document is the source of truth for the dependency layering and for the error
//! table. These functions parse it, and report a problem in the document itself as a
//! violation.

use crate::violation::Violation;
use std::collections::{BTreeMap, BTreeSet};

/// The file the tables live in.
pub const FILE: &str = "ARCHITECTURE.md";

/// One data row of a Markdown table.
#[derive(Clone, Debug)]
pub struct TableRow {
    /// One-based line in the document.
    pub line: usize,
    /// Trimmed cell text.
    pub cells: Vec<String>,
}

/// The data rows of the first table after the heading line `heading` (for example
/// `"### Allowed dependencies"`). `None` when the heading or its table is missing.
pub fn table_under(md: &str, heading: &str) -> Option<Vec<TableRow>> {
    let lines: Vec<&str> = md.lines().collect();
    let start = lines.iter().position(|l| l.trim_end() == heading)?;
    let mut i = start + 1;
    while i < lines.len() && !lines[i].starts_with('|') {
        if lines[i].starts_with('#') {
            return None;
        }
        i += 1;
    }
    if i >= lines.len() {
        return None;
    }
    let mut rows = Vec::new();
    let mut index = 0;
    while i < lines.len() && lines[i].starts_with('|') {
        // Row 0 is the header and row 1 the separator.
        if index >= 2 {
            rows.push(TableRow {
                line: i + 1,
                cells: split_cells(lines[i]),
            });
        }
        index += 1;
        i += 1;
    }
    Some(rows)
}

fn split_cells(line: &str) -> Vec<String> {
    let escaped = line.replace("\\|", "\u{1}");
    let trimmed = escaped.trim().trim_start_matches('|').trim_end_matches('|');
    trimmed
        .split('|')
        .map(|c| c.replace('\u{1}', "|").trim().to_string())
        .collect()
}

/// The text between backticks in `cell`, in order.
pub fn backticked(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

/// What a module may depend on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Deps {
    /// The row says "nothing".
    Nothing,
    /// The row says "any of the above".
    Any,
    /// An explicit list.
    Only(BTreeSet<String>),
}

/// A row of the Allowed dependencies table.
#[derive(Clone, Debug)]
pub struct DepRow {
    /// The module.
    pub module: String,
    /// What it may depend on.
    pub deps: Deps,
    /// One-based line in `ARCHITECTURE.md`.
    pub line: usize,
}

/// The Allowed dependencies table.
#[derive(Clone, Debug)]
pub struct AllowedDeps {
    /// Rows in document order.
    pub rows: Vec<DepRow>,
}

impl AllowedDeps {
    /// The row for `module`.
    pub fn row(&self, module: &str) -> Option<&DepRow> {
        self.rows.iter().find(|r| r.module == module)
    }

    /// Whether `from` may depend on `to`. A module may always use itself. A row that says "any of
    /// the above" allows the modules in the rows above it, and none below.
    pub fn allows(&self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }
        let at = |m: &str| self.rows.iter().position(|r| r.module == m);
        match self.row(from).map(|r| &r.deps) {
            Some(Deps::Any) => at(to).zip(at(from)).is_some_and(|(to, from)| to < from),
            Some(Deps::Only(set)) => set.contains(to),
            _ => false,
        }
    }

    /// The module names, in document order.
    pub fn modules(&self) -> Vec<&str> {
        self.rows.iter().map(|r| r.module.as_str()).collect()
    }
}

const DEPS_HEADING: &str = "### Allowed dependencies";

/// The modules that exist only for something other than the engine: `fuzz`, behind the `fuzzing`
/// feature, for the fuzz targets, and `bench`, in test builds, for the benchmarks. Nothing may
/// depend on them, so no row lists them and no "any of the above" row has one above.
const LEAF_MODULES: [&str; 2] = ["fuzz", "bench"];

/// Parses the Allowed dependencies table. A malformed table (unknown module, a
/// module listed twice, a cycle) is returned as violations against `ARCHITECTURE.md`.
pub fn parse_allowed_deps(md: &str) -> Result<AllowedDeps, Vec<Violation>> {
    let rows = table_under(md, DEPS_HEADING).ok_or_else(|| {
        vec![Violation::new(
            FILE,
            0,
            format!("there is no table under the \"{DEPS_HEADING}\" heading"),
            "restore the heading and its table (columns: Module, May depend on)",
        )]
    })?;
    let mut problems = Vec::new();
    let mut parsed: Vec<DepRow> = Vec::new();
    for row in &rows {
        let Some(module) = row
            .cells
            .first()
            .and_then(|c| backticked(c).into_iter().next())
        else {
            problems.push(Violation::new(
                FILE,
                row.line,
                "the first cell of this row names no module in backticks",
                "write the module name in backticks, for example `wordml`",
            ));
            continue;
        };
        let cell = row.cells.get(1).map(String::as_str).unwrap_or("");
        let deps = if cell.contains("any of the above") {
            Deps::Any
        } else if cell.trim() == "nothing" {
            Deps::Nothing
        } else {
            Deps::Only(backticked(cell).into_iter().collect())
        };
        if parsed.iter().any(|r| r.module == module) {
            problems.push(Violation::new(
                FILE,
                row.line,
                format!("module `{module}` has two rows"),
                "keep one row per module",
            ));
            continue;
        }
        parsed.push(DepRow {
            module,
            deps,
            line: row.line,
        });
    }
    let known: BTreeSet<&str> = parsed.iter().map(|r| r.module.as_str()).collect();
    for row in &parsed {
        if let Deps::Only(set) = &row.deps {
            for dep in set {
                if !known.contains(dep.as_str()) {
                    problems.push(Violation::new(
                        FILE,
                        row.line,
                        format!(
                            "`{}` may depend on `{dep}`, which has no row in the table",
                            row.module
                        ),
                        "list only modules that have their own row",
                    ));
                }
                if dep == &row.module {
                    problems.push(Violation::new(
                        FILE,
                        row.line,
                        format!("`{}` lists itself", row.module),
                        "remove the self-reference",
                    ));
                }
            }
        }
    }
    for leaf in LEAF_MODULES {
        let leaf_at = parsed.iter().position(|r| r.module == leaf);
        for (at, row) in parsed.iter().enumerate() {
            match &row.deps {
                Deps::Only(set) if row.module != leaf && set.contains(leaf) => {
                    problems.push(Violation::new(
                        FILE,
                        row.line,
                        format!(
                            "`{}` may depend on `{leaf}`, which exists only for the fuzz targets or the benchmarks",
                            row.module
                        ),
                        format!("remove `{leaf}` from this row: nothing that ships may use it"),
                    ));
                }
                Deps::Any if leaf_at.is_some_and(|f| f < at) => {
                    problems.push(Violation::new(
                        FILE,
                        row.line,
                        format!(
                            "`{}` says \"any of the above\" with the `{leaf}` row above it, so it could depend on `{leaf}`",
                            row.module
                        ),
                        format!("move the `{leaf}` row below this one"),
                    ));
                }
                _ => {}
            }
        }
    }
    problems.extend(find_cycles(&parsed));
    if problems.is_empty() {
        Ok(AllowedDeps { rows: parsed })
    } else {
        Err(problems)
    }
}

fn find_cycles(rows: &[DepRow]) -> Vec<Violation> {
    // "Any of the above" is an edge to every module in a row above it. A row below it that lists
    // it is no cycle, because the module above does not list that row back.
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let to: Vec<&str> = match &row.deps {
            Deps::Only(set) => set.iter().map(String::as_str).collect(),
            Deps::Any => rows.iter().take(i).map(|r| r.module.as_str()).collect(),
            Deps::Nothing => Vec::new(),
        };
        edges.insert(row.module.as_str(), to);
    }
    let mut problems = Vec::new();
    let mut done: BTreeSet<&str> = BTreeSet::new();
    for row in rows {
        let mut stack = Vec::new();
        if let Some(cycle) = visit(row.module.as_str(), &edges, &mut stack, &mut done) {
            problems.push(Violation::new(
                FILE,
                row.line,
                format!("the table has a dependency cycle: {}", cycle.join(" -> ")),
                "break the cycle: move the shared code into a lower module",
            ));
            break;
        }
    }
    problems
}

fn visit<'a>(
    node: &'a str,
    edges: &BTreeMap<&'a str, Vec<&'a str>>,
    stack: &mut Vec<&'a str>,
    done: &mut BTreeSet<&'a str>,
) -> Option<Vec<String>> {
    if let Some(at) = stack.iter().position(|n| *n == node) {
        let mut cycle: Vec<String> = stack[at..].iter().map(|s| s.to_string()).collect();
        cycle.push(node.to_string());
        return Some(cycle);
    }
    if done.contains(node) {
        return None;
    }
    stack.push(node);
    if let Some(next) = edges.get(node) {
        for dep in next {
            if let Some(cycle) = visit(dep, edges, stack, done) {
                return Some(cycle);
            }
        }
    }
    stack.pop();
    done.insert(node);
    None
}

/// A row of the Errors table: one exit class and its codes.
#[derive(Clone, Debug)]
pub struct ErrorRow {
    /// One-based line in `ARCHITECTURE.md`.
    pub line: usize,
    /// The exit class number.
    pub number: u8,
    /// The exit class label, for example "operation failed".
    pub label: String,
    /// The backticked codes in the row. Other text, such as "CLI argument errors", is
    /// not a code and is ignored.
    pub codes: Vec<String>,
}

const ERRORS_HEADING: &str = "## Errors";

/// Parses the Errors table.
pub fn parse_error_table(md: &str) -> Result<Vec<ErrorRow>, Vec<Violation>> {
    let rows = table_under(md, ERRORS_HEADING).ok_or_else(|| {
        vec![Violation::new(
            FILE,
            0,
            format!("there is no table under the \"{ERRORS_HEADING}\" heading"),
            "restore the heading and its table (columns: Exit class, Codes)",
        )]
    })?;
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for row in rows {
        let first = row.cells.first().map(String::as_str).unwrap_or("");
        let (digits, label) = first.split_once(' ').unwrap_or((first, ""));
        match digits.parse::<u8>() {
            Ok(number) => out.push(ErrorRow {
                line: row.line,
                number,
                label: label.trim().to_string(),
                codes: backticked(row.cells.get(1).map(String::as_str).unwrap_or("")),
            }),
            Err(_) => problems.push(Violation::new(
                FILE,
                row.line,
                format!("the exit class cell \"{first}\" does not start with a number"),
                "write it as, for example, \"1 operation failed\"",
            )),
        }
    }
    if problems.is_empty() {
        Ok(out)
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEPS: &str = "# T\n\n### Allowed dependencies\n\nprose\n\n| Module | May depend on |\n|---|---|\n| `api` | nothing |\n| `xml` | `api` |\n| `ops` | `api`, `xml` |\n| `document` | any of the above |\n\nafter\n";

    #[test]
    fn parses_the_dependency_table() {
        let table = parse_allowed_deps(DEPS).expect("parses");
        assert_eq!(table.modules(), ["api", "xml", "ops", "document"]);
        assert!(table.allows("ops", "xml"));
        assert!(!table.allows("xml", "ops"));
        assert!(!table.allows("api", "xml"));
        assert!(table.allows("document", "ops"));
        assert!(table.allows("xml", "xml"));
        assert_eq!(table.row("ops").expect("row").line, 11);
    }

    #[test]
    fn a_missing_table_is_a_violation() {
        let err = parse_allowed_deps("# nothing here\n").expect_err("missing");
        assert!(err[0].message.contains("Allowed dependencies"));
    }

    #[test]
    fn an_unknown_dependency_is_a_violation() {
        let md = DEPS.replace("`api`, `xml` |", "`api`, `ghost` |");
        let err = parse_allowed_deps(&md).expect_err("unknown");
        assert!(err.iter().any(|v| v.message.contains("`ghost`")), "{err:?}");
    }

    #[test]
    fn a_cycle_is_a_violation() {
        let md = DEPS.replace("| `api` | nothing |", "| `api` | `ops` |");
        let err = parse_allowed_deps(&md).expect_err("cycle");
        assert!(err.iter().any(|v| v.message.contains("cycle")), "{err:?}");
    }

    #[test]
    fn any_of_the_above_allows_the_rows_above_and_no_row_below() {
        let md = DEPS.replace(
            "| `document` | any of the above |",
            "| `document` | any of the above |\n| `fuzz` | `api`, `xml`, `document` |",
        );
        let table = parse_allowed_deps(&md).expect("a row below the any row may depend on it");
        assert!(table.allows("fuzz", "document"));
        assert!(table.allows("document", "ops"));
        assert!(
            !table.allows("document", "fuzz"),
            "fuzz is below document, not above"
        );
        assert!(!table.allows("document", "nothing-like-this"));
        let loops = md.replace("| `api` | nothing |", "| `api` | `fuzz` |");
        let err = parse_allowed_deps(&loops).expect_err("api -> fuzz -> api");
        assert!(err.iter().any(|v| v.message.contains("cycle")), "{err:?}");
    }

    #[test]
    fn nothing_may_depend_on_fuzz_or_bench_and_their_rows_sit_below_every_any_of_the_above_row() {
        for leaf in ["fuzz", "bench"] {
            let below = DEPS.replace(
                "| `document` | any of the above |",
                &format!("| `document` | any of the above |\n| `{leaf}` | `api`, `xml` |"),
            );
            assert!(
                parse_allowed_deps(&below).is_ok(),
                "the {leaf} row at the end is fine"
            );
            let listed = below.replace(
                "| `ops` | `api`, `xml` |",
                &format!("| `ops` | `api`, `xml`, `{leaf}` |"),
            );
            let err = parse_allowed_deps(&listed).expect_err("ops lists it");
            assert!(
                err.iter()
                    .any(|v| v.message.contains(&format!("`ops` may depend on `{leaf}`"))),
                "{err:?}"
            );
            let above = DEPS.replace(
                "| `document` | any of the above |",
                &format!("| `{leaf}` | `api`, `xml` |\n| `document` | any of the above |"),
            );
            let err = parse_allowed_deps(&above).expect_err("it is above document");
            assert!(
                err.iter()
                    .any(|v| v.message.contains("`document` says \"any of the above\"")),
                "{err:?}"
            );
        }
    }

    #[test]
    fn depending_on_the_any_module_is_a_cycle() {
        let md = DEPS.replace("| `xml` | `api` |", "| `xml` | `api`, `document` |");
        let err = parse_allowed_deps(&md).expect_err("cycle");
        assert!(err.iter().any(|v| v.message.contains("cycle")), "{err:?}");
    }

    #[test]
    fn a_duplicate_row_is_a_violation() {
        let md = DEPS.replace("| `xml` | `api` |", "| `xml` | `api` |\n| `xml` | `api` |");
        assert!(parse_allowed_deps(&md).is_err());
    }

    #[test]
    fn parses_the_error_table_ignoring_text_that_is_not_a_code() {
        let md = "## Errors\n\n| Exit class | Codes |\n|---|---|\n| 1 operation failed | `unsupported`, `cross_block` |\n| 2 usage | `invalid_payload`, CLI argument errors |\n| 5 validation | `validation_failed` (the edit would add violations) |\n";
        let rows = parse_error_table(md).expect("parses");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].number, 1);
        assert_eq!(rows[0].label, "operation failed");
        assert_eq!(rows[1].codes, ["invalid_payload"]);
        assert_eq!(rows[2].codes, ["validation_failed"]);
        assert_eq!(rows[2].line, 7);
    }

    #[test]
    fn an_exit_class_without_a_number_is_a_violation() {
        let md = "## Errors\n\n| Exit class | Codes |\n|---|---|\n| operation failed | `a` |\n";
        assert!(parse_error_table(md).is_err());
    }

    #[test]
    fn backticked_extracts_code_spans() {
        assert_eq!(backticked("`a`, x, `b`"), ["a", "b"]);
        assert!(backticked("none").is_empty());
    }
}
