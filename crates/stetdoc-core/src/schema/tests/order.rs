//! Tests of `insert_in_order` and `reorder`.

use super::super::order::{Unit, sort_units};
use super::*;

fn ppr() -> Option<Ctx> {
    Ctx::from_path(PartKind::Document, &[w("body"), w("p"), w("pPr")])
}

fn valid_at(index: usize) -> Placement {
    Placement {
        index,
        fit: Fit::Valid,
    }
}

#[test]
fn ppr_children_have_exactly_one_slot() {
    let ctx = ppr();
    assert!(ctx.is_some());
    let sibs = [w("pStyle"), w("spacing"), w("jc")];
    assert_eq!(place(ctx, &sibs, w("numPr"), Prefer::Last), valid_at(1));
    assert_eq!(place(ctx, &sibs, w("numPr"), Prefer::First), valid_at(1));
    // before everything, after everything
    assert_eq!(
        place(ctx, &[w("spacing")], w("pStyle"), Prefer::Last).index,
        0
    );
    assert_eq!(
        place(ctx, &[w("pStyle"), w("jc")], w("pPrChange"), Prefer::First).index,
        2
    );
    // the paragraph-mark run properties come after jc and before sectPr
    assert_eq!(
        place(
            ctx,
            &[w("pStyle"), w("jc"), w("sectPr")],
            w("rPr"),
            Prefer::Last
        )
        .index,
        2
    );
}

#[test]
fn second_copy_of_a_single_element_has_no_valid_slot() {
    let p = place(ppr(), &[w("pStyle"), w("jc")], w("jc"), Prefer::Last);
    assert_eq!(p.fit, Fit::NoValidSlot);
}

#[test]
fn a_second_copy_is_refused_even_when_the_parent_is_invalid() {
    // The automaton cannot say anything about children that are already out of order, so the
    // refusal comes from the "once" bit the generator emits per symbol. Each of these is a
    // parent that is invalid, and an element that may occur once, already there.
    let cases: [(&[&str], &str, &[&str]); 8] = [
        (&["body", "p"], "pPr", &["r", "pPr"]),
        (&["body", "p", "r"], "rPr", &["t", "rPr"]),
        (&["body", "tbl", "tr", "tc"], "tcPr", &["p", "tcPr"]),
        (&["body", "tbl", "tr"], "trPr", &["tc", "trPr"]),
        (&["body", "sdt"], "sdtPr", &["sdtContent", "sdtPr"]),
        (
            &["body", "sdt"],
            "sdtContent",
            &["sdtContent", "sdtContent"],
        ),
        (&["body", "tbl"], "tblPr", &["tr", "tblPr"]),
        (&["body", "tbl"], "tblGrid", &["tr", "tblGrid"]),
    ];
    for (path, new, kids) in cases {
        let names: Vec<QName<'_>> = path.iter().map(|n| w(n)).collect();
        let ctx = Ctx::from_path(PartKind::Document, &names);
        let sibs: Vec<QName<'_>> = kids.iter().map(|n| w(n)).collect();
        let tree = ListTree::new(&sibs);
        let before = insert_in_order(ctx, &tree, 0, w("t"), Prefer::Last).fit;
        let got = place(ctx, &sibs, w(new), Prefer::Last);
        assert_eq!(
            got.fit,
            Fit::NoValidSlot,
            "{path:?} {new} among {kids:?} ({before:?})"
        );
    }
    // while an element that may repeat is not refused, in a parent that is invalid either
    let repeats: [(&[&str], &str, &[&str]); 6] = [
        (&["body", "p"], "r", &["r", "pPr"]),
        (&["body"], "p", &["p", "p"]),
        (&["body", "tbl"], "tr", &["tblPr", "tblGrid", "tr"]),
        (&["body", "tbl"], "tr", &["tr", "tblPr"]),
        (&["body", "tbl", "tblGrid"], "gridCol", &["gridCol"]),
        (&["body", "tbl", "tr", "tc"], "p", &["tcPr", "p"]),
    ];
    for (path, new, kids) in repeats {
        let names: Vec<QName<'_>> = path.iter().map(|n| w(n)).collect();
        let ctx = Ctx::from_path(PartKind::Document, &names);
        let sibs: Vec<QName<'_>> = kids.iter().map(|n| w(n)).collect();
        let got = place(ctx, &sibs, w(new), Prefer::Last);
        assert_ne!(got.fit, Fit::NoValidSlot, "{path:?} {new} among {kids:?}");
    }
}

