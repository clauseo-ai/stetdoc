//! The two styles a comment uses, found by name, and added the way Word adds them when a document has
//! neither. Style ids are localised (`Testocommento` in Italian), so the id is never assumed.

use super::body::Styles;
use super::xmlw::{self, el};
use crate::api::error::StetError;
use crate::revisions::handle::PartWriter;
use crate::schema::PartKind;
use crate::state::PartsView;
use crate::threads::is_kind;
use crate::wordml::{RelTarget, StyleDefs, StyleKind};
use std::sync::Arc;

/// The names of the two styles, in the language of the style sheet's own default.
const REFERENCE_NAME: &str = "annotation reference";
const TEXT_NAME: &str = "annotation text";

/// What the style sheet has and what is to be added to it.
#[derive(Clone, Debug, Default)]
pub(super) struct Plan {
    /// The styles part, if the document has one. With none, comments carry no style.
    pub(super) part: Option<Arc<str>>,
    /// The ids to use.
    pub(super) styles: Styles,
    add_reference: bool,
    add_text: bool,
    /// The ids that a new style is based on: the default character style and the default paragraph style.
    based_char: Option<String>,
    based_para: Option<String>,
}

impl Plan {
    /// True if the plan adds a style.
    pub(super) fn adds(&self) -> bool {
        self.add_reference || self.add_text
    }
}

/// An id for a new style that no style has: the Word id, or the Word id with a number.
fn free_id(defs: &StyleDefs, wanted: &str) -> String {
    if defs.get(wanted).is_none() {
        return wanted.to_owned();
    }
    (2..)
        .map(|n| format!("{wanted}{n}"))
        .find(|id| defs.get(id).is_none())
        .unwrap_or_else(|| wanted.to_owned())
}

/// Reads the style sheet and decides.
pub(super) fn plan(view: &PartsView<'_>, main: &str) -> Result<Plan, StetError> {
    let Some(main_id) = view.id(main) else {
        return Ok(Plan::default());
    };
    let rels = view.rels(main_id)?;
    let part = rels
        .iter()
        .filter(|r| is_kind(PartKind::Styles, &r.rel_type))
        .find_map(|r| match &r.target {
            RelTarget::Part(name) => view.id(name),
            _ => None,
        });
    let Some(id) = part else {
        return Ok(Plan::default());
    };
    let defs = StyleDefs::read(view.tree(id)?);
    let find = |name: &str, kind: StyleKind| {
        defs.by_name(name)
            .filter(|d| d.kind == kind)
            .map(|d| d.id.to_string())
    };
    let reference = find(REFERENCE_NAME, StyleKind::Character);
    let text = find(TEXT_NAME, StyleKind::Paragraph);
    Ok(Plan {
        part: view.name(id).cloned(),
        add_reference: reference.is_none(),
        add_text: text.is_none(),
        styles: Styles {
            reference: Some(reference.unwrap_or_else(|| free_id(&defs, "CommentReference"))),
            text: Some(text.unwrap_or_else(|| free_id(&defs, "CommentText"))),
        },
        based_char: defs
            .default_for(StyleKind::Character)
            .map(|d| d.id.to_string()),
        based_para: defs
            .default_for(StyleKind::Paragraph)
            .map(|d| d.id.to_string()),
    })
}

/// Appends the styles the plan adds, as Word writes them: `annotation reference` (a small character style)
/// and `annotation text` (a paragraph style). There is no `w:link` and no `Comment Text Char`: nothing
/// uses a linked character style, and a style no run uses is not added. Returns true if the part changed.
///
/// A `w:style` goes where the schema puts it (`schema::insert_in_order`), after the other styles.
pub(super) fn apply(tx: &mut PartWriter<'_, '_>, plan: &Plan) -> Result<bool, StetError> {
    let (Some(part), true) = (&plan.part, plan.adds()) else {
        return Ok(false);
    };
    let tree = tx.part_mut(part)?;
    let w = tree.root().namespace().to_owned();
    let root = tree.root().id();
    let val = |tree: &mut crate::xml::SpanTree, local: &str, v: &str| {
        el(tree, &w, local, &[(&w, "val", v)], &[])
    };
    if plan.add_reference
        && let Some(id) = &plan.styles.reference
    {
        let mut kids = vec![val(tree, "name", REFERENCE_NAME)?];
        if let Some(b) = &plan.based_char {
            kids.push(val(tree, "basedOn", b)?);
        }
        kids.push(val(tree, "uiPriority", "99")?);
        kids.push(el(tree, &w, "semiHidden", &[], &[])?);
        kids.push(el(tree, &w, "unhideWhenUsed", &[], &[])?);
        let sz = val(tree, "sz", "16")?;
        let szcs = val(tree, "szCs", "16")?;
        kids.push(el(tree, &w, "rPr", &[], &[sz, szcs])?);
        let style = el(
            tree,
            &w,
            "style",
            &[(&w, "type", "character"), (&w, "styleId", id)],
            &kids,
        )?;
        xmlw::place(tree, PartKind::Styles, root, style)?;
    }
    if plan.add_text
        && let Some(id) = &plan.styles.text
    {
        let mut kids = vec![val(tree, "name", TEXT_NAME)?];
        if let Some(b) = &plan.based_para {
            kids.push(val(tree, "basedOn", b)?);
        }
        kids.push(val(tree, "uiPriority", "99")?);
        kids.push(el(tree, &w, "semiHidden", &[], &[])?);
        kids.push(el(tree, &w, "unhideWhenUsed", &[], &[])?);
        let spacing = el(
            tree,
            &w,
            "spacing",
            &[(&w, "line", "240"), (&w, "lineRule", "auto")],
            &[],
        )?;
        kids.push(el(tree, &w, "pPr", &[], &[spacing])?);
        let sz = val(tree, "sz", "20")?;
        let szcs = val(tree, "szCs", "20")?;
        kids.push(el(tree, &w, "rPr", &[], &[sz, szcs])?);
        let style = el(
            tree,
            &w,
            "style",
            &[(&w, "type", "paragraph"), (&w, "styleId", id)],
            &kids,
        )?;
        xmlw::place(tree, PartKind::Styles, root, style)?;
    }
    Ok(true)
}
