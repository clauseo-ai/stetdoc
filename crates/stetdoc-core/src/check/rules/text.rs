//! W1 and W2: where text may stand, and when it needs `xml:space`.

use super::{Scan, Tag};
use crate::api::check::FindingCode;
use crate::check::found::Found;
use crate::schema::XML_NS;
use crate::xml::{NodeKind, NodeRef};

/// Whether XML would treat the first or the last character of the text as white space.
fn edge_space(element: &NodeRef<'_>) -> bool {
    let space = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    let (mut first, mut last) = (None, None);
    for child in element.children() {
        if !matches!(child.kind(), NodeKind::Text | NodeKind::CData) {
            continue;
        }
        let text = child.text();
        if first.is_none() {
            first = text.chars().next();
        }
        if let Some(c) = text.chars().next_back() {
            last = Some(c);
        }
    }
    first.is_some_and(space) || last.is_some_and(space)
}

/// Whether the element keeps its white space: what the nearest element that says `xml:space` says, the element itself or an
/// ancestor, as XML inherits the attribute (`wordml` reads it the same way).
fn preserves(element: &NodeRef<'_>) -> bool {
    let mut cur = Some(*element);
    while let Some(e) = cur {
        if let Some(value) = e.attr(XML_NS, "space") {
            return value == "preserve";
        }
        cur = e.parent();
    }
    false
}

pub(super) fn check(scan: &mut Scan<'_>, element: &NodeRef<'_>, local: &str) {
    match local {
        "t" | "instrText" => {
            if scan.up.within_paragraph(Tag::Del) {
                scan.found.push(Found::rule(
                    FindingCode::TextInDeletion,
                    local,
                    Some(element.id()),
                ));
            }
        }
        "delText" | "delInstrText" => {
            if !scan.up.within_paragraph(Tag::Del) {
                scan.found.push(Found::rule(
                    FindingCode::DeltextOutsideDeletion,
                    local,
                    Some(element.id()),
                ));
            }
        }
        _ => return,
    }
    if !preserves(element) && edge_space(element) {
        scan.found.push(Found::element(
            FindingCode::EdgeSpaceWithoutPreserve,
            local,
            Some(element.id()),
        ));
    }
}
