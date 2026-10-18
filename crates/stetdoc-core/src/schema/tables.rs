//! Row types of the generated tables, and the lookups over them.
//!
//! The generator (`xtask/src/codegen`) prints these tables as plain integers and strings.
//! This file is the only place that knows how to read them.
//!
//! * **Namespaces** (`NAMESPACES`): row 0 is "no namespace". The others are sorted by URI.
//!   `ext` marks Office extension namespaces (w14, w15, w16*), which a document must list in
//!   `mc:Ignorable` unless the consumer understands them.
//! * **Names** (`NAMES`): every element and attribute name, sorted by (namespace, local).
//!   The index is the name id, so two prefixes bound to one URI are one name.
//! * **Declarations** (`DECLS`): an element declaration is a (type, element) pair such as
//!   `w:CT_ParaRPr/w:rPr`; the same element name can have different declarations in
//!   different parents, which is why a [`super::Ctx`] is a declaration and not a name.
//! * **Models** (`MODELS`): one minimal DFA per distinct content model. State 0 is the
//!   start. `TRANS` is dense, `states * (alphabet + any)` entries, `u16::MAX` for "no
//!   transition". `ALPHA_*` are parallel arrays sorted by name id: the name, the declaration
//!   of the child, its canonical rank and whether it can occur only once.
//! * **Attributes** (`ATTR_SETS`): per declaration, sorted by name id. A value type is a
//!   disjunction of conjunctions of predicates (`SPECS` -> `CLAUSES` -> `PREDS`).

use super::generated::{attrs, models, names};

pub(super) const NONE: u16 = u16::MAX;
pub(super) const FLAG_TEXT: u8 = 1;
pub(super) const FLAG_OPAQUE: u8 = 2;
/// The schema allows the children in any order; the generated sequence is Word's order.
pub(super) const FLAG_CANON: u8 = 4;
/// A property bag: every child has its own place in the order, so the parent can be
/// rewritten in schema order.
pub(super) const FLAG_REORDER: u8 = 8;

/// (uri, preferred prefix, is extension namespace, has content models in the tables)
pub(super) type NsRow = (&'static str, &'static str, bool, bool);
/// (name id, type name, flags, model, attribute set)
pub(super) type DeclRow = (u16, &'static str, u8, u16, u16);
/// (alphabet offset, alphabet length, states, transition offset, accept offset, has wildcard)
pub(super) type ModelRow = (u32, u16, u16, u32, u32, bool);

pub(super) fn slice<T>(all: &'static [T], off: usize, len: usize) -> &'static [T] {
    all.get(off..off + len).unwrap_or(&[])
}

// ---- namespaces and names -------------------------------------------------------------

pub(super) fn namespace_row(ns: u8) -> Option<&'static NsRow> {
    names::NAMESPACES.get(usize::from(ns))
}

/// Namespace id of a URI. Strict (purl.oclc.org) URIs map to their Transitional twin, and the
/// namespace Word 2013's preview wrote `w15` in maps to `w15`, as Word reads it.
pub(super) fn ns_by_uri(uri: &str) -> Option<u8> {
    let rows = names::NAMESPACES.get(1..)?;
    if let Ok(i) = rows.binary_search_by(|r| r.0.cmp(uri)) {
        return u8::try_from(i + 1).ok();
    }
    names::STRICT_ALIASES
        .iter()
        .chain(names::PREVIEW_ALIASES.iter())
        .find(|a| a.0 == uri)
        .map(|a| a.1)
}

/// Whether the tables hold content models for the namespace. A namespace that is not
/// understood can be skipped when a document lists it in `mc:Ignorable`; one that is
/// understood is always validated.
pub(super) fn is_understood(uri: &str) -> bool {
    ns_by_uri(uri).and_then(namespace_row).is_some_and(|r| r.3)
}

/// Whether the tables know the name in that namespace, as an element or as an attribute.
pub(super) fn is_known_name(uri: &str, local: &str) -> bool {
    ns_by_uri(uri).and_then(|ns| name_id(ns, local)).is_some()
}

/// Whether a URI is an ISO 29500 Strict namespace (as opposed to its Transitional twin).
pub(super) fn is_strict_uri(uri: &str) -> bool {
    names::STRICT_ALIASES.iter().any(|a| a.0 == uri)
}

/// Name id for a namespace and local name.
pub(super) fn name_id(ns: u8, local: &str) -> Option<u16> {
    names::NAMES
        .binary_search_by(|(n, l)| n.cmp(&ns).then_with(|| (*l).cmp(local)))
        .ok()
        .and_then(|i| u16::try_from(i).ok())
}

pub(super) fn name_row(id: u16) -> Option<&'static (u8, &'static str)> {
    names::NAMES.get(usize::from(id))
}

/// `w:p`-style display name using the schema's preferred prefix.
pub(super) fn display_name(id: u16) -> String {
    match name_row(id) {
        Some((ns, local)) => match namespace_row(*ns) {
            Some((_, prefix, _, _)) if !prefix.is_empty() => format!("{prefix}:{local}"),
            _ => (*local).to_string(),
        },
        None => "?".to_string(),
    }
}

