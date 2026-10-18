//! Sections: one `Section` per `w:sectPr` of the body, with the headers and footers resolved.
//!
//! The `w:sectPr` of a section is in the `w:pPr` of the paragraph that ends it, except the last one,
//! which is the final child of `w:body`. `Story::sections` lists them in document order and applies
//! the inheritance rule: a section that names no header or footer of a kind takes the one of the
//! section before it. References are resolved through [`Rels`], so nobody downstream sees a
//! relationship id (`docs/design/wordml.md` 3.7).

use super::names::{is_w, r_attr, w_attr, w_child};
use super::paragraph::Paragraph;
use super::props::PropsRef;
use super::rels::{RelKind, RelTarget, Rels};
use super::story::{BlockKind, MarkRevs, SECTION_BREAK, Story, StoryKind};
use crate::api::view::Mode;
use crate::xml::{NodeId, SpanTree};
use std::sync::Arc;

/// How a section starts, `w:type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SectionBreak {
    /// On the next page. The default, and the reading of a missing or unknown value.
    NextPage,
    /// On the same page, `continuous`.
    Continuous,
    /// On the next even page, `evenPage`.
    EvenPage,
    /// On the next odd page, `oddPage`.
    OddPage,
    /// In the next column, `nextColumn`.
    NextColumn,
}

/// The part names of the default, first-page and even-page header or footer of a section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartSlots {
    /// The part for `w:type="default"`.
    pub default: Option<Arc<str>>,
    /// The part for `w:type="first"`, shown when the section has `w:titlePg`.
    pub first: Option<Arc<str>>,
    /// The part for `w:type="even"`, shown on even pages when the document has `w:evenAndOddHeaders`.
    pub even: Option<Arc<str>>,
}

impl PartSlots {
    fn slot(&mut self, kind: &str) -> Option<&mut Option<Arc<str>>> {
        match kind {
            "default" => Some(&mut self.default),
            "first" => Some(&mut self.first),
            "even" => Some(&mut self.even),
            _ => None,
        }
    }
}

/// One `w:sectPr` of the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// The `w:sectPr` element.
    pub node: NodeId,
    /// The entry index of the paragraph whose `w:pPr` holds it, or `None` for the final section,
    /// which is the last child of `w:body`.
    pub ends_at: Option<usize>,
    /// How the section starts.
    pub break_type: SectionBreak,
    /// Whether the first page has its own header and footer, `w:titlePg`.
    pub title_pg: bool,
    /// The revisions on the mark of the paragraph that holds it, all of them. None for the final
    /// section.
    pub mark_revs: MarkRevs,
    /// The headers, with inheritance applied.
    pub headers: PartSlots,
    /// The footers, with inheritance applied.
    pub footers: PartSlots,
    /// The headers this `w:sectPr` names itself, before inheritance. A kind it names that does not
    /// resolve is `None` here and in `headers`.
    pub own_headers: PartSlots,
    /// The footers this `w:sectPr` names itself, before inheritance.
    pub own_footers: PartSlots,
}

impl Section {
    /// Whether the section exists in `mode`. A section whose paragraph mark is deleted or moved
    /// away is gone in `Final`, and one whose mark is inserted or moved in is gone in `Original`,
    /// with its page setup. A mark that is both is gone in both. The final section always exists.
    pub fn exists_in(&self, mode: Mode) -> bool {
        !self.mark_revs.gone_in(mode)
    }

    /// The section properties, `w:sectPr`, read in place: page size, margins, columns and so on.
    /// `tree` is the tree the story was scanned from.
    pub fn props<'a>(&self, tree: &'a SpanTree) -> PropsRef<'a> {
        PropsRef::new(tree.node_ref(self.node))
    }
}

