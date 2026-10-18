//! W5 and W10: a revision inside a revision of its kind, and what a revision or a property
//! container holds.

use super::{Scan, Tag};
use crate::api::check::FindingCode;
use crate::check::found::Found;
use crate::xml::NodeRef;

/// Whether the element has no element child.
fn is_empty(element: &NodeRef<'_>) -> bool {
    element.child_elements().next().is_none()
}

pub(super) fn check(scan: &mut Scan<'_>, element: &NodeRef<'_>, local: &str) {
    match local {
        "ins" | "del" | "moveFrom" | "moveTo" => {
            // In a paragraph mark's properties, a row's or a numbering's, the element names the
            // revision and holds nothing: only a revision around runs is read here.
            if scan.up.parent() == Some(Tag::Properties) {
                return;
            }
            let nested = match local {
                "ins" => scan.up.within_paragraph(Tag::Ins),
                "del" => scan.up.within_paragraph(Tag::Del),
                _ => false,
            };
            if nested {
                scan.found.push(Found::rule(
                    FindingCode::RevisionNestedInItself,
                    local,
                    Some(element.id()),
                ));
            }
            if is_empty(element) {
                scan.found.push(Found::rule(
                    FindingCode::EmptyRevision,
                    local,
                    Some(element.id()),
                ));
            }
        }
        "rPr" | "pPr" => {
            if scan.up.parent() != Some(Tag::Snapshot) && is_empty(element) {
                scan.found.push(Found::rule(
                    FindingCode::EmptyProperties,
                    local,
                    Some(element.id()),
                ));
            }
        }
        _ => {}
    }
}