#[test]
fn element_the_model_does_not_name_is_not_allowed() {
    let p = place(ppr(), &[w("pStyle")], w("tbl"), Prefer::Last);
    assert_eq!(
        p,
        Placement {
            index: 1,
            fit: Fit::NotAllowed
        }
    );
}

#[test]
fn unknown_siblings_do_not_block_and_prefer_picks_among_slots() {
    let x = ("urn:example:unknown", "thing");
    let sibs = [w("pStyle"), x, w("jc")];
    let ctx = ppr();
    assert_eq!(place(ctx, &sibs, w("numPr"), Prefer::First), valid_at(1));
    assert_eq!(place(ctx, &sibs, w("numPr"), Prefer::Last), valid_at(2));
    // an unknown element in a namespace that the document lists as ignorable behaves the same
    let w14x = (W14, "someFutureElement");
    let sibs = [w("pStyle"), w14x, w("jc")];
    assert_eq!(place(ctx, &sibs, w("ind"), Prefer::Last).fit, Fit::Valid);
}

#[test]
fn a_property_container_out_of_order_is_rewritten_not_patched() {
    // jc before pStyle is already wrong. Whatever index `ind` gets, a new pair would be out of
    // order, so the container is rewritten: pStyle, ind, jc.
    let x = ("urn:example:unknown", "thing");
    let sibs = [w("jc"), x, w("pStyle")];
    let order = vec![Some(2), None, Some(0), Some(1)];
    // x stays behind the jc it followed; the new element sits between pStyle and jc
    assert_eq!(
        place(ppr(), &sibs, w("ind"), Prefer::Last),
        Placement {
            index: 1,
            fit: Fit::Reorder(order.clone())
        }
    );
    let tree = ListTree::new(&sibs);
    assert_eq!(reorder(ppr(), &tree, 0, Some(w("ind"))), Some(order));
    // the same rule applies to a table's properties, the case python-docx documents are in
    let tbl_pr = Ctx::from_path(PartKind::Document, &[w("body"), w("tbl"), w("tblPr")]);
    let sibs = [w("tblW"), w("jc"), w("tblLook"), w("tblBorders")];
    assert!(matches!(
        place(tbl_pr, &sibs, w("tblLayout"), Prefer::Last).fit,
        Fit::Reorder(_)
    ));
    let order =
        reorder(tbl_pr, &ListTree::new(&sibs), 0, None).expect("tblPr is a property container");
    assert_eq!(order, vec![Some(0), Some(1), Some(3), Some(2)]);
    // a valid container never asks for a rewrite
    assert_eq!(
        place(ppr(), &[w("pStyle"), w("jc")], w("ind"), Prefer::Last).fit,
        Fit::Valid
    );
    // content is not rewritten
    let body = Ctx::from_path(PartKind::Document, &[w("body")]);
    assert!(reorder(body, &ListTree::new(&[w("p")]), 0, None).is_none());
}

#[test]
fn reorder_is_the_identity_on_a_container_in_order() {
    // So that a caller may apply it after every removal. In order means: ranks never go down.
    let sibs = [w("pStyle"), w("numPr"), w("spacing"), w("jc"), w("rPr")];
    let order = reorder(ppr(), &ListTree::new(&sibs), 0, None).expect("a property container");
    assert_eq!(order, (0..5).map(Some).collect::<Vec<_>>());
    // and after a removal from one that was out of order it is the order that is left: jc,
    // ind, pStyle with jc taken out
    let order = reorder(ppr(), &ListTree::new(&[w("ind"), w("pStyle")]), 0, None);
    assert_eq!(order, Some(vec![Some(1), Some(0)]));
}

#[test]
fn a_wrapper_that_spans_ranks_is_not_pulled_apart() {
    // The wrapper stands for jc and ind, which are out of order inside it. spacing ranks below
    // both, so it must come first; the wrapper itself stays whole and where it is relative to
    // what its ranks overlap.
    let xml = doc_xml(
        r#"<w:p><w:pPr><w:pStyle w:val="A"/><mc:AlternateContent><mc:Choice Requires="w14"><w:jc w:val="left"/><w:ind w:left="0"/></mc:Choice></mc:AlternateContent><w:spacing w:before="0"/></w:pPr></w:p>"#,
    );
    let tree = parse(&xml);
    let ppr = nth(&tree, "pPr", 0);
    let order = reorder(ppr_ctx(), &tree, ppr, None).expect("a property container");
    assert_eq!(order, vec![Some(0), Some(2), Some(1)]);
}

