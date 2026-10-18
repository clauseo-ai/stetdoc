//! The names a property container allows, for `set` and the typed operations over it: every property to set, and every child
//! of one, must be an element that the schema's declaration of the container (`w:pPr`, `w:rPr`, `w:tblPr`, `w:trPr`, `w:tcPr`,
//! `w:sectPr`) names, whatever the paragraph's place in the document (design 2.5 and 8.1).

use super::props::invalid;
use crate::api::error::StetError;
use crate::revisions::types::{PropEdit, PropElement, PropTarget};
use crate::schema::{Ctx, PartKind, QName};

/// The main namespace of the schema's names, which a Strict document's properties are looked up in too.
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The property container of a target, as the path from the root element of a document part.
fn container(target: &PropTarget) -> &'static [&'static str] {
    match target {
        PropTarget::Run { .. } => &["body", "p", "r", "rPr"],
        PropTarget::ParagraphMark { .. } => &["body", "p", "pPr", "rPr"],
        PropTarget::Paragraph { .. } => &["body", "p", "pPr"],
        PropTarget::Section { .. } | PropTarget::BodySection { .. } => &["body", "sectPr"],
        PropTarget::Table { .. } => &["body", "tbl", "tblPr"],
        PropTarget::Row { .. } => &["body", "tbl", "tr", "trPr"],
        PropTarget::Cell { .. } => &["body", "tbl", "tr", "tc", "tcPr"],
    }
}

/// Every property to set, and every child of one, is an element its container allows. The check is made on the container's
/// own declaration (`w:pPr` of a paragraph, `w:tblPr` of a table), not on where the target sits, so a paragraph whose place
/// the schema does not describe (inside a body-level `w:moveFrom`) is held to the same names as any other.
pub(super) fn allowed(target: &PropTarget, edit: &PropEdit) -> Result<(), StetError> {
    let path: Vec<QName<'_>> = container(target).iter().map(|l| (W, *l)).collect();
    let Some(ctx) = Ctx::from_path(PartKind::Document, &path) else {
        return Ok(());
    };
    fn check(ctx: Ctx, parent: &str, e: &PropElement) -> Result<(), StetError> {
        let name = (W, e.local.as_str());
        if !ctx.allows_child(name) {
            return Err(invalid(format!(
                "the property {} is not allowed in {parent}",
                e.local
            ))
            .with_next("Name an element that the container allows, as the schema of WordprocessingML lists it."));
        }
        match ctx.child(name) {
            Some(inner) => e
                .children
                .iter()
                .try_for_each(|c| check(inner, &e.local, c)),
            None => Ok(()),
        }
    }
    let parent = container(target).last().copied().unwrap_or_default();
    edit.set.iter().try_for_each(|e| check(ctx, parent, e))
}
