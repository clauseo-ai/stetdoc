//! Finds the paths the dependency check cares about in one source file.
//!
//! It walks the syntax tree with `syn` and also scans the tokens of macros, because a
//! path in a macro body is still a dependency. It reports four kinds of finding:
//! - paths into other modules of the core;
//! - aliases and globs of the crate root, which would hide those paths from the check;
//! - use of `std` file, network, process, environment, standard stream and clock APIs,
//!   which the core must never touch;
//! - aliases and globs of `std`, `std::time` and `std::io`, which would hide the previous
//!   kind (`use std as s; s::fs::read(..)`).
//!
//! The `std` rule works on paths, so it has limits. Methods that touch the file system
//! without naming `std::fs`, such as `Path::exists`, `read_dir` and `metadata`, and calls
//! through a plain `use std::io;` import such as `io::stdin()`, cannot be caught here. A
//! reviewer checks for them.

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use syn::visit::Visit;

/// What was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A path into a module of the core. `target` is the top-level module it names.
    Path,
    /// An alias or glob of the crate root, such as `use crate as c;` or `use crate::*;`.
    RootAlias,
    /// A forbidden `std` path, such as `std::fs`.
    Io,
    /// An alias or glob of `std`, `std::time` or `std::io`, which hides `Kind::Io` paths.
    StdAlias,
}

/// One finding.
#[derive(Clone, Debug)]
pub struct Found {
    /// One-based line.
    pub line: usize,
    /// The top-level module named, for `Kind::Path`.
    pub target: String,
    /// The path as written.
    pub shown: String,
    /// What kind of finding it is.
    pub kind: Kind,
    /// Whether it sits in test-only code (`#[cfg(test)]` or `#[test]`).
    pub in_test: bool,
}

type Segments = Vec<(String, usize)>;

/// Walks one file. `mod_path` is the module path of the file, for example
/// `["ops", "diff"]` for `ops/diff.rs`.
pub struct Finder {
    mod_path: Vec<String>,
    test_depth: usize,
    /// Everything found so far.
    pub found: Vec<Found>,
}

impl Finder {
    /// A finder for a file at `mod_path`.
    pub fn new(mod_path: Vec<String>) -> Finder {
        Finder {
            mod_path,
            test_depth: 0,
            found: Vec::new(),
        }
    }

    fn push(&mut self, kind: Kind, line: usize, target: &str, shown: String) {
        self.found.push(Found {
            line,
            target: target.to_string(),
            shown,
            kind,
            in_test: self.test_depth > 0,
        });
    }

    fn line_at(segs: &Segments, at: usize) -> usize {
        segs.get(at).or(segs.first()).map_or(1, |(_, l)| *l)
    }

    fn record(&mut self, segs: &Segments) {
        let names: Vec<&str> = segs.iter().map(|(n, _)| n.as_str()).collect();
        let shown = names.join("::");
        if io_forbidden(&names) {
            self.push(Kind::Io, Self::line_at(segs, 1), "", shown.clone());
        }
        if let Some((target, at)) = resolve(&names, &self.mod_path) {
            self.push(Kind::Path, Self::line_at(segs, at), &target, shown);
        }
    }

    /// The end of a `use` path or a macro path.
    fn leaf(&mut self, segs: &Segments, mode: Mode) {
        let names: Vec<&str> = segs.iter().map(|(n, _)| n.as_str()).collect();
        if mode != Mode::Plain && is_root(&names, &self.mod_path) {
            let shown = names.join("::");
            self.push(Kind::RootAlias, Self::line_at(segs, 0), "", shown);
        } else if let Some(hub) = mode_hub(&names, mode) {
            self.push(Kind::StdAlias, Self::line_at(segs, 0), "", hub);
        } else {
            self.record(segs);
        }
    }

