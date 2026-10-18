//! Reads the error vocabulary out of `api/error.rs` without compiling it.
//!
//! xtask must not depend on `stetdoc-core`: a syntax error in the core would stop
//! `cargo xtask check` before any check ran, and the code generation could
//! not repair a core that fails to build. So the entries of the `error_codes!` list and
//! the `ExitClass::{number, label}` matches are parsed with `syn` instead. The core's own
//! tests keep checking the enum itself.

use crate::violation::Violation;
use std::collections::BTreeMap;
use syn::parse::{ParseStream, Parser};
use syn::{Expr, ImplItem, Item, Lit, Pat, Token};

/// The file the vocabulary lives in.
pub const ENUM_FILE: &str = "crates/stetdoc-core/src/api/error.rs";

/// An exit class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassSpec {
    /// The `ExitClass` variant name.
    pub variant: String,
    /// The exit status.
    pub number: u8,
    /// The short name used in `ARCHITECTURE.md`.
    pub label: String,
}

/// An error code, as written in the `error_codes!` list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeSpec {
    /// The `ErrorCode` variant name.
    pub variant: String,
    /// The snake_case wire name.
    pub wire: String,
    /// The exit status of its class.
    pub exit: u8,
    /// The label of its class.
    pub label: String,
    /// The documentation sentence.
    pub doc: String,
    /// The default `next` hint.
    pub next: String,
    /// One-based line of the wire name in `error.rs`.
    pub line: usize,
}

/// Everything read from `error.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorSpecs {
    /// The exit classes, in the order of the `ExitClass` enum.
    pub classes: Vec<ClassSpec>,
    /// The codes, in the order of the `error_codes!` list.
    pub codes: Vec<CodeSpec>,
}

struct RawEntry {
    variant: String,
    fields: BTreeMap<String, (String, usize)>,
}

fn fail(line: usize, message: impl Into<String>) -> Violation {
    Violation::new(
        ENUM_FILE,
        line,
        message,
        "keep the shape the checks read: an `error_codes!` list of `Variant { wire, exit, doc, next }` entries and `ExitClass::{number, label}` matches of plain literals",
    )
}

