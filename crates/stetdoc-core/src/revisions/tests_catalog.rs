//! The catalog of shapes (design 8.2d) as a corpus-gated test: named documents of the corpus and the
//! changes that `changes()` finds in each, by kind. The documents are the ones of the curated set that
//! hold the shapes of 1.3 between them: a paragraph deleted, inserted and split, a row, a cell, a merged
//! cell, a move with and without range markers, formatting at every level, a numbering change, the markers
//! around a content control, and a text box whose revision Word writes twice. A document that the set
//! does not hold is skipped, so a smaller set passes. When the corpus changes, or Word's behaviour is
//! learned from the first oracle run, a row changes here.

use super::changes::changes;
use super::index::revision_parts;
use crate::api::limits::Limits;
use crate::state::State;
use std::collections::BTreeMap;
use stetdoc_testkit::corpus::Corpus;

/// A document, and the number of changes of each kind that `changes()` finds in all its stories.
const CATALOG: &[(&str, &[(&str, usize)])] = &[
    (
        "adeu:shared/cross_platform_tests/08_agent_nested_revisions/input.docx",
        &[("Deletion", 2), ("Insertion", 3), ("ParagraphInserted", 1)],
    ),
    (
        "docx-cli:.claude/skills/weak-agent-test/scenarios/contract-finalize/contract-redlined.docx",
        &[
            ("Deletion", 4),
            ("Insertion", 3),
            ("ParagraphDeleted", 1),
            ("ParagraphFormat", 1),
        ],
    ),
    (
        "libreoffice-core:sw/qa/extras/ooxmlexport/data/n830205.docx",
        &[
            ("Deletion", 17),
            ("Insertion", 23),
            ("MarkDeleted", 1),
            ("ParagraphDeleted", 47),
            ("ParagraphFormat", 56),
            ("ParagraphInserted", 2),
            ("RowDeleted", 22),
            ("RunFormat", 325),
            ("SectionFormat", 1),
        ],
    ),
    (
        "libreoffice-core:sw/qa/extras/ooxmlexport/data/rhbz1180114.docx",
        &[("Deletion", 1), ("Insertion", 1), ("Move", 2)],
    ),
    (
        "libreoffice-core:sw/qa/extras/ooxmlexport/data/tdf89731.docx",
        &[("NumberingChange", 2)],
    ),
    (
        "libreoffice-core:sw/qa/extras/ooxmlexport/data/tdf123460.docx",
        &[("Move", 2), ("RunFormat", 2)],
    ),
    (
        "libreoffice-core:sw/qa/writerfilter/dmapper/data/redlined-shape-sdt.docx",
        &[("Insertion", 1)],
    ),
    (
        "officecli:examples/word/revisions.docx",
        &[
            ("CellDeleted", 1),
            ("CellFormat", 1),
            ("CellInserted", 1),
            ("Deletion", 4),
            ("Insertion", 4),
            ("Move", 1),
            ("ParagraphDeleted", 1),
            ("ParagraphFormat", 2),
            ("ParagraphInserted", 2),
            ("RowFormat", 1),
            ("RowInserted", 1),
            ("RunFormat", 8),
            ("SectionFormat", 1),
            ("TableFormat", 4),
        ],
    ),
    (
        "open-xml-powertools:TestFiles/RA001-Tracked-Revisions-02.docx",
        &[
            ("CellFormat", 3),
            ("Deletion", 4),
            ("GridChange", 1),
            ("Insertion", 2),
            ("MarkInserted", 2),
            ("Move", 1),
            ("ParagraphDeleted", 38),
            ("ParagraphFormat", 1),
            ("ParagraphInserted", 57),
            ("RowDeleted", 10),
            ("RowFormat", 1),
            ("RowInserted", 10),
            ("TableFormat", 1),
        ],
    ),
    (
        "open-xml-powertools:TestFiles/RP/RP036-Vert-Merged-Cells.docx",
        &[
            ("CellFormat", 9),
            ("CellMerged", 3),
            ("Deletion", 2),
            ("GridChange", 1),
            ("ParagraphInserted", 1),
            ("TableFormat", 1),
        ],
    ),
];

/// The changes of every story part of a document, counted by kind.
fn kinds(bytes: Vec<u8>) -> BTreeMap<String, usize> {
    let state = State::open(bytes, &Limits::default()).expect("a catalog document opens");
    let tx = state.begin();
    let view = state.parts();
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for name in revision_parts(&tx).expect("the parts that hold revisions") {
        let Some(id) = view.id(&name) else { continue };
        if let (Ok(story), Ok(tree)) = (view.story(id), view.tree(id)) {
            for c in changes(&story, tree, None) {
                *out.entry(format!("{:?}", c.kind)).or_default() += 1;
            }
        }
    }
    out
}

#[test]
fn the_documents_of_the_catalog_group_into_the_changes_they_hold() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut wrong = Vec::new();
    let mut seen = 0;
    for (id, want) in CATALOG {
        let Some(entry) = corpus.entries.iter().find(|e| e.id == *id) else {
            eprintln!("catalog: {id} is not in this set, skipped");
            continue;
        };
        seen += 1;
        let got = kinds(entry.bytes());
        let want: BTreeMap<String, usize> =
            want.iter().map(|(k, n)| ((*k).to_owned(), *n)).collect();
        if got != want {
            wrong.push(format!("{id}\n  want {want:?}\n  got  {got:?}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} documents differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!("catalog: {seen} of {} documents checked", CATALOG.len());
}
