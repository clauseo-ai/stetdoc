//! The complex form of a simple field for `normalize_unfolding` (design `revisions.md`, 3.6.3 and 7.1): the laws R1 and R2
//! read the `w:fldSimple` elements that a batch's tracked deletions wrote as complex fields in that form on the original side.

use super::laws_norm::{N, is, run_own_props};

/// The runs of a complex field for the `w:fldSimple` `f`, as a tracked deletion of a whole simple field writes them (design
/// `revisions.md`, 3.6.3): begin, code, separate, its content, end. The begin carries `w:fldLock`, `w:dirty` and the
/// `w:fldData`, and the field's runs take the properties of the content's first run, the end those of its last, with any
/// pending change of them (`normalize` compares a change up to its id). When all of the content is in insertions, the field's
/// runs are in copies of the first and the last.
pub(super) fn complex_of(f: N) -> Vec<N> {
    let N::El {
        ns, attrs, kids, ..
    } = f
    else {
        return vec![f];
    };
    let mut runs: Vec<&N> = Vec::new();
    let mut stack: Vec<&N> = kids.iter().rev().collect();
    while let Some(k) = stack.pop() {
        if is(k, "r") {
            runs.push(k);
        } else if let N::El { kids: inner, .. } = k
            && !is(k, "fldData")
        {
            stack.extend(inner.iter().rev());
        }
    }
    let props = |r: &N| run_own_props(r);
    let (first, last) = (
        runs.first().and_then(|r| props(r)),
        runs.last().and_then(|r| props(r)),
    );
    let el = |local: &str, attrs: Vec<(String, String, String)>, kids: Vec<N>| N::El {
        ns: ns.clone(),
        local: local.to_owned(),
        attrs,
        kids,
    };
    let run = |rpr: &Option<N>, c: N| el("r", Vec::new(), rpr.iter().cloned().chain([c]).collect());
    let kind = |k: &str| (ns.clone(), "fldCharType".to_owned(), k.to_owned());
    let mut begin_attrs = vec![kind("begin")];
    begin_attrs.extend(
        attrs
            .iter()
            .filter(|a| a.0 == ns && matches!(a.1.as_str(), "fldLock" | "dirty"))
            .cloned(),
    );
    begin_attrs.sort();
    let instr = attrs
        .iter()
        .find(|a| a.0 == ns && a.1 == "instr")
        .map(|a| a.2.clone())
        .unwrap_or_default();
    let (data, content): (Vec<N>, Vec<N>) = kids.into_iter().partition(|k| is(k, "fldData"));
    let opening = vec![
        run(&first, el("fldChar", begin_attrs, data)),
        run(&first, el("instrText", Vec::new(), vec![N::Text(instr)])),
        run(&first, el("fldChar", vec![kind("separate")], Vec::new())),
    ];
    let closing = vec![run(&last, el("fldChar", vec![kind("end")], Vec::new()))];
    // A field whose content is all in insertions has its runs in copies of the first and the last.
    let markers = [
        "bookmarkStart",
        "bookmarkEnd",
        "commentRangeStart",
        "commentRangeEnd",
        "permStart",
        "permEnd",
        "proofErr",
    ];
    let inner: Vec<&N> = content
        .iter()
        .filter(|k| !markers.iter().any(|m| is(k, m)))
        .collect();
    let copy = |k: Option<&&N>, runs: Vec<N>| match k {
        Some(N::El { attrs, .. }) => vec![el("ins", attrs.clone(), runs)],
        _ => runs,
    };
    let (opening, closing) = if !inner.is_empty() && inner.iter().all(|k| is(k, "ins")) {
        (copy(inner.first(), opening), copy(inner.last(), closing))
    } else {
        (opening, closing)
    };
    let mut out = opening;
    out.extend(content);
    out.extend(closing);
    out
}

/// The simple fields of the main part that the batch's tracked deletions wrote as complex fields. Rejecting such a deletion
/// gives back a complex field where the original had a `w:fldSimple` (design `revisions.md`, 3.6.3), so R1 and R2 read
/// exactly these fields of the original in that form, and every other field as it is.
pub(super) fn unfolded_of(outcome: &super::outcome::Outcome) -> Vec<crate::wordml::NodeId> {
    outcome
        .effects
        .iter()
        .flat_map(|e| e.unfolded.iter())
        .filter(|(part, _)| &**part == super::tests_support::MAIN)
        .map(|(_, node)| *node)
        .collect()
}