    fn flatten_use(&mut self, tree: &syn::UseTree, prefix: &mut Segments) {
        let leaf = |this: &mut Finder, ident: &syn::Ident, mode: Mode, prefix: &mut Segments| {
            prefix.push((ident.to_string(), ident.span().start().line));
            this.leaf(prefix, mode);
            prefix.pop();
        };
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push((p.ident.to_string(), p.ident.span().start().line));
                self.flatten_use(&p.tree, prefix);
                prefix.pop();
            }
            // A plain name can name a module itself only as `{self}`.
            syn::UseTree::Name(n) => {
                let mode = if n.ident == "self" {
                    Mode::SelfName
                } else {
                    Mode::Plain
                };
                leaf(self, &n.ident, mode, prefix);
            }
            syn::UseTree::Rename(r) => leaf(self, &r.ident, Mode::Rename, prefix),
            syn::UseTree::Glob(_) => self.leaf(prefix, Mode::Glob),
            syn::UseTree::Group(g) => {
                for item in &g.items {
                    self.flatten_use(item, prefix);
                }
            }
        }
    }

    /// Finds paths in a macro's tokens, including grouped ones such as `crate::{a, b::c}`.
    fn scan(&mut self, stream: TokenStream) {
        let tokens: Vec<TokenTree> = stream.into_iter().collect();
        let mut i = 0;
        while i < tokens.len() {
            match &tokens[i] {
                TokenTree::Group(g) => {
                    self.scan(g.stream());
                    i += 1;
                }
                TokenTree::Ident(id) if id == "extern" && is_extern_crate_self(&tokens, i) => {
                    let line = id.span().start().line;
                    self.push(Kind::RootAlias, line, "", "extern crate self".to_string());
                    i += 3;
                }
                TokenTree::Ident(id) if is_path_start(&id.to_string()) => {
                    i = self.path_from(&tokens, i, Vec::new());
                }
                _ => i += 1,
            }
        }
    }

    fn path_from(&mut self, tokens: &[TokenTree], mut i: usize, prefix: Segments) -> usize {
        let mut segs = prefix;
        loop {
            match tokens.get(i) {
                Some(TokenTree::Ident(id)) => {
                    segs.push((id.to_string(), id.span().start().line));
                    i += 1;
                }
                Some(TokenTree::Punct(p)) if p.as_char() == '*' && !segs.is_empty() => {
                    self.leaf(&segs, Mode::Glob);
                    return i + 1;
                }
                Some(TokenTree::Group(g))
                    if g.delimiter() == Delimiter::Brace && !segs.is_empty() =>
                {
                    for element in split_commas(g.stream()) {
                        self.path_from(&element, 0, segs.clone());
                        for tree in &element {
                            if let TokenTree::Group(inner) = tree {
                                self.scan(inner.stream());
                            }
                        }
                    }
                    return i + 1;
                }
                _ => break,
            }
            if is_path_sep(tokens, i) {
                i += 2;
            } else {
                break;
            }
        }
        let renamed = matches!(tokens.get(i), Some(TokenTree::Ident(a)) if a == "as");
        let mode = if renamed {
            Mode::Rename
        } else if segs.len() >= 2 && segs.last().is_some_and(|(n, _)| n == "self") {
            Mode::SelfName
        } else {
            Mode::Plain
        };
        self.leaf(&segs, mode);
        i
    }
}

/// How a path ends, which decides whether it can hide other paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// A plain path or item, such as `crate::api::E`.
    Plain,
    /// A module imported as itself with `{self}`.
    SelfName,
    /// A rename with `as`.
    Rename,
    /// A glob.
    Glob,
}

/// The `std` module an import hides other paths behind: `std` and `std::time` however
/// they are aliased or globbed, and `std::io` when renamed or globbed. A plain
/// `use std::io::{self, Read}` is ordinary and is not a hub.
fn mode_hub(names: &[&str], mode: Mode) -> Option<String> {
    if mode == Mode::Plain {
        return None;
    }
    let end = names
        .iter()
        .rposition(|n| *n != "self")
        .map_or(0, |i| i + 1);
    let base = &names[..end];
    let hub = match base {
        ["std"] | ["std", "time"] => true,
        ["std", "io"] => matches!(mode, Mode::Rename | Mode::Glob),
        _ => false,
    };
    hub.then(|| base.join("::"))
}

fn is_path_start(word: &str) -> bool {
    matches!(word, "crate" | "super" | "self" | "std")
}

fn is_extern_crate_self(tokens: &[TokenTree], i: usize) -> bool {
    let word = |at: usize, expected: &str| matches!(tokens.get(at), Some(TokenTree::Ident(id)) if id == expected);
    word(i + 1, "crate") && word(i + 2, "self")
}

fn is_path_sep(tokens: &[TokenTree], i: usize) -> bool {
    matches!(
        (tokens.get(i), tokens.get(i + 1)),
        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
            if a.as_char() == ':' && a.spacing() == Spacing::Joint && b.as_char() == ':'
    )
}

fn split_commas(stream: TokenStream) -> Vec<Vec<TokenTree>> {
    let mut out = vec![Vec::new()];
    for tree in stream {
        match &tree {
            TokenTree::Punct(p) if p.as_char() == ',' => out.push(Vec::new()),
            _ => {
                if let Some(last) = out.last_mut() {
                    last.push(tree);
                }
            }
        }
    }
    out
}

