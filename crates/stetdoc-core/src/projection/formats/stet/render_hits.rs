//! The lines of a compact find: one for each paragraph that matched, under a line for each outermost clause.
//!
//! ```text
//! {in p98 "Clause 5 — PRICING"}
//! p102 {§5.2} …this Agreement are denominated in United States Dollars (USD). All invoices shall be issued in USD…
//! p144 CLAUSE 11 — PAYMENT
//! p150 11.6 All payments due under this Agreement shall be made in United States Dollars (USD) by wire… {~ 4 matches}
//! {tip: p144 is the heading of clause 11: view clause 11 to read it whole, or look up clause 11 for what cites it}
//! ```
//!
//! The text of a line is the paragraph's own, so a piece of it can be given as an edit's `find`. An ellipsis outside it
//! says the paragraph goes on.

use super::layout::LineBuf;
use super::refs_syntax::write_ref;
use super::render::{build, emit};
use super::render_other::terms_text;
use crate::projection::model::{FindHit, HitList};

/// The lines of a `HitList`.
pub(super) fn hit_lines(out: &mut String, list: &HitList) {
    let mut under = None;
    for hit in &list.hits {
        if let Some(target) = &hit.under
            && under != Some(target.r#ref)
        {
            under = Some(target.r#ref);
            if target.r#ref != hit.r#ref {
                emit(
                    out,
                    &build(|b| {
                        b.atomic(&format!("{{in {} ", write_ref(target.r#ref)));
                        b.quoted(&target.text);
                        b.atomic("}");
                    }),
                );
            }
        }
        emit(out, &build(|b| hit_line(b, hit, list.needles)));
    }
    for tip in &list.tips {
        emit(
            out,
            &build(|b| {
                b.atomic(&format!(
                    "{{tip: {} is the heading of {}: view {} to read it whole, or look up {} for what cites it}}",
                    write_ref(tip.r#ref),
                    tip.clause,
                    tip.clause,
                    tip.clause
                ));
            }),
        );
    }
}

/// One hit: its ref, its clause unless the text starts with its number, the text around the match, and the needles it
/// holds.
fn hit_line(b: &mut LineBuf, hit: &FindHit, needles: u32) {
    b.atomic(&write_ref(hit.r#ref));
    if let Some(clause) = &hit.clause
        && (hit.cut_before || !hit.text.starts_with(&**clause))
    {
        b.atomic(" {§");
        b.content(clause, true, None);
        b.atomic("}");
    }
    b.atomic(" ");
    if hit.cut_before {
        b.atomic("…");
    }
    b.content(&hit.text, false, None);
    if hit.cut_after {
        b.atomic("…");
    }
    if hit.deleted_only {
        b.atomic(" {~ only in deleted text}");
    }
    let total: u32 = hit.terms.iter().fold(0, |a, t| a.saturating_add(t.count));
    if needles > 1 {
        b.atomic(&format!(" {{~ {}}}", terms_text(&hit.terms)));
    } else if total > 1 {
        b.atomic(&format!(" {{~ {total} matches}}"));
    }
}
