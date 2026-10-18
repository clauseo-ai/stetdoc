//! Tests of the plan: edits, insertions and deletions with their places, rows, splits, merges, field groups, formatting and
//! new notes, on documents built with the fixture builder.

use super::model::Atoms;
use super::plan::{Entry, Plan, plan};
use super::plan_para::PlanEdit;
use super::stories::pair_documents;
use super::tests_pairing::{note, p, sentence, state, table, with_notes, with_ref};
use crate::wordml::StoryKind;

fn plan_of(a: &str, b: &str) -> Plan {
    let (sa, sb) = (state(a), state(b));
    let pairings = pair_documents(&sa, &sb, &mut Atoms::default()).unwrap();
    let pl = plan(&pairings, &sa, &sb, &|x, y| x == y).unwrap();
    assert_covered(&pairings, &pl);
    pl
}

fn body(plan: &Plan) -> &super::plan::StoryPlan {
    plan.stories
        .iter()
        .find(|s| s.kind == StoryKind::Body)
        .unwrap()
}

fn rp(rpr: &str, text: &str) -> String {
    format!("<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

#[test]
fn equal_documents_plan_nothing() {
    let a = format!(
        "{}{}{}",
        p(&sentence(1)),
        table(&[&["x one two", "y"]]),
        p(&sentence(2))
    );
    let pl = plan_of(&a, &a);
    assert!(body(&pl).entries.is_empty());
    assert_eq!(
        body(&pl).kept,
        4,
        "two paragraphs and the two cells of the row"
    );
}

#[test]
fn a_replaced_word_is_an_edit_with_both_ranges() {
    let a = p("The supplier shall deliver the goods within thirty days of the order");
    let b = p("The supplier shall deliver the goods within sixty days of the order");
    let pl = plan_of(&a, &b);
    let [Entry::Edit(e)] = &body(&pl).entries[..] else {
        panic!("{:?}", body(&pl).entries)
    };
    let at = "The supplier shall deliver the goods within "
        .chars()
        .count();
    assert_eq!(
        e.edits,
        vec![PlanEdit {
            insert: at..at + 5,
            deleted: Some(at..at + 6)
        }]
    );
    assert!(!e.ppr && !e.mark && e.fmt.is_empty());
}

#[test]
fn deletions_come_before_insertions_and_each_is_one_run() {
    let a = format!(
        "{}{}{}{}",
        p(&sentence(1)),
        p("Old clause about notices to the registered office of the buyer"),
        p("Another old clause about payment of invoices by the buyer"),
        p(&sentence(2))
    );
    let b = format!(
        "{}{}{}",
        p(&sentence(1)),
        p("Entirely new text on audit rights of the seller during the term"),
        p(&sentence(2))
    );
    let pl = plan_of(&a, &b);
    let es = &body(&pl).entries;
    assert_eq!(es.len(), 2, "{es:?}");
    let Entry::Delete {
        blocks,
        at,
        trailing,
    } = &es[0]
    else {
        panic!("{es:?}")
    };
    assert_eq!(blocks.len(), 2);
    assert!(at.before && trailing.is_none());
    assert!(matches!(&es[1], Entry::Insert { blocks } if blocks.len() == 1));
}

#[test]
fn deletions_at_the_end_go_after_the_last_live_paragraph_and_mark_it() {
    let a = format!(
        "{}{}{}",
        p(&sentence(1)),
        p("Final clause one of the agreement goes away here today"),
        p("Final clause two of the agreement goes away here too")
    );
    let b = p(&sentence(1));
    let pl = plan_of(&a, &b);
    let es = &body(&pl).entries;
    let Some(Entry::Delete {
        blocks,
        at,
        trailing,
    }) = es.iter().find(|e| matches!(e, Entry::Delete { .. }))
    else {
        panic!("{es:?}")
    };
    assert_eq!(blocks.len(), 2);
    assert!(!at.before);
    assert_eq!(*trailing, Some(at.anchor));
    assert!(
        es.iter()
            .any(|e| matches!(e, Entry::Edit(x) if x.trailing_mark && x.b == Some(at.anchor)))
    );
}

#[test]
fn rows_inserted_and_deleted_are_entries_next_to_a_revised_row() {
    let a = table(&[
        &["first row text", "x"],
        &["gone row text", "y"],
        &["third row text", "z"],
    ]);
    let b = table(&[
        &["first row text", "x"],
        &["third row text", "z"],
        &["new last row", "w"],
    ]);
    let pl = plan_of(&a, &b);
    let es = &body(&pl).entries;
    assert!(
        es.iter().any(
            |e| matches!(e, Entry::Delete { blocks, at, .. } if blocks.len() == 1 && at.before)
        )
    );
    assert!(
        es.iter()
            .any(|e| matches!(e, Entry::Insert { blocks } if blocks.len() == 1))
    );
}

#[test]
fn a_cell_edit_is_planned_through_the_paired_row() {
    let a = table(&[&["first row text", "the old words here"]]);
    let b = table(&[&["first row text", "the new words here"]]);
    let pl = plan_of(&a, &b);
    assert!(matches!(&body(&pl).entries[..], [Entry::Edit(e)] if e.edits.len() == 1));
}

#[test]
fn bold_added_to_unchanged_text_is_a_formatting_change_and_a_jc_change_is_a_paragraph_one() {
    let a = format!(
        "<w:p>{}{}</w:p>",
        rp("", "Payment is due in "),
        rp("", "thirty days")
    );
    let b = format!(
        "<w:p>{}{}</w:p>",
        rp("", "Payment is due in "),
        rp("<w:b/>", "thirty days")
    );
    let pl = plan_of(&a, &b);
    let [Entry::Edit(e)] = &body(&pl).entries[..] else {
        panic!("{:?}", body(&pl).entries)
    };
    assert!(e.edits.is_empty());
    assert_eq!(e.fmt.len(), 1);
    assert_eq!(e.fmt[0].b, 18..29);
    assert_eq!(e.fmt[0].a, 18..29);
    let a = "<w:p><w:r><w:t>Centred heading here</w:t></w:r></w:p>".to_owned();
    let b = "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>Centred heading here</w:t></w:r></w:p>".to_owned();
    let pl = plan_of(&a, &b);
    let [Entry::Edit(e)] = &body(&pl).entries[..] else {
        panic!()
    };
    assert!(e.ppr && e.edits.is_empty());
    // what is not a difference
    let b = "<w:p><w:pPr><w:rPr><w:lang w:val=\"en-GB\"/></w:rPr></w:pPr><w:r><w:rPr><w:noProof/></w:rPr><w:t>Centred heading here</w:t></w:r></w:p>".to_owned();
    let pl = plan_of(&a, &b);
    assert!(body(&pl).entries.is_empty());
}

#[test]
fn a_split_gives_each_revised_paragraph_its_edits_and_marks_the_first_parts() {
    let whole = "The supplier shall deliver the goods within thirty days of the order and shall keep complete records of every delivery for six years";
    let (first, second) = whole.split_at(whole.find(" and shall").unwrap());
    let a = format!("{}{}{}", p(&sentence(1)), p(whole), p(&sentence(2)));
    let b = format!(
        "{}{}{}{}",
        p(&sentence(1)),
        p(first),
        p(&format!("{}!", second.trim())),
        p(&sentence(2))
    );
    let pl = plan_of(&a, &b);
    let [Entry::Split(s)] = &body(&pl).entries[..] else {
        panic!("{:?}", body(&pl).entries)
    };
    assert_eq!(s.parts.len(), 2);
    assert!(s.parts[0].mark_inserted && !s.parts[1].mark_inserted);
    // the second part gains the "!" at its end, and the leading space of the original is a deletion
    assert!(s.parts[1].edits.iter().any(|e| e.insert.len() == 1));
}

#[test]
fn a_merge_is_cut_where_the_original_paragraphs_met() {
    let whole = "The supplier shall deliver the goods within thirty days of the order and shall keep complete records of every delivery for six years";
    let (first, second) = whole.split_at(whole.find(" and shall").unwrap());
    let a = format!(
        "{}{}{}{}",
        p(&sentence(1)),
        p(first),
        p(second.trim()),
        p(&sentence(2))
    );
    let b = format!("{}{}{}", p(&sentence(1)), p(whole), p(&sentence(2)));
    let pl = plan_of(&a, &b);
    let [Entry::Merge(m)] = &body(&pl).entries[..] else {
        panic!("{:?}", body(&pl).entries)
    };
    assert_eq!(m.parts.len(), 2);
    assert_eq!(m.cuts.len(), 1);
    // the original paragraphs met at the end of `first`; the joined text has a space there, which the word diff glues to the
    // word before, so the cut is at the start of that replacement or just after the space
    let met = first.chars().count();
    assert!(
        m.cuts[0] <= met + 1 && m.cuts[0] + 8 >= met,
        "cut at {}",
        m.cuts[0]
    );
}

#[test]
fn a_toc_with_one_changed_page_number_is_unpaired_whole() {
    let toc = |n: u32| {
        format!(
            "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> TOC </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>Heading one 1</w:t></w:r></w:p><w:p><w:r><w:t>Heading two {n}</w:t></w:r></w:p><w:p><w:r><w:t>Heading three 3</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>"
        )
    };
    let a = format!(
        "{}{}{}",
        p("Before the table of contents"),
        toc(2),
        p("After the table of contents")
    );
    let b = format!(
        "{}{}{}",
        p("Before the table of contents"),
        toc(5),
        p("After the table of contents")
    );
    let pl = plan_of(&a, &b);
    let es = &body(&pl).entries;
    assert!(
        es.iter()
            .any(|e| matches!(e, Entry::Delete { blocks, .. } if blocks.len() == 3)),
        "{es:?}"
    );
    assert!(
        es.iter()
            .any(|e| matches!(e, Entry::Insert { blocks } if blocks.len() == 3)),
        "{es:?}"
    );
    assert!(!es.iter().any(|e| matches!(e, Entry::Edit(_))));
    // the same TOC is kept
    let pl = plan_of(&a, &a);
    assert!(body(&pl).entries.is_empty());
}

#[test]
fn a_new_note_is_inserted_with_its_reference() {
    let a = with_notes(
        &with_ref(
            "The seller warrants the title to the goods sold under this agreement",
            1,
        ),
        &[note(1, "See the schedule")],
    );
    let b = with_notes(
        &format!(
            "{}{}",
            with_ref(
                "The seller warrants the title to the goods sold under this agreement",
                1
            ),
            with_ref(
                "A brand new sentence with its own footnote reference here",
                2
            )
        ),
        &[note(1, "See the schedule"), note(2, "A new note")],
    );
    let pairings = pair_documents(&a, &b, &mut Atoms::default()).unwrap();
    let pl = plan(&pairings, &a, &b, &|x, y| x == y).unwrap();
    let notes = pl
        .stories
        .iter()
        .find(|s| s.kind == StoryKind::Footnotes)
        .unwrap();
    assert!(
        matches!(&notes.entries[..], [Entry::Insert { blocks }] if blocks.len() == 1),
        "{:?}",
        notes.entries
    );
}

/// The coverage law of the plan: every block of both sides is accounted for exactly once. An original block is paired, in a
/// split or a merge, or deleted by an entry, and a revised block is paired, in a group, or inserted. The rows of two paired
/// tables are held to the same.
pub(super) fn assert_covered(pairings: &super::stories::Pairings, plan: &Plan) {
    use super::model::Body;
    use std::collections::HashSet;
    for (story, sp) in pairings.stories.iter().zip(&plan.stories) {
        let mut deleted: HashSet<crate::wordml::NodeId> = HashSet::new();
        let mut inserted: HashSet<crate::wordml::NodeId> = HashSet::new();
        for e in &sp.entries {
            match e {
                Entry::Delete { blocks, .. } => {
                    for b in blocks {
                        assert!(deleted.insert(*b), "a block is deleted twice");
                    }
                }
                Entry::Insert { blocks } => {
                    for b in blocks {
                        assert!(inserted.insert(*b), "a block is inserted twice");
                    }
                }
                _ => {}
            }
        }
        for root in &story.roots {
            for (ci, c) in root.pairing.containers.iter().enumerate() {
                let (Some(ca), Some(cb)) = (
                    story.model_a.containers.get(c.ca),
                    story.model_b.containers.get(c.cb),
                ) else {
                    continue;
                };
                let groups: Vec<_> = root
                    .regrouped
                    .groups
                    .iter()
                    .filter(|g| g.pairing == ci)
                    .collect();
                // pairs that a field group dissolved are not in the plan's pairs, so an unpaired block may be in neither list:
                // it is then in a dissolved group, and the plan deletes or inserts it, which the sets say
                for (i, n) in ca.blocks.iter().enumerate() {
                    let paired =
                        c.pairs.iter().any(|p| p.a == i) || groups.iter().any(|g| g.a.contains(&i));
                    let gone = deleted.contains(&n.id);
                    assert!(
                        paired || gone,
                        "an original block is lost: {} of container {ci}",
                        i
                    );
                    if let Body::Table(rows) = &n.body {
                        let pair = c.pairs.iter().find(|p| p.a == i);
                        if let Some(rp) = pair.and_then(|p| p.rows.as_ref()) {
                            for (ri, row) in rows.iter().enumerate() {
                                assert!(
                                    rp.pairs.iter().any(|r| r.a == ri) || deleted.contains(&row.id),
                                    "an original row is lost"
                                );
                            }
                        }
                    }
                }
                for (j, n) in cb.blocks.iter().enumerate() {
                    let paired =
                        c.pairs.iter().any(|p| p.b == j) || groups.iter().any(|g| g.b.contains(&j));
                    assert!(
                        paired || inserted.contains(&n.id),
                        "a revised block is lost: {j} of container {ci}"
                    );
                    if let Body::Table(rows) = &n.body {
                        let pair = c.pairs.iter().find(|p| p.b == j);
                        if let Some(rp) = pair.and_then(|p| p.rows.as_ref()) {
                            for (ri, row) in rows.iter().enumerate() {
                                assert!(
                                    rp.pairs.iter().any(|r| r.b == ri)
                                        || inserted.contains(&row.id),
                                    "a revised row is lost"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