/// `std` paths the core must not use: the file system, network, environment, processes,
/// OS-specific extensions, the standard streams and the clock. Of `std::time`, only
/// `Duration` and `TryFromFloatSecsError` are allowed, so constants such as `UNIX_EPOCH`
/// are caught too.
fn io_forbidden(names: &[&str]) -> bool {
    if names.first() != Some(&"std") {
        return false;
    }
    match names.get(1) {
        Some(&("fs" | "net" | "env" | "process" | "os")) => true,
        Some(&"time") => !matches!(names.get(2), Some(&("Duration" | "TryFromFloatSecsError"))),
        Some(&"io") => matches!(
            names.get(2),
            Some(
                &("stdin"
                    | "stdout"
                    | "stderr"
                    | "Stdin"
                    | "Stdout"
                    | "Stderr"
                    | "StdinLock"
                    | "StdoutLock"
                    | "StderrLock")
            )
        ),
        _ => false,
    }
}

fn strip_self<'a, 'b>(names: &'b [&'a str]) -> (&'b [&'a str], usize) {
    let offset = names.iter().take_while(|n| **n == "self").count();
    (&names[offset..], offset)
}

/// The top-level module a path names, and the index of the segment that names it.
/// A leading `self` is skipped before `super` is counted.
fn resolve(names: &[&str], mod_path: &[String]) -> Option<(String, usize)> {
    let (rest, offset) = strip_self(names);
    match *rest.first()? {
        "crate" => {
            let name = rest.get(1).filter(|s| **s != "self")?;
            Some((name.to_string(), offset + 1))
        }
        "super" => {
            let ups = rest.iter().take_while(|s| **s == "super").count();
            let keep = mod_path.len().checked_sub(ups)?;
            if let Some(top) = mod_path[..keep].first() {
                return Some((top.clone(), offset));
            }
            let name = rest.get(ups).filter(|s| **s != "self")?;
            Some((name.to_string(), offset + ups))
        }
        _ => None,
    }
}

/// Whether the path names the crate root itself, as in `crate`, `crate::self` or
/// `super::super` from a top-level module.
fn is_root(names: &[&str], mod_path: &[String]) -> bool {
    let (rest, _) = strip_self(names);
    match rest.first() {
        Some(&"crate") => rest[1..].iter().all(|s| *s == "self"),
        Some(&"super") => {
            let ups = rest.iter().take_while(|s| **s == "super").count();
            mod_path.len().checked_sub(ups) == Some(0) && rest[ups..].iter().all(|s| *s == "self")
        }
        _ => false,
    }
}

/// Whether attributes mark the item as test-only.
fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg") && a.parse_args::<syn::Ident>().is_ok_and(|i| i == "test"))
    })
}

impl<'ast> Visit<'ast> for Finder {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let gated = test_only(&node.attrs);
        self.test_depth += usize::from(gated);
        self.flatten_use(&node.tree, &mut Vec::new());
        self.test_depth -= usize::from(gated);
    }

    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        if node.ident == "self" {
            let line = node.ident.span().start().line;
            self.push(Kind::RootAlias, line, "", "extern crate self".to_string());
        } else if node.ident == "std" && node.rename.is_some() {
            let line = node.ident.span().start().line;
            self.push(Kind::StdAlias, line, "", "std".to_string());
        }
    }

    fn visit_path(&mut self, node: &'ast syn::Path) {
        let segs: Segments = node
            .segments
            .iter()
            .map(|s| (s.ident.to_string(), s.ident.span().start().line))
            .collect();
        self.record(&segs);
        syn::visit::visit_path(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.scan(node.tokens.clone());
        syn::visit::visit_macro(self, node);
    }

    fn visit_meta_list(&mut self, node: &'ast syn::MetaList) {
        self.scan(node.tokens.clone());
        syn::visit::visit_meta_list(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        let gated = test_only(&node.attrs);
        self.test_depth += usize::from(gated);
        if node.content.is_some() {
            self.mod_path.push(node.ident.to_string());
            syn::visit::visit_item_mod(self, node);
            self.mod_path.pop();
        } else {
            syn::visit::visit_item_mod(self, node);
        }
        self.test_depth -= usize::from(gated);
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let gated = test_only(&node.attrs);
        self.test_depth += usize::from(gated);
        syn::visit::visit_item_fn(self, node);
        self.test_depth -= usize::from(gated);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let gated = test_only(&node.attrs);
        self.test_depth += usize::from(gated);
        syn::visit::visit_item_impl(self, node);
        self.test_depth -= usize::from(gated);
    }
}
