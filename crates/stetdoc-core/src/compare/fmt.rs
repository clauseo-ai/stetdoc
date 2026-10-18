//! Comparing formatting: paragraph properties, the properties of a paragraph mark, and run properties character by
//! character along the text that two paragraphs have in common (design `compare.md`, 5.4).
//!
//! Properties are read through `wordml` and compared after the **ignore list**: properties that no reader sees and that
//! Word rewrites on save are not differences. They are `w:rsid*` attributes, `w14:paraId` and `w14:textId`, `w:cnfStyle` (a
//! cache of conditional formatting), `w:lang`, `w:noProof`, `w:proofErr`, and the `w:hint` attribute of `w:rFonts`. The list is
//! the one `const` below, and `FIDELITY.md` repeats it. Style ids are compared as they are written: a style's definition is not
//! compared (8.3 of the design), and an original list counts as the revised one when the caller says so.

use super::edits::Replace;
use crate::api::view::Mode;
use crate::wordml::{Attr, Ns, Paragraph, Prop, Props, TextMap};
use std::collections::HashMap;
use std::ops::Range;

/// The properties that are not differences: elements in the Word namespace, by local name.
pub(super) const IGNORED_ELEMENTS: &[&str] = &["cnfStyle", "lang", "noProof", "proofErr"];

/// The attributes that are not differences: by local name, whatever the namespace, for the ones Word keeps as a cache.
pub(super) const IGNORED_ATTRIBUTES: &[&str] = &["paraId", "textId"];

/// One range of the revised paragraph whose run properties differ from the original's over the same text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FmtChange {
    /// The characters of the revised paragraph.
    pub b: Range<usize>,
    /// The same characters in the original paragraph: its run properties are the snapshot.
    pub a: Range<usize>,
}

/// Property signatures, interned so that a whole paragraph compares as small numbers.
#[derive(Default)]
pub(super) struct Sigs {
    ids: HashMap<String, u32>,
}

impl Sigs {
    fn id(&mut self, canon: String) -> u32 {
        let next = u32::try_from(self.ids.len()).unwrap_or(u32::MAX);
        *self.ids.entry(canon).or_insert(next)
    }
}

fn ignored_attr(a: &Attr, element: &Prop) -> bool {
    a.name.starts_with("rsid")
        || IGNORED_ATTRIBUTES.contains(&&*a.name)
        || (element.ns == Ns::Word && &*element.name == "rFonts" && &*a.name == "hint")
}

fn write_prop(p: &Prop, out: &mut String) {
    out.push('<');
    match &p.ns {
        Ns::None => {}
        Ns::Word => out.push_str("w:"),
        Ns::Ext(uri) => {
            out.push('{');
            out.push_str(uri);
            out.push('}');
        }
    }
    out.push_str(&p.name);
    let mut attrs: Vec<String> = p
        .attrs
        .iter()
        .filter(|a| !ignored_attr(a, p))
        .map(|a| {
            let ns = match &a.ns {
                Ns::None => String::new(),
                Ns::Word => "w:".to_owned(),
                Ns::Ext(u) => format!("{{{u}}}"),
            };
            format!(" {ns}{}={:?}", a.name, &*a.value)
        })
        .collect();
    attrs.sort();
    for a in attrs {
        out.push_str(&a);
    }
    out.push('>');
    write_children(&p.children, out);
    out.push_str("</>");
}

fn write_children(children: &[Prop], out: &mut String) {
    let mut parts: Vec<String> = children
        .iter()
        .filter(|c| !(c.ns == Ns::Word && IGNORED_ELEMENTS.contains(&&*c.name)))
        .map(|c| {
            let mut s = String::new();
            write_prop(c, &mut s);
            s
        })
        .collect();
    parts.sort();
    for s in parts {
        out.push_str(&s);
    }
}

/// The canonical form of a property container, after the ignore list.
pub(super) fn canon(props: &Props) -> String {
    let all: Vec<Prop> = props.iter().cloned().collect();
    let mut out = String::new();
    write_children(&all, &mut out);
    out
}