/// Display name for a (uri, local) pair that may not be in the tables.
pub fn display_qname(uri: &str, local: &str) -> String {
    match ns_by_uri(uri).and_then(namespace_row) {
        Some((_, prefix, _, _)) if !prefix.is_empty() => format!("{prefix}:{local}"),
        _ if uri.is_empty() => local.to_string(),
        _ => format!("{{{uri}}}{local}"),
    }
}

// ---- declarations ---------------------------------------------------------------------

pub(super) fn decl_row(id: u16) -> Option<&'static DeclRow> {
    names::DECLS.get(usize::from(id))
}

/// `w:CT_PPr/w:pPr`
pub(super) fn decl_display(id: u16) -> String {
    match decl_row(id) {
        Some((name, ty, ..)) => format!("{ty}/{}", display_name(*name)),
        None => "?".to_string(),
    }
}

// ---- models ---------------------------------------------------------------------------

/// A content model: the DFA of one declaration.
#[derive(Clone, Copy)]
pub(super) struct Model {
    pub alpha_name: &'static [u16],
    pub alpha_decl: &'static [u16],
    pub alpha_rank: &'static [u16],
    /// Whether the symbol occurs at most once in every accepted sequence.
    pub alpha_once: &'static [bool],
    pub trans: &'static [u16],
    pub accept: &'static [u8],
    pub any: bool,
}

pub(super) static EMPTY_MODEL: Model = Model {
    alpha_name: &[],
    alpha_decl: &[],
    alpha_rank: &[],
    alpha_once: &[],
    trans: &[NONE],
    accept: &[1],
    any: false,
};

impl Model {
    pub(super) fn of_decl(decl: u16) -> Option<Model> {
        let (_, _, flags, model, _) = decl_row(decl)?;
        if flags & FLAG_OPAQUE != 0 {
            return None;
        }
        if *model == NONE {
            return Some(EMPTY_MODEL);
        }
        let &(ao, an, states, to, co, any) = models::MODELS.get(usize::from(*model))?;
        let (ao, an, states, to, co) = (
            ao as usize,
            usize::from(an),
            usize::from(states),
            to as usize,
            co as usize,
        );
        let cols = an + usize::from(any);
        Some(Model {
            alpha_name: slice(&models::ALPHA_NAME, ao, an),
            alpha_decl: slice(&models::ALPHA_DECL, ao, an),
            alpha_rank: slice(&models::ALPHA_RANK, ao, an),
            alpha_once: slice(&models::ALPHA_ONCE, ao, an),
            trans: slice(&models::TRANS, to, states * cols),
            accept: slice(&models::ACCEPT, co, states),
            any,
        })
    }

    pub(super) fn states(&self) -> usize {
        self.accept.len()
    }

    pub(super) fn cols(&self) -> usize {
        self.alpha_name.len() + usize::from(self.any)
    }

    /// Alphabet column of a name, if the model names it.
    pub(super) fn col_of(&self, name: u16) -> Option<usize> {
        self.alpha_name.binary_search(&name).ok()
    }

    /// Whether the symbol in column `col` can occur only once in the parent.
    pub(super) fn is_once(&self, col: usize) -> bool {
        self.alpha_once.get(col).copied().unwrap_or(false)
    }

    /// Wildcard column (only when `any`).
    pub(super) fn wildcard(&self) -> Option<usize> {
        self.any.then_some(self.alpha_name.len())
    }

    pub(super) fn next(&self, state: usize, col: usize) -> Option<usize> {
        let t = *self.trans.get(state * self.cols() + col)?;
        (t != NONE).then_some(usize::from(t))
    }

    pub(super) fn accepts(&self, state: usize) -> bool {
        self.accept.get(state).copied().unwrap_or(0) != 0
    }
}

// ---- attributes -----------------------------------------------------------------------

/// One attribute of a declaration.
#[derive(Clone, Copy)]
pub(super) struct AttrDecl {
    pub name: u16,
    pub spec: u16,
    pub required: bool,
}

pub(super) fn attrs_of(decl: u16) -> impl Iterator<Item = AttrDecl> {
    let range = decl_row(decl)
        .filter(|r| r.4 != NONE)
        .and_then(|r| attrs::ATTR_SETS.get(usize::from(r.4)))
        .map(|&(off, len)| slice(&attrs::ATTR_ITEMS, off as usize, usize::from(len)))
        .unwrap_or(&[]);
    range.iter().map(|&(name, spec, required)| AttrDecl {
        name,
        spec,
        required,
    })
}

pub(super) fn find_attr(decl: u16, name: u16) -> Option<AttrDecl> {
    let range = decl_row(decl)
        .filter(|r| r.4 != NONE)
        .and_then(|r| attrs::ATTR_SETS.get(usize::from(r.4)))
        .map(|&(off, len)| slice(&attrs::ATTR_ITEMS, off as usize, usize::from(len)))
        .unwrap_or(&[]);
    range
        .binary_search_by(|r| r.0.cmp(&name))
        .ok()
        .and_then(|i| range.get(i))
        .map(|&(name, spec, required)| AttrDecl {
            name,
            spec,
            required,
        })
}
