//! The operations table and the generated operations reference.
//!
//! `stetdoc_core::api::op_spec` describes every operation once, in an `op_specs!` invocation of literals. Like the error
//! codes, it is read from source with `syn`, never linked, so a core that does not build still gets every static check. The
//! reference `docs/reference/operations.md` is written from it by `cargo xtask docs`, and the generated-docs drift check
//! fails when it is stale.

use crate::violation::Violation;
use syn::parse::{ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Item, LitBool, LitStr, Token};

/// The file the table lives in.
pub const SPEC_FILE: &str = "crates/stetdoc-core/src/api/op_spec.rs";

/// The generated reference.
pub const REFERENCE: &str = "docs/reference/operations.md";

/// One field of an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// The key.
    pub name: String,
    /// The kind of value.
    pub kind: String,
    /// Whether the operation needs it.
    pub required: bool,
    /// What it means.
    pub doc: String,
    /// For a choice, the words it may be.
    pub choices: Vec<String>,
}

/// One operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Op {
    /// The `Op` variant.
    pub variant: String,
    /// The name, as the `op` field spells it.
    pub name: String,
    /// What it does.
    pub summary: String,
    /// Whether it writes revisions.
    pub trackable: bool,
    /// Whether rejecting what it wrote restores the original.
    pub reversible: bool,
    /// The group of optional fields of which exactly one must be given.
    pub one_of: Vec<String>,
    /// An example, as JSON.
    pub example: String,
    /// Its fields.
    pub fields: Vec<Field>,
}

fn fail(line: usize, message: impl Into<String>) -> Violation {
    Violation::new(
        SPEC_FILE,
        line,
        message,
        "keep the shape the docs read: one `op_specs!` invocation of `Variant { name, summary, trackable, reversible, one_of, example, fields }` entries, written with literals",
    )
}

/// Parses the text of `op_spec.rs`.
pub fn parse(source: &str) -> Result<Vec<Op>, Violation> {
    let file = syn::parse_file(source).map_err(|e| {
        fail(
            e.span().start().line,
            format!("cannot parse this file: {e}"),
        )
    })?;
    for item in &file.items {
        if let Item::Macro(m) = item
            && m.mac.path.is_ident("op_specs")
        {
            return entries.parse2(m.mac.tokens.clone()).map_err(|e| {
                fail(
                    e.span().start().line,
                    format!("cannot read the op_specs! table: {e}"),
                )
            });
        }
    }
    Err(fail(0, "there is no `op_specs!` invocation"))
}

fn strings(input: ParseStream) -> syn::Result<Vec<String>> {
    let content;
    syn::bracketed!(content in input);
    let list: Punctuated<LitStr, Token![,]> = Punctuated::parse_terminated(&content)?;
    Ok(list.iter().map(LitStr::value).collect())
}

fn field(input: ParseStream) -> syn::Result<Field> {
    let content;
    syn::parenthesized!(content in input);
    let name: LitStr = content.parse()?;
    content.parse::<Token![,]>()?;
    let kind: syn::Ident = content.parse()?;
    content.parse::<Token![,]>()?;
    let required: LitBool = content.parse()?;
    content.parse::<Token![,]>()?;
    let doc: LitStr = content.parse()?;
    content.parse::<Token![,]>()?;
    content.parse::<Token![&]>()?;
    let choices = strings(&content)?;
    Ok(Field {
        name: name.value(),
        kind: kind.to_string(),
        required: required.value,
        doc: doc.value(),
        choices,
    })
}

fn entries(input: ParseStream) -> syn::Result<Vec<Op>> {
    let mut out = Vec::new();
    while !input.is_empty() {
        let variant: syn::Ident = input.parse()?;
        let body;
        syn::braced!(body in input);
        let mut op = Op {
            variant: variant.to_string(),
            name: String::new(),
            summary: String::new(),
            trackable: false,
            reversible: false,
            one_of: Vec::new(),
            example: String::new(),
            fields: Vec::new(),
        };
        while !body.is_empty() {
            let key: syn::Ident = body.parse()?;
            body.parse::<Token![:]>()?;
            match key.to_string().as_str() {
                "name" => op.name = body.parse::<LitStr>()?.value(),
                "summary" => op.summary = body.parse::<LitStr>()?.value(),
                "example" => op.example = body.parse::<LitStr>()?.value(),
                "trackable" => op.trackable = body.parse::<LitBool>()?.value,
                "reversible" => op.reversible = body.parse::<LitBool>()?.value,
                "one_of" => op.one_of = strings(&body)?,
                "fields" => {
                    let content;
                    syn::bracketed!(content in body);
                    while !content.is_empty() {
                        op.fields.push(field(&content)?);
                        if content.peek(Token![,]) {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown key `{other}`"),
                    ));
                }
            }
            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            }
        }
        out.push(op);
    }
    Ok(out)
}