/// What differs between the properties of two paired paragraphs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ParaDiff {
    /// The paragraph properties differ (style, list, indent and the rest): a `pPrChange`.
    pub ppr: bool,
    /// The properties of the paragraph mark differ: an `rPrChange` in the mark.
    pub mark: bool,
}

/// Compares the paragraph properties and the mark properties of two paragraphs. `same_num` says whether the original's list
/// is the revised document's: it is called for the two `numId`s, and a missing `numPr` is the same as a missing one only.
pub(super) fn para_diff(
    a: &Paragraph<'_>,
    b: &Paragraph<'_>,
    same_num: &dyn Fn(i64, i64) -> bool,
) -> ParaDiff {
    let rest = |p: &Paragraph<'_>| canon(&p.base_props().without(&["numPr"]));
    let mut ppr = rest(a) != rest(b);
    if !ppr {
        ppr = match (a.num_pr(), b.num_pr()) {
            (None, None) => false,
            (Some(x), Some(y)) => {
                x.ilvl != y.ilvl
                    || match (x.num_id, y.num_id) {
                        (Some(m), Some(n)) => !same_num(m, n),
                        (None, None) => false,
                        _ => true,
                    }
            }
            _ => true,
        };
    }
    let mark = |p: &Paragraph<'_>| {
        p.mark_props()
            .map(|r| r.to_owned())
            .map(|x| canon(&x))
            .unwrap_or_default()
    };
    ParaDiff {
        ppr,
        mark: mark(a) != mark(b),
    }
}

/// The run properties of every character of the paragraph, as signatures: one number for each character of the `Final`
/// text. A character with no run has the signature of no properties.
pub(super) fn char_sigs(p: &Paragraph<'_>, map: &TextMap, sigs: &mut Sigs) -> Vec<u32> {
    let mut out = Vec::new();
    for item in map.items() {
        if item.len == 0 || !item.vis.in_mode(Mode::Final) {
            continue;
        }
        let canon = p
            .run_props(item)
            .map(|r| canon(&r.to_owned()))
            .unwrap_or_default();
        let id = sigs.id(canon);
        out.extend(std::iter::repeat_n(id, item.len as usize));
    }
    out
}

/// The ranges of the revised paragraph where the run properties differ from the original's over the same text.
///
/// Only the text that the replacements leave alone is compared, character for character: the revised character at `i` is the
/// original's at `i` less the distance of the stretch it is in. A range ends where either side's properties change, so each
/// range has one snapshot.
pub(super) fn run_changes(a_sigs: &[u32], b_sigs: &[u32], reps: &[Replace]) -> Vec<FmtChange> {
    let mut out: Vec<FmtChange> = Vec::new();
    let mut a_at = 0usize;
    let mut b_at = 0usize;
    let mut stretches: Vec<(Range<usize>, Range<usize>)> = Vec::new();
    for r in reps {
        stretches.push((a_at..r.a.start, b_at..r.b.start));
        a_at = r.a.end;
        b_at = r.b.end;
    }
    stretches.push((a_at..a_sigs.len(), b_at..b_sigs.len()));
    for (ra, rb) in stretches {
        let mut current: Option<FmtChange> = None;
        let mut last: Option<(u32, u32)> = None;
        for (x, y) in ra.clone().zip(rb.clone()) {
            let (Some(&sa), Some(&sb)) = (a_sigs.get(x), b_sigs.get(y)) else {
                break;
            };
            if sa == sb {
                out.extend(current.take());
                last = None;
                continue;
            }
            match (&mut current, last) {
                (Some(c), Some(l)) if l == (sa, sb) => {
                    c.a.end = x + 1;
                    c.b.end = y + 1;
                }
                _ => {
                    out.extend(current.take());
                    current = Some(FmtChange {
                        a: x..x + 1,
                        b: y..y + 1,
                    });
                }
            }
            last = Some((sa, sb));
        }
        out.extend(current);
    }
    out
}
