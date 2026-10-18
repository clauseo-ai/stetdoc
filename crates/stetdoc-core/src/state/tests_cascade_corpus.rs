//! The cascade checked against what Word wrote (`docs/design/styles-and-numbering.md`, 3.4 and 3.5).
//!
//! 1. **`cnfStyle` as an oracle.** In a document that Word saved (the manifest's `producer` and `word_fp`
//!    say so), the conditional regions computed from `w:tblLook` and the cell's position equal the set Word
//!    recorded in the cell's `w:cnfStyle`.
//! 2. **Theme colours.** In a document that Word saved, the colour computed from `w:themeColor` equals the
//!    cached `w:val` Word writes beside it exactly. With a tint or shade the computed colour is within two in
//!    each channel (Word's integer rounding is not reproduced exactly, see `Rgb::tinted`), no colour is
//!    further off than that, and the number that are exact is pinned, so that a change to the formula shows.
//! 3. **Theme fonts.** The typeface a theme attribute resolves to equals the explicit attribute written
//!    beside it. Word's own files rarely write both, so this one reads every producer's documents.

use super::tests_corpus_docs::{each_document, main_of, saved_by_word};
use super::*;
use crate::styles::{Rgb, Theme};
use crate::wordml::{SpanTree, TableRegion};
use crate::xml::NodeRef;
use std::collections::{BTreeSet, HashSet};

/// The regions a 12-character `cnfStyle` value names. The first eight flags are in the order ISO 29500
/// lists them. In the corners Word's value has the top right before the top left: `Formatted_elements.docx`
/// has `firstRowFirstColumn="1"` with the value `001000000100`, so position 9 is the top left. Positions 10
/// and 11 follow the same pairing and have no evidence in the corpus.
fn cnf_regions(val: &str) -> Option<HashSet<TableRegion>> {
    let bits: Vec<char> = val.trim().chars().collect();
    if bits.len() != 12 || bits.iter().any(|c| *c != '0' && *c != '1') {
        return None;
    }
    use TableRegion::*;
    let order = [
        FirstRow, LastRow, FirstCol, LastCol, Band1Vert, Band2Vert, Band1Horz, Band2Horz, NeCell,
        NwCell, SeCell, SwCell,
    ];
    Some(
        order
            .iter()
            .zip(&bits)
            .filter(|(_, b)| **b == '1')
            .map(|(r, _)| *r)
            .collect(),
    )
}

#[derive(Default)]
struct Cnf {
    cells: usize,
    differences: Vec<String>,
}

fn check_cnf(path: &str, state: &State, out: &mut Cnf) {
    let view = state.parts();
    let main = main_of(state);
    let (Ok(cascade), Ok(story), Ok(tree)) = (view.cascade(), view.story(main), view.tree(main))
    else {
        out.differences.push(format!("{path}: does not read"));
        return;
    };
    let mut seen = BTreeSet::new();
    let blocks = story.blocks();
    for (i, site) in story.table_sites(tree).iter().enumerate() {
        let Some(site) = site else { continue };
        // Word records the row-level flags (first and last row, horizontal bands) on the row and the
        // column-level ones on the cell, so the set it recorded is the union of the two.
        let Some(cell) = site.cell_cnf.as_deref().and_then(cnf_regions) else {
            continue;
        };
        let mut want = cell;
        if let Some(row) = site.row_cnf.as_deref().and_then(cnf_regions) {
            want.extend(row);
        }
        // One check per cell: a cell with several paragraphs has the same site for each. The table is named
        // by its entry (the parent of the row, which is the parent of the paragraph).
        let row = blocks.get(i).and_then(|e| e.parent);
        let table = row
            .and_then(|r| blocks.get(r as usize))
            .and_then(|e| e.parent);
        if !seen.insert((table, site.row, site.column)) {
            continue;
        }
        let got: HashSet<TableRegion> = cascade.applicable_regions(site).into_iter().collect();
        out.cells += 1;
        if got != want {
            out.differences.push(format!(
                "{path}: cell ({}, {}) of {}x{}, look {:?}: computed {got:?}, Word wrote {want:?}",
                site.row, site.column, site.row_count, site.column_count, site.look
            ));
        }
    }
}

#[test]
fn computed_table_regions_equal_the_cnf_style_word_wrote() {
    let mut cnf = Cnf::default();
    let seen = each_document(|entry, state| {
        if saved_by_word(entry) {
            check_cnf(&entry.id, state, &mut cnf);
        }
    });
    if seen.is_none() {
        return;
    }
    // Documents where Word's own cnfStyle contradicts itself. In the second table of
    // `Corrupted footnotes.docx` the odd-band flag sits on the even rows and the rows between have no
    // cnfStyle, where the first table of the same document, with the same style, is consistent: Word kept a
    // stale value after an edit. The 34 cells are counted exactly, and no other document may differ.
    const STALE: &[&str] = &["Corrupted footnotes.docx"];
    let (excused, real): (Vec<&String>, Vec<&String>) = cnf
        .differences
        .iter()
        .partition(|d| STALE.iter().any(|s| d.contains(s)));
    let report = format!(
        "cells with a cnfStyle: {}, excused: {}",
        cnf.cells,
        excused.len()
    );
    assert!(real.is_empty(), "{report}\n{real:?}");
    assert_eq!(excused.len(), 34, "{report}");
    assert!(cnf.cells >= 100, "{report}");
}