fn ppr_ctx() -> Option<Ctx> {
    ppr()
}

#[test]
fn out_of_order_content_gets_the_fewest_inversions() {
    // rPr after content is wrong; w:r is content, not a property container, so there is no
    // rewrite and the new element goes where it adds the fewest inversions
    let r = Ctx::from_path(PartKind::Document, &[w("body"), w("p"), w("r")]);
    let sibs = [w("t"), w("rPr")];
    let p = place(r, &sibs, w("br"), Prefer::First);
    assert_eq!(p.fit, Fit::BestEffort);
    assert_eq!(p.index, 2);
}

#[test]
fn a_table_without_properties_gets_them_first() {
    // In w:tbl every symbol shares one canonical rank (range markup lets tblPr, tblGrid and tr
    // interleave in the strongly connected component), so rank cannot tell where tblPr goes.
    // The content model can: the only place that makes the table valid is the front.
    let tbl = Ctx::from_path(PartKind::Document, &[w("body"), w("tbl")]);
    assert_eq!(
        place(tbl, &[w("tblGrid"), w("tr")], w("tblPr"), Prefer::Last),
        valid_at(0)
    );
    assert_eq!(
        place(tbl, &[w("tblPr"), w("tr")], w("tblGrid"), Prefer::Last),
        valid_at(1)
    );
    assert_eq!(
        place(tbl, &[w("tblPr"), w("tblGrid")], w("tr"), Prefer::First),
        valid_at(2)
    );
    // rows only: neither property element alone repairs the table, so both go where they add
    // the fewest failures and the answer is honest about it
    let got = place(tbl, &[w("tr"), w("tr")], w("tblGrid"), Prefer::First);
    assert_eq!(got.fit, Fit::BestEffort);
}

#[test]
fn repeated_content_has_many_slots_and_required_elements_are_respected() {
    let body = Ctx::from_path(PartKind::Document, &[w("body")]);
    // paragraphs go before the trailing section properties, wherever else is fine
    let sibs = [w("p"), w("tbl"), w("p"), w("sectPr")];
    assert_eq!(place(body, &sibs, w("p"), Prefer::Last), valid_at(3));
    assert_eq!(place(body, &sibs, w("p"), Prefer::First), valid_at(0));
    assert_eq!(place(body, &sibs, w("p"), Prefer::Before(2)).index, 2);
    assert_eq!(place(body, &sibs, w("p"), Prefer::After(1)).index, 2);
    // sectPr itself can only go last
    assert_eq!(
        place(body, &[w("p"), w("p")], w("sectPr"), Prefer::First).index,
        2
    );
    // a table cell needs its properties first and ends with a paragraph
    let tc = Ctx::from_path(PartKind::Document, &[w("body"), w("tbl"), w("tr"), w("tc")]);
    assert!(tc.is_some());
    assert_eq!(place(tc, &[w("p")], w("tcPr"), Prefer::Last), valid_at(0));
    assert_eq!(
        place(tc, &[w("tcPr"), w("p")], w("p"), Prefer::Last).index,
        2
    );
}

#[test]
fn paragraph_properties_come_first_and_runs_follow() {
    let p = Ctx::from_path(PartKind::Document, &[w("body"), w("p")]);
    assert_eq!(
        place(p, &[w("r"), w("hyperlink")], w("pPr"), Prefer::Last),
        valid_at(0)
    );
    assert_eq!(place(p, &[w("pPr"), w("r")], w("r"), Prefer::Last).index, 2);
    assert_eq!(
        place(p, &[w("pPr"), w("r")], w("ins"), Prefer::After(1)).index,
        2
    );
    // run properties likewise
    let r = Ctx::from_path(PartKind::Document, &[w("body"), w("p"), w("r")]);
    assert_eq!(place(r, &[w("t")], w("rPr"), Prefer::Last).index, 0);
    assert_eq!(place(r, &[w("rPr"), w("t")], w("t"), Prefer::Last).index, 2);
}