/// Parses `error.rs` text.
pub fn parse(source: &str) -> Result<ErrorSpecs, Violation> {
    let file = syn::parse_file(source).map_err(|e| {
        fail(
            e.span().start().line,
            format!("cannot parse this file: {e}"),
        )
    })?;
    let mut raw: Option<Vec<RawEntry>> = None;
    let mut numbers: BTreeMap<String, u8> = BTreeMap::new();
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    let mut class_order: Vec<String> = Vec::new();
    for item in &file.items {
        match item {
            Item::Macro(m) if m.mac.path.is_ident("error_codes") => {
                let entries = entries.parse2(m.mac.tokens.clone()).map_err(|e| {
                    fail(
                        e.span().start().line,
                        format!("cannot read the error_codes! list: {e}"),
                    )
                })?;
                raw = Some(entries);
            }
            Item::Enum(e) if e.ident == "ExitClass" => {
                class_order = e.variants.iter().map(|v| v.ident.to_string()).collect();
            }
            Item::Impl(i) if i.trait_.is_none() && is_type(&i.self_ty, "ExitClass") => {
                for member in &i.items {
                    if let ImplItem::Fn(f) = member {
                        let name = f.sig.ident.to_string();
                        if name == "number" {
                            numbers = match_arms(&f.block, |lit| match lit {
                                Lit::Int(n) => n.base10_parse::<u8>().ok(),
                                _ => None,
                            })?;
                        } else if name == "label" {
                            labels = match_arms(&f.block, |lit| match lit {
                                Lit::Str(s) => Some(s.value()),
                                _ => None,
                            })?;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let raw = raw.ok_or_else(|| fail(0, "there is no `error_codes!` list"))?;
    let mut classes = Vec::new();
    for variant in class_order {
        let number = numbers.get(&variant).copied().ok_or_else(|| {
            fail(
                0,
                format!("ExitClass::{variant} has no literal in `ExitClass::number`"),
            )
        })?;
        let label = labels.get(&variant).cloned().ok_or_else(|| {
            fail(
                0,
                format!("ExitClass::{variant} has no string literal in `ExitClass::label`"),
            )
        })?;
        classes.push(ClassSpec {
            variant,
            number,
            label,
        });
    }
    if classes.is_empty() {
        return Err(fail(0, "there is no `ExitClass` enum"));
    }
    let mut codes = Vec::new();
    for entry in raw {
        let get = |key: &str| {
            entry.fields.get(key).cloned().ok_or_else(|| {
                fail(
                    0,
                    format!("the entry `{}` has no `{key}` field", entry.variant),
                )
            })
        };
        let (wire, line) = get("wire")?;
        let (exit, _) = get("exit")?;
        let (doc, _) = get("doc")?;
        let (next, _) = get("next")?;
        let class = classes.iter().find(|c| c.variant == exit).ok_or_else(|| {
            fail(
                line,
                format!(
                    "the entry `{}` names an unknown exit class `{exit}`",
                    entry.variant
                ),
            )
        })?;
        codes.push(CodeSpec {
            variant: entry.variant.clone(),
            wire,
            exit: class.number,
            label: class.label.clone(),
            doc,
            next,
            line,
        });
    }
    Ok(ErrorSpecs { classes, codes })
}

fn is_type(ty: &syn::Type, name: &str) -> bool {
    matches!(ty, syn::Type::Path(p) if p.path.is_ident(name))
}

/// Reads the arms of the single `match` that is a function's tail expression.
fn match_arms<T>(
    block: &syn::Block,
    literal: impl Fn(&Lit) -> Option<T>,
) -> Result<BTreeMap<String, T>, Violation> {
    let tail = match block.stmts.last() {
        Some(syn::Stmt::Expr(Expr::Match(m), _)) => m,
        _ => {
            return Err(fail(
                0,
                "ExitClass::number and ::label must each be one `match`",
            ));
        }
    };
    let mut map = BTreeMap::new();
    for arm in &tail.arms {
        let variant = match &arm.pat {
            Pat::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
            _ => None,
        };
        let value = match &*arm.body {
            Expr::Lit(l) => literal(&l.lit),
            _ => None,
        };
        match (variant, value) {
            (Some(v), Some(value)) => {
                map.insert(v, value);
            }
            _ => {
                return Err(fail(
                    arm.pat_line(),
                    "each ExitClass match arm must be `ExitClass::Variant => <literal>`",
                ));
            }
        }
    }
    Ok(map)
}

trait PatLine {
    fn pat_line(&self) -> usize;
}

impl PatLine for syn::Arm {
    fn pat_line(&self) -> usize {
        syn::spanned::Spanned::span(&self.pat).start().line
    }
}

fn entries(input: ParseStream) -> syn::Result<Vec<RawEntry>> {
    let mut out = Vec::new();
    while !input.is_empty() {
        let variant: syn::Ident = input.parse()?;
        let body;
        syn::braced!(body in input);
        let mut fields = BTreeMap::new();
        while !body.is_empty() {
            let key: syn::Ident = body.parse()?;
            body.parse::<Token![:]>()?;
            let (value, line) = if body.peek(syn::LitStr) {
                let lit: syn::LitStr = body.parse()?;
                (lit.value(), lit.span().start().line)
            } else {
                let id: syn::Ident = body.parse()?;
                (id.to_string(), id.span().start().line)
            };
            fields.insert(key.to_string(), (value, line));
            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            }
        }
        out.push(RawEntry {
            variant: variant.to_string(),
            fields,
        });
    }
    Ok(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::repo::Repo;

    /// The specs of the real `error.rs`.
    pub fn real() -> ErrorSpecs {
        let source = Repo::discover()
            .and_then(|r| r.read(ENUM_FILE))
            .expect("read error.rs");
        parse(&source).expect("error.rs parses")
    }

    const SAMPLE: &str = r#"
        pub enum ExitClass { Usage, Target }
        impl ExitClass {
            pub const fn number(self) -> u8 { match self { ExitClass::Usage => 2, ExitClass::Target => 3 } }
            pub const fn label(self) -> &'static str {
                match self { ExitClass::Usage => "usage", ExitClass::Target => "target" }
            }
        }
        error_codes! {
            InvalidPayload {
                wire: "invalid_payload",
                exit: Usage,
                doc: "A payload \
                      is bad.",
                next: "Fix it.",
            }
            NotFound { wire: "not_found", exit: Target, doc: "Missing.", next: "Look." }
        }
    "#;

    #[test]
    fn parses_entries_classes_and_joins_string_continuations() {
        let specs = parse(SAMPLE).expect("parses");
        assert_eq!(specs.classes.len(), 2);
        assert_eq!(specs.codes.len(), 2);
        let first = &specs.codes[0];
        assert_eq!(first.variant, "InvalidPayload");
        assert_eq!(first.wire, "invalid_payload");
        assert_eq!((first.exit, first.label.as_str()), (2, "usage"));
        assert_eq!(first.doc, "A payload is bad.");
        assert_eq!(first.next, "Fix it.");
        assert_eq!(first.line, 11);
        assert_eq!(specs.codes[1].exit, 3);
    }

    #[test]
    fn the_real_error_rs_parses_with_every_code_in_a_known_class() {
        let specs = real();
        assert_eq!(
            specs.classes.iter().map(|c| c.number).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7]
        );
        let stale = specs
            .codes
            .iter()
            .find(|c| c.wire == "stale_ref")
            .expect("stale_ref");
        assert_eq!((stale.exit, stale.label.as_str()), (4, "stale"));
        assert!(
            specs
                .codes
                .iter()
                .all(|c| c.doc.ends_with('.') && !c.next.is_empty())
        );
    }

    #[test]
    fn a_missing_list_or_field_or_class_is_a_violation_at_error_rs() {
        let no_list = SAMPLE.replace("error_codes!", "other!");
        assert!(
            parse(&no_list)
                .expect_err("no list")
                .message
                .contains("error_codes!")
        );
        let no_next = SAMPLE.replace("next: \"Look.\"", "");
        assert!(
            parse(&no_next)
                .expect_err("no field")
                .message
                .contains("`next`")
        );
        let bad_class = SAMPLE.replace("exit: Target", "exit: Nowhere");
        let err = parse(&bad_class).expect_err("bad class");
        assert!(err.message.contains("unknown exit class"));
        assert_eq!(err.file, ENUM_FILE);
        assert!(
            parse("fn broken( {")
                .expect_err("syntax")
                .message
                .contains("cannot parse")
        );
    }

    #[test]
    fn a_match_arm_that_is_not_a_literal_is_a_violation() {
        let computed = SAMPLE.replace("ExitClass::Usage => 2", "ExitClass::Usage => 1 + 1");
        assert!(parse(&computed).is_err());
    }
}