fn yes(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

/// Writes the reference.
pub fn render(ops: &[Op]) -> String {
    let mut out = String::new();
    out.push_str("<!-- Generated by `cargo xtask docs` from `crates/stetdoc-core/src/api/op_spec.rs`. Do not edit by hand. -->\n\n");
    out.push_str("# Operations reference\n\n");
    out.push_str(&format!(
        "There are {} operations. A batch names the base token it was written against and lists operations, which apply completely or not at all. Every operation is a JSON object with an `op` field, and an unknown field is refused. A range inside a paragraph is `find` (which must match once, or `occurrence` says which match) or `start` and `end` (offsets in the paragraph's Final text, in Unicode scalar values unless the edit asks for UTF-16 units), or nothing for the whole paragraph.\n\n",
        ops.len()
    ));
    out.push_str("## Block text\n\n");
    out.push_str("Block text is the same batch without JSON escaping. The grammar is provisional until the format experiments settle it.\n\n");
    out.push_str("```text\nbase: 3f9a0c11d2b4e577:7:51ac09f2e8b34d11\n---\n@replace\nref: p42\nfind: the Supplier shall\nwith: the Supplier must\n---\n@insert\nref: p42\nplace: after\nparagraphs: <<END\nA first new paragraph.\nA second one.\nEND\n```\n\n");
    out.push_str("A stanza starts with `@name`. A field is `key: value` on one line, or `key: <<TAG` and the lines up to a line that is exactly `TAG`. A key that is not a field of the operation is refused. What a newline means depends on the kind of the field:\n\n");
    out.push_str("| Kind | One line | Heredoc | A newline inside the value |\n|---|---|---|---|\n");
    out.push_str("| text | everything after the first `: ` | the lines, joined by a newline | only in a heredoc |\n");
    out.push_str("| list | one entry | one entry for each line | never inside an entry |\n");
    out.push_str("| ref, choice, number, bool | the value | not allowed | never |\n\n");
    out.push_str(
        "## Summary\n\n| Operation | What it does | Tracked | Reversible |\n|---|---|---|---|\n",
    );
    for op in ops {
        out.push_str(&format!(
            "| [`{}`](#{}) | {} | {} | {} |\n",
            op.name,
            op.name,
            op.summary,
            yes(op.trackable),
            yes(op.reversible)
        ));
    }
    for op in ops {
        out.push_str(&format!("\n## `{}`\n\n{}\n\n", op.name, op.summary));
        out.push_str(&format!(
            "Tracked as revisions: {}. Reversible by rejecting what it wrote: {}.",
            yes(op.trackable),
            yes(op.reversible)
        ));
        if !op.one_of.is_empty() {
            let names: Vec<String> = op.one_of.iter().map(|n| format!("`{n}`")).collect();
            out.push_str(&format!(" Give exactly one of {}.", names.join(", ")));
        }
        out.push_str("\n\n| Field | Kind | Required | Meaning |\n|---|---|---|---|\n");
        for f in &op.fields {
            let mut doc = f.doc.clone();
            if !f.choices.is_empty() {
                let words: Vec<String> = f.choices.iter().map(|c| format!("`{c}`")).collect();
                doc.push_str(&format!(" One of {}.", words.join(", ")));
            }
            let kind = match f.kind.as_str() {
                "NumberList" => "number list".to_string(),
                "Props" => "properties".to_string(),
                other => other.to_lowercase(),
            };
            out.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                f.name,
                kind,
                yes(f.required),
                doc
            ));
        }
        out.push_str(&format!("\n```json\n{}\n```\n", op.example));
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::repo::Repo;

    /// The real table, read from the repository.
    pub fn real() -> Vec<Op> {
        let source = Repo::discover()
            .and_then(|r| r.read(SPEC_FILE))
            .expect("read");
        parse(&source).expect("parse")
    }

    #[test]
    fn the_real_table_reads_and_is_whole() {
        let ops = real();
        assert_eq!(ops.len(), 19);
        for op in &ops {
            assert!(
                !op.name.is_empty() && !op.summary.is_empty() && !op.example.is_empty(),
                "{}",
                op.variant
            );
            assert!(!op.fields.is_empty(), "{}", op.name);
            assert!(
                op.example.contains(&format!("\"op\":\"{}\"", op.name)),
                "{}",
                op.name
            );
        }
        let replace = ops.iter().find(|o| o.name == "replace").expect("replace");
        assert!(replace.trackable && replace.reversible);
        let with = replace
            .fields
            .iter()
            .find(|f| f.name == "with")
            .expect("with");
        assert!(with.required && with.doc.contains("newline"));
    }

    #[test]
    fn the_reference_lists_every_operation_and_field() {
        let ops = real();
        let doc = render(&ops);
        for op in &ops {
            assert!(doc.contains(&format!("## `{}`", op.name)), "{}", op.name);
            for f in &op.fields {
                assert!(
                    doc.contains(&format!("| `{}` |", f.name)),
                    "{}.{}",
                    op.name,
                    f.name
                );
            }
        }
        assert!(doc.contains("exactly one of `all`, `author`, `ids`"));
    }

    #[test]
    fn a_table_that_cannot_be_read_is_reported_not_a_panic() {
        assert!(parse("fn broken( {").is_err());
        assert!(parse("fn fine() {}").is_err());
        assert!(parse("op_specs! { Replace { colour: \"red\" } }").is_err());
    }
}
