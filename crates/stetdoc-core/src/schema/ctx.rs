//! A position in the schema: the declaration of a parent element, reached from a part root.

use super::PartKind;
use super::effective::{Mc, disposition_in, is_mc};
use super::generated::parts;
use super::scope::{MAX_ANCESTORS, is_plain, scope_at};
use super::tables::{self, Model, NONE};
use super::tree::{Nodes, QName};

/// A position in the schema: "the children of an element declared like this". The same
/// element name can have different declarations in different parents (`w:rPr` in `w:r` and
/// in `w:pPr`), so a context is reached by walking down from a part root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx(u16);

impl Ctx {
    /// The root element of a part.
    pub fn root(part: PartKind) -> Option<Ctx> {
        parts::PART_ROOTS
            .get(part as usize)
            .copied()
            .filter(|d| *d != NONE)
            .map(Ctx)
    }

    /// The context of a child element, if the content model names it.
    pub fn child(self, name: QName<'_>) -> Option<Ctx> {
        let model = Model::of_decl(self.0)?;
        let col = col_of(&model, name)?;
        model.alpha_decl.get(col).copied().map(Ctx)
    }

    /// Walks down from a part root through element names (the ancestors, outermost first).
    pub fn from_path(part: PartKind, path: &[QName<'_>]) -> Option<Ctx> {
        path.iter().try_fold(Ctx::root(part)?, |c, n| c.child(*n))
    }

    /// Whether the content model names `name` as a child at all.
    pub fn allows_child(self, name: QName<'_>) -> bool {
        Model::of_decl(self.0).is_some_and(|m| col_of(&m, name).is_some() || m.any)
    }

    /// The canonical order of a child: lower comes first. Children with equal order may
    /// interleave in any way, or never occur together (the alternatives of a choice).
    pub fn order_of(self, name: QName<'_>) -> Option<u16> {
        let model = Model::of_decl(self.0)?;
        let col = col_of(&model, name)?;
        model.alpha_rank.get(col).copied()
    }

    /// Whether the parent is a property container: its children have a fixed order that can be
    /// rewritten. The generator marks a declaration this way when its content model has no
    /// wildcard, at least two symbols, at most three symbols that share a rank, and symbols that
    /// never occur together sharing a rank (98 of the 832 declarations, among them `w:pPr`,
    /// `w:rPr`, `w:tblPr`, `w:sectPr`, `w:settings`, `w:styles`, `w:numbering` and
    /// `w:document`).
    pub fn is_reorderable(self) -> bool {
        tables::decl_row(self.0).is_some_and(|r| r.2 & tables::FLAG_REORDER != 0)
    }

    /// `w:CT_PPr/w:pPr`, for messages and tests.
    pub fn label(self) -> String {
        tables::decl_display(self.0)
    }

    #[cfg(test)]
    pub(crate) fn from_decl(decl: u16) -> Ctx {
        Ctx(decl)
    }

    /// The names the content model allows as children, for tests.
    #[cfg(test)]
    pub(crate) fn child_names(self) -> Vec<(String, String)> {
        let Some(model) = Model::of_decl(self.0) else {
            return Vec::new();
        };
        model
            .alpha_name
            .iter()
            .filter_map(|n| tables::name_row(*n))
            .filter_map(|(ns, local)| {
                tables::namespace_row(*ns).map(|r| (r.0.to_string(), (*local).to_string()))
            })
            .collect()
    }

    /// The declaration index, for the validator.
    pub(crate) fn decl(self) -> u16 {
        self.0
    }

    /// The content model, unless the declaration is opaque.
    pub(super) fn model(self) -> Option<Model> {
        Model::of_decl(self.0)
    }
}

/// The alphabet column of a name, if the model names it.
pub(super) fn col_of(model: &Model, (ns, local): QName<'_>) -> Option<usize> {
    let name = tables::ns_by_uri(ns).and_then(|n| tables::name_id(n, local))?;
    model.col_of(name)
}

/// The column of a sibling or a new element: its alphabet column, else the wildcard.
pub(super) fn column(model: &Model, name: QName<'_>) -> Option<usize> {
    col_of(model, name).or_else(|| model.wildcard())
}

/// The context of `node`: the declaration of its children, found by walking down from the root
/// of the part through its ancestors with [`Ctx::child`]. The walk passes through what insertion
/// and validation read through: an `mc:AlternateContent` and its branches, and an element that
/// `mc:ProcessContent` keeps the children of, are not a level of their own, so a `w:r` in the
/// `mc:Choice` of an `mc:AlternateContent` in a `w:p` has the context of a run in a paragraph.
/// There is no context (`None`) inside an element that is skipped, for an element the schema does
/// not place where it stands, and for a part whose root is not what `part` names, except that a
/// paragraph, a run, a table, a row or a cell has its own context wherever it stands (`known`), and
/// so has what is below it.
///
/// A node in a branch of an `mc:AlternateContent` that Word does not take (the `mc:Fallback` of
/// one whose `mc:Choice` Word reads) has the context it would have if Word read it, because a
/// context is a fact about structure and not about what Word reads. Code that must not edit what
/// Word does not read, such as the copy of a text box that is only there for Word 2007, asks
/// [`in_taken_branch`](super::in_taken_branch) as well.
pub fn ctx_at<T: Nodes>(part: PartKind, tree: &T, node: T::Node) -> Option<Ctx> {
    let mut path = vec![node];
    let mut cur = node;
    while let Some(parent) = tree.parent(cur) {
        path.push(parent);
        cur = parent;
        if path.len() > MAX_ANCESTORS {
            return None;
        }
    }
    path.reverse();
    let mut ctx = Some(Ctx::root(part)?);
    for ancestor in path.into_iter().skip(1) {
        let name = tree.name(ancestor);
        if ["AlternateContent", "Choice", "Fallback"]
            .iter()
            .any(|local| is_mc(name, local))
        {
            continue;
        }
        if is_plain(name.0, name.1) {
            ctx = step(ctx, name);
            continue;
        }
        let scope = scope_at(tree, ancestor, false);
        match disposition_in(tree, ancestor, &scope.chain, scope.ign) {
            Mc::Normal => ctx = step(ctx, name),
            Mc::ProcessContent => {}
            Mc::Skip | Mc::Branch(_) | Mc::NoBranch => return None,
        }
    }
    ctx
}

/// The context of a child of `ctx` named `name`. A paragraph, a run, a table, a row or a cell of WordprocessingML that the
/// schema does not place where it stands (inside a body-level `w:moveFrom`, under an element of an unknown namespace) still
/// has its own content model, so it gets the context it has in the body, and its children are checked by it: a misplaced
/// `w:pPr` cannot pass unseen there. Any other element that the parent does not name has no context.
fn step(ctx: Option<Ctx>, name: QName<'_>) -> Option<Ctx> {
    ctx.and_then(|c| c.child(name)).or_else(|| known(name))
}

/// The context of a known block-level or run-level element in the body, whatever its parent.
pub(super) fn known((ns, local): QName<'_>) -> Option<Ctx> {
    if !super::walk::is_word_namespace(ns) {
        return None;
    }
    let path: &[&str] = match local {
        "p" => &["body", "p"],
        "r" => &["body", "p", "r"],
        "tbl" => &["body", "tbl"],
        "tr" => &["body", "tbl", "tr"],
        "tc" => &["body", "tbl", "tr", "tc"],
        _ => return None,
    };
    let path: Vec<QName<'_>> = path.iter().map(|l| (ns, *l)).collect();
    Ctx::from_path(PartKind::Document, &path)
}