/// The value of attribute `name` in the Word namespace.
fn attr(n: &NodeRef<'_>, name: &str) -> Option<String> {
    n.attrs()
        .find(|a| {
            a.local_name() == name && a.namespace().ends_with("/wordprocessingml/2006/main")
                || a.local_name() == name && a.namespace().ends_with("/ooxml/wordprocessingml/main")
        })
        .map(|a| a.value().to_string())
}

fn walk(node: NodeRef<'_>, f: &mut impl FnMut(&NodeRef<'_>)) {
    for c in node.child_elements() {
        f(&c);
        walk(c, f);
    }
}

#[derive(Default)]
struct Theming {
    plain: usize,
    plain_misses: usize,
    tinted: usize,
    tinted_exact: usize,
    tinted_over_two: usize,
    fonts: usize,
    font_misses: usize,
    examples: Vec<String>,
}

/// The tree of the part that the main part's relationship of this type (the last segment of its URI) names.
fn related_tree<'a>(view: &PartsView<'a>, main: PartId, kind: &str) -> Option<&'a SpanTree> {
    let part = view
        .related_part(main, &|t| t.rsplit('/').next() == Some(kind))
        .ok()??;
    view.tree(part).ok()
}

fn check_theme(path: &str, word: bool, state: &State, out: &mut Theming) {
    let view = state.parts();
    let main = main_of(state);
    let Some(theme_tree) = related_tree(&view, main, "theme") else {
        return;
    };
    let theme = Theme::read(Some(theme_tree), related_tree(&view, main, "settings"));
    let trees: Vec<&SpanTree> = std::iter::once(view.tree(main).ok())
        .chain(std::iter::once(related_tree(&view, main, "styles")))
        .flatten()
        .collect();
    for tree in trees {
        walk(tree.root(), &mut |n| match n.local_name() {
            "color" | "shd" if word => {
                let (val, tc) = if n.local_name() == "color" {
                    (attr(n, "val"), attr(n, "themeColor"))
                } else {
                    (attr(n, "fill"), attr(n, "themeFill"))
                };
                let (tint, shade) = if n.local_name() == "color" {
                    (attr(n, "themeTint"), attr(n, "themeShade"))
                } else {
                    (attr(n, "themeFillTint"), attr(n, "themeFillShade"))
                };
                let (Some(val), Some(tc)) = (val, tc) else {
                    return;
                };
                let tinted = tint.is_some() || shade.is_some();
                let hex = |s: Option<String>| s.and_then(|v| u8::from_str_radix(v.trim(), 16).ok());
                let (Some(cached), Some(got)) =
                    (Rgb::parse(&val), theme.color(&tc, hex(tint), hex(shade)))
                else {
                    return;
                };
                let worst = [
                    cached.r.abs_diff(got.r),
                    cached.g.abs_diff(got.g),
                    cached.b.abs_diff(got.b),
                ]
                .into_iter()
                .max()
                .unwrap_or(0);
                if tinted {
                    out.tinted += 1;
                    out.tinted_exact += usize::from(worst == 0);
                    out.tinted_over_two += usize::from(worst > 2);
                } else {
                    out.plain += 1;
                    out.plain_misses += usize::from(worst != 0);
                }
                if worst > if tinted { 2 } else { 0 } && out.examples.len() < 8 {
                    out.examples
                        .push(format!("{path}: {tc} cached {val}, computed {}", got.hex()));
                }
            }
            "rFonts" => {
                for (plain, themed) in [("ascii", "asciiTheme"), ("hAnsi", "hAnsiTheme")] {
                    let (Some(explicit), Some(t)) = (attr(n, plain), attr(n, themed)) else {
                        continue;
                    };
                    let Some(face) = theme.font(&t) else { continue };
                    out.fonts += 1;
                    if face != explicit {
                        out.font_misses += 1;
                        if out.examples.len() < 8 {
                            out.examples.push(format!(
                                "{path}: {t} cached {explicit:?}, computed {face:?}"
                            ));
                        }
                    }
                }
            }
            _ => {}
        });
    }
}

#[test]
fn theme_colours_and_fonts_equal_the_values_word_cached() {
    let mut t = Theming::default();
    let mut word_documents = 0;
    let seen = each_document(|entry, state| {
        word_documents += usize::from(saved_by_word(entry));
        check_theme(&entry.id, saved_by_word(entry), state, &mut t);
    });
    if seen.is_none() {
        return;
    }
    let report = format!(
        "Word documents {word_documents}; plain colours {} (misses {}); tinted or shaded {} (exact {}, over two {}); fonts {} (misses {}); {:?}",
        t.plain,
        t.plain_misses,
        t.tinted,
        t.tinted_exact,
        t.tinted_over_two,
        t.fonts,
        t.font_misses,
        t.examples
    );
    // A plain theme colour of a Word document is exactly the cached value, and so is every theme font of
    // every document that writes both. A tinted or shaded colour is within two, and the exact count is
    // pinned to what the formula gives (the counts below are the curated set's), so a change to the hue or
    // saturation rounding in `Rgb::tinted` fails here.
    assert_eq!(t.plain_misses, 0, "{report}");
    assert_eq!(t.font_misses, 0, "{report}");
    assert_eq!(t.tinted_over_two, 0, "{report}");
    assert!(t.plain >= 1_360, "{report}");
    assert!(t.tinted >= 1_230, "{report}");
    assert!(t.tinted_exact >= EXACT_TINTS, "{report}");
    assert!(t.fonts >= 190, "{report}");
}

/// How many tinted or shaded theme colours of the curated set equal Word's cached value exactly.
const EXACT_TINTS: usize = 1_101;