/// The references of one kind (`w:headerReference` or `w:footerReference`) of a `w:sectPr`, resolved.
/// The first reference of each `w:type` counts. A reference that names no relationship, a
/// relationship of another type, or a target outside the package gives `None` for its slot.
fn resolve(
    sect: &crate::xml::NodeRef<'_>,
    reference: &str,
    kind: RelKind,
    rels: &Rels,
) -> (PartSlots, [bool; 3]) {
    let mut slots = PartSlots::default();
    let mut named = [false; 3];
    for r in sect.child_elements().filter(|c| is_w(c, reference)) {
        let Some(which) = w_attr(&r, "type") else {
            continue;
        };
        let at = match which.as_ref() {
            "default" => 0,
            "first" => 1,
            "even" => 2,
            _ => continue,
        };
        if named.get(at).copied().unwrap_or(true) {
            continue;
        }
        if let Some(flag) = named.get_mut(at) {
            *flag = true;
        }
        let part = r_attr(&r, "id")
            .and_then(|id| rels.get(&id))
            .filter(|rel| kind.matches(&rel.rel_type))
            .and_then(|rel| match &rel.target {
                RelTarget::Part(name) => Some(Arc::from(name.as_str())),
                _ => None,
            });
        if let Some(slot) = slots.slot(which.as_ref()) {
            *slot = part;
        }
    }
    (slots, named)
}

/// Applies the inheritance rule: a kind the section does not name takes the previous section's.
fn inherit(own: &PartSlots, named: [bool; 3], previous: &PartSlots) -> PartSlots {
    let pick = |at: usize, mine: &Option<Arc<str>>, before: &Option<Arc<str>>| {
        if named.get(at).copied().unwrap_or(false) {
            mine.clone()
        } else {
            before.clone()
        }
    };
    PartSlots {
        default: pick(0, &own.default, &previous.default),
        first: pick(1, &own.first, &previous.first),
        even: pick(2, &own.even, &previous.even),
    }
}

fn break_type(sect: &PropsRef<'_>) -> SectionBreak {
    match sect.val("type").as_deref() {
        Some("continuous") => SectionBreak::Continuous,
        Some("evenPage") => SectionBreak::EvenPage,
        Some("oddPage") => SectionBreak::OddPage,
        Some("nextColumn") => SectionBreak::NextColumn,
        _ => SectionBreak::NextPage,
    }
}

impl Story {
    /// The sections of the body, in document order: one for each paragraph that holds a
    /// `w:sectPr`, then the final one in `w:body`. Any other story gives an empty list.
    ///
    /// Inheritance is applied over the sections as written, whatever the mode: a section that names
    /// no reference of a kind takes the previous section's, and the first has none. A caller that
    /// shows a mode where a section is gone (`Section::exists_in`) can rebuild the chain from
    /// `own_headers` and `own_footers` of the sections that remain.
    pub fn sections(&self, tree: &SpanTree, rels: &Rels) -> Vec<Section> {
        if self.kind != StoryKind::Body {
            return Vec::new();
        }
        let mut found = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if e.kind == BlockKind::Paragraph
                && e.has(SECTION_BREAK)
                && let Some(p) = Paragraph::new(tree, e.node)
                && let Some(sect) = p.section_break()
            {
                found.push((sect, Some(i), e.mark_revs()));
            }
        }
        let last = w_child(&tree.root(), "body")
            .and_then(|b| b.child_elements().filter(|c| is_w(c, "sectPr")).last());
        if let Some(sect) = last {
            found.push((sect, None, MarkRevs::default()));
        }
        let mut out: Vec<Section> = Vec::with_capacity(found.len());
        for (sect, ends_at, mark_revs) in found {
            let props = PropsRef::new(sect);
            let (own_headers, named_h) = resolve(&sect, "headerReference", RelKind::Header, rels);
            let (own_footers, named_f) = resolve(&sect, "footerReference", RelKind::Footer, rels);
            let (headers, footers) = match out.last() {
                Some(prev) => (
                    inherit(&own_headers, named_h, &prev.headers),
                    inherit(&own_footers, named_f, &prev.footers),
                ),
                None => (own_headers.clone(), own_footers.clone()),
            };
            out.push(Section {
                node: sect.id(),
                ends_at,
                break_type: break_type(&props),
                title_pg: props.on_off("titlePg").unwrap_or(false),
                mark_revs,
                headers,
                footers,
                own_headers,
                own_footers,
            });
        }
        out
    }
}