#[test]
fn run_property_order_and_extension_elements() {
    let rpr = Ctx::from_path(PartKind::Document, &[w("body"), w("p"), w("r"), w("rPr")]);
    assert!(rpr.is_some());
    let sibs = [w("rStyle"), w("b"), w("color"), w("sz")];
    assert_eq!(place(rpr, &sibs, w("i"), Prefer::Last).index, 2);
    assert_eq!(place(rpr, &sibs, w("rFonts"), Prefer::Last).index, 1);
    assert_eq!(place(rpr, &sibs, w("lang"), Prefer::Last).index, 4);
    // Office 2010 text effects come after the base properties
    assert_eq!(
        place(rpr, &sibs, (W14, "ligatures"), Prefer::Last),
        valid_at(4)
    );
}

fn wrapped_run(requires: &str, fallback: &str) -> String {
    doc_xml(&format!(
        r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="{requires}"><w:drawing><wp:inline/></w:drawing></mc:Choice>{fallback}</mc:AlternateContent><w:t/></w:r></w:p>"#
    ))
}

#[test]
fn alternate_content_stands_for_the_branch_word_reads() {
    let fallback = "<mc:Fallback><w:pict/></mc:Fallback>";
    // wps is a namespace the tables know, so the Choice is taken and the wrapper stands for
    // the drawing: rPr goes before it
    let xml = wrapped_run("wps", fallback);
    assert_eq!(
        place_in_xml(PartKind::Document, &xml, "r", w("rPr"), Prefer::Last),
        valid_at(0)
    );
    // a Choice that needs a namespace nobody knows is not taken; the Fallback is, and a pict
    // is run content just like a drawing
    let xml = wrapped_run("x", fallback);
    assert_eq!(
        place_in_xml(PartKind::Document, &xml, "r", w("rPr"), Prefer::Last),
        valid_at(0)
    );
    // with no Fallback the wrapper stands for nothing: rPr may go on either side of it, and
    // only before the text
    let xml = wrapped_run("x", "");
    assert_eq!(
        place_in_xml(PartKind::Document, &xml, "r", w("rPr"), Prefer::Last),
        valid_at(1)
    );
    assert_eq!(
        place_in_xml(PartKind::Document, &xml, "r", w("rPr"), Prefer::First),
        valid_at(0)
    );
    // the wrapper does not hide a second copy: a numbering level whose numFmt is the wrapper's
    // Choice already has one
    let lvl = format!(
        r#"<w:numbering {NS_DECLS}><w:abstractNum><w:lvl><w:start w:val="1"/><mc:AlternateContent><mc:Choice Requires="w14"><w:numFmt w:val="decimal"/></mc:Choice><mc:Fallback><w:numFmt w:val="decimal"/></mc:Fallback></mc:AlternateContent><w:lvlText w:val="%1"/></w:lvl></w:abstractNum></w:numbering>"#
    );
    let got = place_in_xml(PartKind::Numbering, &lvl, "lvl", w("numFmt"), Prefer::Last);
    assert_eq!(got.fit, Fit::NoValidSlot);
    // and insertion and validation read the same sequence: the document that results from
    // the placement validates, here by hand
    assert!(violations(PartKind::Document, &wrapped_run("wps", fallback)).is_empty());
}

#[test]
fn unknown_parent_follows_the_preference() {
    let sibs = [w("a"), w("b")];
    assert_eq!(
        place(None, &sibs, w("x"), Prefer::Last),
        Placement {
            index: 2,
            fit: Fit::UnknownContext
        }
    );
    assert_eq!(place(None, &sibs, w("x"), Prefer::Before(1)).index, 1);
    // an opaque element (a drawing's inline) has no content model to place children in
    let wp = (
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
        "inline",
    );
    let ext = Ctx::from_path(
        PartKind::Document,
        &[w("body"), w("p"), w("r"), w("drawing"), wp],
    );
    assert!(ext.is_some());
    assert_eq!(
        place(ext, &sibs, w("x"), Prefer::First).fit,
        Fit::UnknownContext
    );
    // while `w:drawing` itself has a model that does not name `w:x`
    let drawing = Ctx::from_path(
        PartKind::Document,
        &[w("body"), w("p"), w("r"), w("drawing")],
    );
    assert_eq!(
        place(drawing, &sibs, w("x"), Prefer::First).fit,
        Fit::NotAllowed
    );
}

