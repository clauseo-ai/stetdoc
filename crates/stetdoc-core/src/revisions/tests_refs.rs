//! Refs across transactions: which element a ref names after a batch that deletes, joins or splits, and for
//! what reason a ref is gone. The model is `refs-and-state`: a ref follows its element, a tracked deletion
//! keeps its ref, a merge and a join keep the first, and a ref is retired only when nothing of its block
//! is left to name.

use super::tests_support::*;
use crate::state::State;
use crate::wordml::{BlockKey, Mode};

fn ref_of(s: &State, key: &BlockKey) -> String {
    s.parts()
        .ref_of(key)
        .map(|r| r.to_string())
        .unwrap_or_default()
}

/// Runs a batch by the author under test on a state and commits it.
fn batch(
    s: &State,
    f: impl FnOnce(&mut super::compiler::Compiler<'_, '_>, &[BlockKey]),
) -> (State, crate::state::Committed) {
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    {
        let mut c = tracked(&mut tx);
        f(&mut c, &ks);
        c.finish().expect("the batch finishes");
    }
    tx.commit().expect("the batch commits")
}

#[test]
fn deleting_the_first_half_of_ones_own_split_in_a_later_batch_keeps_its_ref_on_the_paragraph_with_its_text()
 {
    let s1 = state(&p(&r("hello world")));
    let first = para_keys(&s1.begin())[0].clone();
    let (s2, _) = batch(&s1, |c, ks| {
        c.split_block(&ks[0], 5).expect("the split");
    });
    let keys = para_keys(&s2.begin());
    let (p1, p2) = (ref_of(&s2, &keys[0]), ref_of(&s2, &keys[1]));
    assert_eq!(keys[0], first, "the first half keeps the element");
    assert_ne!(p1, p2);
    // The same author deletes the first half, whose mark is her own insertion from the batch before.
    let (s3, committed) = batch(&s2, |c, ks| {
        c.delete_block(&ks[0]).expect("the deletion");
    });
    let after = para_keys(&s3.begin());
    assert_eq!(after.len(), 1, "one paragraph holds both halves now");
    assert_eq!(
        ref_of(&s3, &after[0]),
        p1,
        "the ref of the first half stays on the paragraph"
    );
    assert!(
        committed.retired.iter().all(|(r, _)| r.to_string() != p1),
        "the first half is not retired: its text is a pending deletion in the paragraph"
    );
    assert!(
        committed
            .merged
            .iter()
            .any(|(kept, gone)| kept.to_string() == p1 && gone.to_string() == p2),
        "the second half is merged into the first: {:?}",
        committed.merged
    );
    let view = s3.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    let para = story.paragraph(tree, 0).unwrap();
    assert_eq!(para.text(Mode::Final), " world");
    assert_eq!(para.text(Mode::Original), "hello world");
}

#[test]
fn deleting_a_row_that_holds_ones_own_inserted_paragraphs_removes_them_last_to_first_and_keeps_the_rest()
 {
    // A row of another author holds, in one cell, the author's own insertion of two paragraphs after the last one, in the
    // shape that the cell's last mark stays live (3.2.1).
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc>\
         <w:p><w:pPr><w:rPr><w:ins w:id=\"1\" {AT}/></w:rPr></w:pPr>{}</w:p>\
         <w:p><w:pPr><w:rPr><w:ins w:id=\"3\" {AT}/></w:rPr></w:pPr><w:ins w:id=\"2\" {AT}>{}</w:ins></w:p>\
         <w:p><w:ins w:id=\"4\" {AT}>{}</w:ins></w:p>\
         </w:tc></w:tr></w:tbl><w:p>{}</w:p>",
        r("theirs"),
        new_run("mine one"),
        new_run("mine two"),
        r("after")
    );
    let s = with_compiler(&body, super::stamp::Track::Tracked(stamp()), |c, all, _| {
        vec![c.delete_block(&all[1])]
    });
    s.assert_clean();
    let all: String = s.red.concat();
    // The other author's text is a pending deletion, and the author's own text is gone.
    assert!(all.contains("theirs") && all.contains("after"), "{all}");
    assert!(!all.contains("mine"), "{all}");
    assert_eq!(s.fin, ["", "after"]);
    // The cell still ends with a paragraph.
    assert!(s.xml.contains("</w:p></w:tc>"), "{}", s.xml);
}

#[test]
fn a_second_compiler_in_the_same_transaction_knows_which_ids_are_copies_of_what_existed() {
    // The author's own pending format change from an earlier batch is on one run. A replacement in the
    // middle of the run cuts it in three, and each piece but the first takes a copy of the change with
    // a new id, which is a piece of what existed and not something this transaction wrote.
    let body = p(&format!(
        "<w:r><w:rPr><w:b/><w:rPrChange w:id=\"7\" {AT}><w:rPr/></w:rPrChange></w:rPr><w:t>abcdefgh</w:t></w:r>"
    ));
    let s = state(&body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let mut first = tracked(&mut tx);
    first
        .replace_text(&ks[0], 2..4, "XY")
        .expect("the replacement");
    first.finish().expect("the first compiler finishes");
    let copies: Vec<u32> = tx.copied_annotation_ids().to_vec();
    assert!(!copies.is_empty(), "the carve noted its copies");
    let second = tracked(&mut tx);
    let view = second.tx().parts();
    let id = view.id(MAIN).unwrap();
    let tree = view.tree(id).unwrap();
    let mut seen = 0;
    for rev in crate::wordml::part_revisions(tree) {
        let Some(n) = rev.id.and_then(|i| u32::try_from(i).ok()) else {
            continue;
        };
        if copies.contains(&n) {
            seen += 1;
            assert!(
                !second.is_created_node(tree, rev.node),
                "the copy {n} is not something this transaction created"
            );
        } else if rev.kind == crate::wordml::RevKind::Ins {
            assert!(
                second.is_created_node(tree, rev.node),
                "the insertion {n} was"
            );
        }
    }
    assert_eq!(seen, copies.len());
}
