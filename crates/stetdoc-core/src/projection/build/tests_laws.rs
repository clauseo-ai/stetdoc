//! The model laws M1 to M7 and the file laws on built models: what the build makes of a document, rendered
//! by each format, parses back to its draft (F2), and the alignment of the unedited files has no operation (T1).

use super::testing::*;
use crate::api::limits::Limits;
use crate::api::view::{Mode, WarningCode};
use crate::projection::files::testing::{plan_with, styles};
use crate::projection::formats::registry;
use crate::projection::model::*;
use crate::refs::BlockRef;
use crate::state::State;
use crate::wordml::{BlockKind, ItemKind, Paragraph as WPara, StoryKind};
use std::collections::HashMap;

/// The characters that stand for atoms in the text map, which a model shows in its own way.
fn strip(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))
        .collect()
}

/// Every paragraph of a list of blocks, tables and cells included, in order.
fn paragraphs<'m>(blocks: &'m [Block], out: &mut Vec<&'m Paragraph>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.push(p),
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        paragraphs(&cell.blocks, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn all_blocks(m: &Projection) -> Vec<&Block> {
    let mut out = Vec::new();
    for s in stories(m) {
        for u in &s.units {
            out.extend(u.blocks.iter());
        }
    }
    out
}

fn all_paragraphs(m: &Projection) -> Vec<&Paragraph> {
    let mut out = Vec::new();
    for s in stories(m) {
        for u in &s.units {
            paragraphs(&u.blocks, &mut out);
        }
    }
    out
}

/// Every ref of a block list: paragraphs, tables and rows.
fn refs_of(blocks: &[Block], out: &mut Vec<BlockRef>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.extend(p.r#ref),
            Block::Table(t) => {
                out.extend(t.r#ref);
                for row in &t.rows {
                    out.extend(row.r#ref);
                    for cell in &row.cells {
                        refs_of(&cell.blocks, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn has_rev(inlines: &[Inline]) -> bool {
    inlines.iter().any(|i| match i {
        Inline::Rev { .. } => true,
        Inline::Link { inlines, .. }
        | Inline::Control { inlines, .. }
        | Inline::Hidden { inlines, .. }
        | Inline::LooksHidden { inlines, .. } => has_rev(inlines),
        _ => false,
    })
}

/// What the build knows from the state itself: each ref's story part and entry, and the text map of each.
struct Entries {
    by_ref: HashMap<BlockRef, (crate::state::PartId, usize)>,
    total: usize,
}

fn entries_of(state: &State) -> Entries {
    let parts = state.parts();
    let mut by_ref = HashMap::new();
    let mut total = 0;
    for spec in parts.story_specs().unwrap().0.iter() {
        let id = parts.id(&spec.part).unwrap();
        let story = parts.story(id).unwrap();
        for (i, e) in story.blocks().iter().enumerate() {
            if let Some(r) = parts.ref_of(&story.key(e)) {
                by_ref.insert(r, (id, i));
                total += 1;
            }
        }
    }
    Entries { by_ref, total }
}

/// M1: the text of every paragraph of the model is the text of its map, in each mode, and the width of
/// what the model shows is the number of characters in `Final`.
fn m1(state: &State, mode: Mode, m: &Projection, e: &Entries) -> usize {
    let parts = state.parts();
    let mut checked = 0;
    for par in all_paragraphs(m) {
        let Some(r) = par.r#ref else { continue };
        let (part, i) = e.by_ref[&r];
        let story = parts.story(part).unwrap();
        let tree = parts.tree(part).unwrap();
        let wp: WPara<'_> = story.paragraph(tree, i).unwrap();
        let map = wp.map();
        if map.items().iter().any(|it| it.kind == ItemKind::FieldValue) {
            continue;
        }
        assert_eq!(
            strip(&flatten(&par.inlines)),
            strip(&map.text(mode)),
            "M1 text of {r} in {mode:?}"
        );
        if mode != Mode::Original && par.flags.readonly != Some(ReadOnly::DeletedBlock) {
            assert_eq!(
                width_of(&par.inlines) as usize,
                map.len(Mode::Final),
                "M1 width of {r} in {mode:?}"
            );
        }
        checked += 1;
    }
    checked
}

/// M1, slot by slot (review_g's law): the start of each text slot of a paragraph's draft, counted the way `to_batch`
/// counts it (the text of the slots before it, and each seam by its width), is where that text is in the `Final`
/// text of the paragraph. This is what makes an operation's offsets land on the text they mean.
fn m1_slots(state: &State, m: &Projection, e: &Entries) -> usize {
    use crate::projection::formats::Format;
    let parts = state.parts();
    let draft = m.draft(&crate::projection::formats::stet::StetFormat.policy());
    let mut checked = 0;
    for file in &draft.files {
        for block in file.blocks() {
            let Some(r) = block.r#ref else { continue };
            if block.facts.readonly == Some(ReadOnly::DeletedBlock) {
                continue;
            }
            let Some(&(part, i)) = e.by_ref.get(&r) else {
                continue;
            };
            let story = parts.story(part).unwrap();
            let tree = parts.tree(part).unwrap();
            let wp: WPara<'_> = story.paragraph(tree, i).unwrap();
            let map = wp.map();
            if map.items().iter().any(|it| it.kind == ItemKind::FieldValue) {
                continue;
            }
            let text: Vec<char> = map.text(Mode::Final).chars().collect();
            let mut start = 0usize;
            for (j, slot) in block.texts.iter().enumerate() {
                let n = slot.chars().count();
                let got: String = text
                    .get(start..start + n)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default();
                assert_eq!(&got, slot, "M1: slot {j} of {r} does not start at {start}");
                start += n + block.seams.get(j).map_or(0, |s| s.width as usize);
            }
            checked += 1;
        }
    }
    checked
}

/// M2 and M3: every block appears once, or inside a collapsed block, and the rows fit the grid.
fn m2_m3(m: &Projection, e: &Entries, state: &State) {
    let mut refs = Vec::new();
    for b in all_blocks(m) {
        refs_of(std::slice::from_ref(b), &mut refs);
    }
    let parts = state.parts();
    let mut covered = 0usize;
    for b in all_blocks(m) {
        if let Block::Opaque(OpaqueBlock::Collapsed {
            first: Some(f),
            last: Some(l),
            ..
        }) = b
        {
            let (part, a) = e.by_ref[f];
            let (_, z) = e.by_ref[l];
            let story = parts.story(part).unwrap();
            covered += (a..=z)
                .filter(|k| {
                    story
                        .blocks()
                        .get(*k)
                        .is_some_and(|x| parts.ref_of(&story.key(x)).is_some())
                })
                .count();
        }
    }
    let n = refs.len();
    refs.sort_by_key(|r| (r.kind() as u8, r.number()));
    refs.dedup();
    assert_eq!(refs.len(), n, "M2: a ref appears twice");
    assert_eq!(
        n + covered,
        e.total,
        "M2: every block appears once or in a collapsed block"
    );
    for b in all_blocks(m) {
        check_tables(b);
    }
}

fn check_tables(b: &Block) {
    if let Block::Table(t) = b {
        for row in &t.rows {
            assert!(!row.cells.is_empty());
            for c in &row.cells {
                assert!(c.col + c.span <= t.cols, "M3: a cell passes the grid");
                assert!(!c.blocks.is_empty(), "M3: a cell has a block");
                for inner in &c.blocks {
                    check_tables(inner);
                }
            }
        }
    }
}

/// M4: what each mode shows of tracked changes.
fn m4(m: &Projection, mode: Mode) {
    if mode == Mode::Redline {
        return;
    }
    for p in all_paragraphs(m) {
        assert!(!has_rev(&p.inlines), "M4: change markup in {mode:?}");
        assert!(p.mark.is_none() && p.revs.is_empty());
    }
}

/// M6 and M7: stories and notes, and the warnings.
fn m6_m7(m: &Projection) {
    let notes: Vec<u32> = stories(m)
        .iter()
        .filter(|s| s.kind == StoryKind::Footnotes)
        .flat_map(|s| s.units.iter().map(|u| u.ordinal))
        .collect();
    let mut sorted = notes.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        notes.len(),
        "M6: a note ordinal is used twice"
    );
    for p in all_paragraphs(m) {
        let Some(r) = p.r#ref else { continue };
        let want = |code: WarningCode| {
            m.head
                .warnings
                .iter()
                .any(|w| w.code == code && w.block.as_deref() == Some(&r.to_string()))
        };
        let mut looks = false;
        let mut hidden = false;
        scan(&p.inlines, &mut looks, &mut hidden);
        assert!(
            !looks || want(WarningCode::LooksHidden),
            "M7: {r} has looks-hidden text and no warning"
        );
        assert!(
            !hidden || want(WarningCode::HiddenText),
            "M7: {r} has hidden text and no warning"
        );
    }
    for b in all_blocks(m) {
        if matches!(
            b,
            Block::Opaque(OpaqueBlock::AltChunk | OpaqueBlock::Unknown | OpaqueBlock::Math(_))
        ) {
            assert!(
                m.head
                    .warnings
                    .iter()
                    .any(|w| w.code == WarningCode::OpaqueContent)
            );
        }
    }
}

/// The target of every field in a list of inlines, wrappers included.
fn field_targets(inlines: &[Inline], out: &mut Vec<BlockRef>) {
    for i in inlines {
        match i {
            Inline::Field(f) => out.extend(f.target),
            Inline::Rev { inlines, .. }
            | Inline::Link { inlines, .. }
            | Inline::Control { inlines, .. }
            | Inline::Hidden { inlines, .. }
            | Inline::LooksHidden { inlines, .. } => field_targets(inlines, out),
            _ => {}
        }
    }
}

/// M5, the part that is this module's: a field that points at a bookmark points at a paragraph that the model has,
/// by its ref. Whether the cached result of a `REF` equals the number of its target is the numbering's, and
/// `numbering` compares it with Word's own results on the same documents.
fn m5_targets(m: &Projection) {
    let mut refs: Vec<BlockRef> = Vec::new();
    for b in all_blocks(m) {
        refs_of(std::slice::from_ref(b), &mut refs);
    }
    let known: std::collections::HashSet<BlockRef> = refs.into_iter().collect();
    for p in all_paragraphs(m) {
        let mut targets = Vec::new();
        field_targets(&p.inlines, &mut targets);
        for t in targets {
            assert!(
                known.contains(&t),
                "M5: a field points at {t}, which is not a block of the model"
            );
        }
    }
}

fn scan(inlines: &[Inline], looks: &mut bool, hidden: &mut bool) {
    for i in inlines {
        match i {
            Inline::LooksHidden { inlines, .. } => {
                *looks = true;
                scan(inlines, looks, hidden);
            }
            Inline::Hidden { inlines, .. } => {
                *hidden = true;
                scan(inlines, looks, hidden);
            }
            Inline::Rev { inlines, .. }
            | Inline::Link { inlines, .. }
            | Inline::Control { inlines, .. } => {
                scan(inlines, looks, hidden);
            }
            _ => {}
        }
    }
}

/// F2 and T1 on the model of the files, for every registered format.
fn files_laws(m: &Projection) {
    for format in registry() {
        let rendered = format.render(m);
        let parsed = format
            .parse(&rendered.files, &Limits::default())
            .unwrap_or_else(|e| {
                panic!(
                    "F2: {} cannot read what it wrote: {}",
                    format.id(),
                    e.message()
                )
            });
        assert_eq!(parsed, m.draft(&format.policy()), "F2 for {}", format.id());
        let plan =
            plan_with(m, *format, &styles(), |_, text| text.to_string()).unwrap_or_else(|e| {
                panic!(
                    "T1: {} refused the unedited files: {}",
                    format.id(),
                    e.message()
                )
            });
        assert!(
            plan.ops.is_empty(),
            "T1: {} found an edit in the unedited files: {:?}",
            format.id(),
            plan.ops
        );
    }
}

/// Every law of the model and the files, on one document. Returns how many paragraphs M1 compared.
pub(super) fn check_document(state: &State) -> usize {
    let entries = entries_of(state);
    let mut compared = 0;
    for mode in Mode::ALL {
        let m = model(state, mode);
        compared += m1(state, mode, &m, &entries);
        m4(&m, mode);
        if mode == Mode::Redline {
            m1_slots(state, &m, &entries);
            m2_m3(&m, &entries, state);
            m5_targets(&m);
            m6_m7(&m);
            files_laws(&m);
        }
    }
    compared
}

/// The unedited files of a document with no revision read the same in every mode.
pub(super) fn check_modes_agree(state: &State) {
    let (a, b, c) = (
        model(state, Mode::Final),
        model(state, Mode::Original),
        model(state, Mode::Redline),
    );
    let strip_mode = |mut m: Projection| {
        m.head.mode = Mode::Redline;
        m
    };
    assert_eq!(strip_mode(a.clone()), strip_mode(b));
    assert_eq!(strip_mode(a), strip_mode(c));
}

#[test]
fn the_laws_hold_on_a_document_with_a_little_of_everything() {
    let sdt = "<w:sdt><w:sdtPr><w:alias w:val=\"Date\"/><w:date/></w:sdtPr><w:sdtContent><w:r><w:t>2026-10-01</w:t></w:r></w:sdtContent></w:sdt>";
    let field = "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> REF _Ref1 </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>30</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";
    let body = format!(
        "{}{}{}{}{}{}",
        px(&format!(
            "{}{}{}{}",
            r("Pay "),
            del(1, "Jane Doe", "ten"),
            ins(2, "Eric White", "twelve"),
            r(" days.")
        )),
        px(&format!("{}{}{}{}", r("within "), field, r(" of "), sdt)),
        px(
            "<w:r><w:rPr><w:vanish/></w:rPr><w:t>hidden</w:t></w:r><w:r><w:t> tail \t tab</w:t></w:r><w:r><w:footnoteReference w:id=\"1\"/></w:r>"
        ),
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1\"/><w:gridCol w:w=\"1\"/></w:tblGrid><w:tr><w:tc><w:tcPr/><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc><w:tc><w:tcPr/><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
        px(&format!(
            "<w:commentRangeStart w:id=\"3\"/>{}<w:commentRangeEnd w:id=\"3\"/>",
            r("commented")
        )),
        p("end")
    );
    let f = Fixture::new(&body)
        .footnotes("<w:footnote w:id=\"1\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t> note text</w:t></w:r></w:p></w:footnote>")
        .comments("<w:comment w:id=\"3\" w:author=\"Eric White\" w:initials=\"EW\"><w:p><w:r><w:t>why?</w:t></w:r></w:p></w:comment>");
    let state = f.state();
    let compared = check_document(&state);
    assert!(compared >= 8, "{compared}");
}

#[test]
fn a_document_without_revisions_reads_the_same_in_every_mode() {
    let f = Fixture::new(&format!("{}{}", p("one"), p("two")));
    let state = f.state();
    check_modes_agree(&state);
    check_document(&state);
    let _ = BlockKind::Paragraph;
}