/// Exhaustive check against brute force: for every content model, random valid child
/// sequences and every element the model names, `insert_in_order` finds a valid slot exactly
/// when one exists, and the slot it returns validates.
#[test]
fn insert_in_order_agrees_with_brute_force_on_every_model() {
    use super::super::tables::Model;
    let mut rng = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let mut checked = 0usize;
    for decl in 0..super::super::generated::names::DECLS.len() as u16 {
        let Some(model) = Model::of_decl(decl) else {
            continue;
        };
        if model.alpha_name.is_empty() {
            continue;
        }
        let ctx = Ctx::from_decl(decl);
        for _ in 0..6 {
            // a random valid sequence: walk the DFA, stop at an accepting state sometimes
            let mut seq: Vec<usize> = Vec::new();
            let mut st = 0usize;
            for _ in 0..12 {
                let outs: Vec<usize> = (0..model.alpha_name.len())
                    .filter(|c| model.next(st, *c).is_some())
                    .collect();
                if outs.is_empty() || (model.accepts(st) && next() % 4 == 0) {
                    break;
                }
                let c = outs[(next() % outs.len() as u64) as usize];
                seq.push(c);
                st = model.next(st, c).unwrap();
            }
            if !model.accepts(st) {
                continue; // dead end before acceptance; skip this sample
            }
            let name_of = |c: usize| -> (&'static str, &'static str) {
                let (ns, local) = super::super::tables::name_row(model.alpha_name[c]).unwrap();
                (
                    super::super::generated::names::NAMESPACES[*ns as usize].0,
                    *local,
                )
            };
            let sibs: Vec<(&str, &str)> = seq.iter().map(|c| name_of(*c)).collect();
            for new in 0..model.alpha_name.len() {
                let valid_at = |p: usize| -> bool {
                    let mut all = seq.clone();
                    all.insert(p, new);
                    let mut s = Some(0usize);
                    for c in &all {
                        s = s.and_then(|s| model.next(s, *c));
                    }
                    s.is_some_and(|s| model.accepts(s))
                };
                let any_valid = (0..=seq.len()).any(valid_at);
                for prefer in [Prefer::First, Prefer::Last] {
                    let got = place(Some(ctx), &sibs, name_of(new), prefer);
                    if any_valid {
                        assert_eq!(
                            got.fit,
                            Fit::Valid,
                            "{} inserting {:?} into {:?}",
                            ctx.label(),
                            name_of(new),
                            sibs
                        );
                        assert!(
                            valid_at(got.index),
                            "{} slot {} for {:?} in {:?}",
                            ctx.label(),
                            got.index,
                            name_of(new),
                            sibs
                        );
                    } else {
                        assert_ne!(
                            got.fit,
                            Fit::Valid,
                            "{} {:?} {:?}",
                            ctx.label(),
                            name_of(new),
                            sibs
                        );
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 5_000, "only {checked} cases ran");
}

#[test]
fn insertion_reads_mc_process_content_as_the_validator_does() {
    // pPr holds x:wrap, an element of an unknown namespace that the document makes ignorable and
    // lists in mc:ProcessContent, so Word reads its child, a jc. The ind that is added has to go
    // before the jc, that is, before the wrapper. Insertion once saw the wrapper as one unknown
    // element, put the ind after it, and the validator then reported the ind out of order.
    let doc = |root: &str, ppr: &str| {
        format!(
            r#"<w:document {NS_DECLS} {root}><w:body><w:p><w:pPr {ppr}><x:wrap><w:jc w:val="left"/></x:wrap></w:pPr></w:p></w:body></w:document>"#
        )
    };
    let on_root = doc(r#"mc:Ignorable="x" mc:ProcessContent="x:wrap""#, "");
    let on_ppr = doc("", r#"mc:Ignorable="x" mc:ProcessContent="x:wrap""#);
    let on_ppr_wildcard = doc("", r#"mc:Ignorable="x" mc:ProcessContent="x:*""#);
    for xml in [&on_root, &on_ppr, &on_ppr_wildcard] {
        let got = place_in_xml(PartKind::Document, xml, "pPr", w("ind"), Prefer::Last);
        assert_eq!(got, valid_at(0), "{xml}");
        // what the validator says of the result: the ind before the wrapper is fine, after it
        // is not
        let before = xml.replace("<x:wrap>", r#"<w:ind w:left="1"/><x:wrap>"#);
        let after = xml.replace("</x:wrap>", r#"</x:wrap><w:ind w:left="1"/>"#);
        assert!(
            violations(PartKind::Document, &before).is_empty(),
            "{before}"
        );
        let late = violations(PartKind::Document, &after);
        assert_eq!(
            kinds(&late),
            vec![(ViolationKind::OutOfOrder, "w:ind".to_string())]
        );
    }
    // Ignorable and not processed: the wrapper and the jc in it are skipped, the new element
    // may go anywhere, and it goes last when asked.
    let skipped = doc(r#"mc:Ignorable="x""#, "");
    let got = place_in_xml(PartKind::Document, &skipped, "pPr", w("ind"), Prefer::Last);
    assert_eq!(got, valid_at(1));
}

#[test]
fn a_row_into_a_table_with_no_grid_and_no_rows_is_refused() {
    // The one deliberate exception to "an edit is never refused for a problem that was in the
    // input". The table has a tblPr and lacks its grid, so it is invalid (rows are optional). A
    // row can go nowhere without standing before the grid that is missing, which is a kind of
    // failure the table did not have, so insertion refuses and the caller repairs the table
    // first.
    let tbl = Ctx::from_path(PartKind::Document, &[w("body"), w("tbl")]);
    let got = place(tbl, &[w("tblPr")], w("tr"), Prefer::Last);
    assert_eq!(got.fit, Fit::NoValidSlot);
    // the grid is the repair: it goes after the tblPr and leaves a valid table
    let got = place(tbl, &[w("tblPr")], w("tblGrid"), Prefer::Last);
    assert_eq!(got, valid_at(1));
    // and then a row is no problem
    let got = place(tbl, &[w("tblPr"), w("tblGrid")], w("tr"), Prefer::Last);
    assert_eq!(got, valid_at(2));
}

/// The units' order by the rule the scan used to apply: the first unit, in the current order,
/// that none of the units still to be placed must precede, taken again and again.
fn scanned(spans: &[(u16, u16)]) -> Vec<usize> {
    let mut left: Vec<(usize, (u16, u16))> = spans.iter().copied().enumerate().collect();
    let mut out = Vec::new();
    while !left.is_empty() {
        let pick = (0..left.len())
            .find(|&i| {
                let lo = left.get(i).map_or(0, |u| u.1.0);
                !left.iter().enumerate().any(|(j, u)| j != i && u.1.1 < lo)
            })
            .unwrap_or(0);
        out.push(left.remove(pick).0);
    }
    out
}

fn sorted(spans: &[(u16, u16)]) -> Vec<usize> {
    let units = spans
        .iter()
        .enumerate()
        .map(|(i, (lo, hi))| Unit {
            members: vec![i],
            lo: *lo,
            hi: *hi,
        })
        .collect();
    sort_units(units)
        .into_iter()
        .flat_map(|u| u.members)
        .collect()
}

#[test]
fn units_sort_stably_by_rank() {
    // one rank each: a stable sort by rank
    let spans = [(2, 2), (0, 0), (1, 1), (0, 0), (2, 2), (1, 1)];
    assert_eq!(sorted(&spans), vec![1, 3, 2, 5, 0, 4]);
    // A wrapper over ranks 1 to 9 is not ordered against the 3 and the 5 inside its range, so it
    // stays in the place the order leaves it, and only what must come before something moves.
    assert_eq!(sorted(&[(3, 3), (1, 9), (5, 5)]), vec![0, 1, 2]);
    // the 3 must come before the 5; the wrapper is first among what can be placed
    assert_eq!(sorted(&[(5, 5), (1, 9), (3, 3)]), vec![1, 2, 0]);
    // the 0 is below the whole wrapper and below the 5, and goes first
    assert_eq!(sorted(&[(5, 5), (1, 9), (0, 0)]), vec![2, 0, 1]);
    assert_eq!(sorted(&[]), Vec::<usize>::new());
}

#[test]
fn the_sort_gives_what_the_scan_gave_on_any_ranks() {
    // Random units, with wrappers that span ranks, against the rule applied the slow way.
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut below = move |n: u64| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) % n
    };
    for round in 0..3000 {
        let len = 1 + usize::try_from(below(14)).expect("small");
        let spans: Vec<(u16, u16)> = (0..len)
            .map(|_| {
                let lo = u16::try_from(below(8)).expect("small");
                let extra = if below(4) == 0 { below(6) } else { 0 };
                (lo, lo + u16::try_from(extra).expect("small"))
            })
            .collect();
        assert_eq!(sorted(&spans), scanned(&spans), "round {round}: {spans:?}");
    }
}
