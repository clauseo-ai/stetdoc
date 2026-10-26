//! The generator and the string model of the laws (design `revisions.md`, 7.4). It draws compiler calls
//! on the current document, and keeps a model of what each paragraph's `Final` text must be, as a map
//! from the paragraph's key to its characters, which knows no XML. After every call the model is checked
//! against the document (law R3).

use super::compiler::Compiler;
use super::tests_diff::{MISC, Rng, WORDS, mutate};
use super::tests_support::*;
use super::types::{MarkerSpec, NewParagraph, Place, PropTarget};
use crate::api::error::{ErrorCode, StetError};
use crate::state::Tx;
use crate::wordml::BlockKey;
use crate::wordml::{BlockKind, Mode};
use std::collections::HashMap;

/// What a paragraph's `Final` text must be, by paragraph.
pub(super) type Model = HashMap<BlockKey, Vec<char>>;

/// A failed call: expected refusals are `Refused`, everything else is a bug.
pub(super) enum Failure {
    Refused(ErrorCode),
    Bug(String),
}

impl From<StetError> for Failure {
    fn from(e: StetError) -> Failure {
        match e.code() {
            ErrorCode::ReadonlyRegion | ErrorCode::CrossBlock | ErrorCode::Unsupported => {
                Failure::Refused(e.code())
            }
            _ => Failure::Bug(format!("{e}")),
        }
    }
}

/// One paragraph of the current document.
pub(super) struct Para {
    pub key: BlockKey,
    pub text: Vec<char>,
    pub node: crate::wordml::NodeId,
}

/// The blocks of the current document.
pub(super) struct Layout {
    pub paras: Vec<Para>,
    pub tables: Vec<BlockKey>,
    pub rows: Vec<BlockKey>,
}

/// Reads the blocks of the body in document order.
pub(super) fn layout(tx: &Tx<'_>) -> Layout {
    let view = tx.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    let mut lay = Layout {
        paras: Vec::new(),
        tables: Vec::new(),
        rows: Vec::new(),
    };
    for (i, e) in story.blocks().iter().enumerate() {
        let key = story.key(e);
        match e.kind {
            BlockKind::Paragraph => {
                let text: Vec<char> = story
                    .paragraph(tree, i)
                    .unwrap()
                    .text(Mode::Final)
                    .chars()
                    .collect();
                lay.paras.push(Para {
                    key,
                    text,
                    node: e.node,
                });
            }
            BlockKind::Table => lay.tables.push(key),
            BlockKind::Row => lay.rows.push(key),
        }
    }
    lay
}

/// The model of the document as it is.
pub(super) fn model_of(tx: &Tx<'_>) -> Model {
    layout(tx)
        .paras
        .into_iter()
        .map(|p| (p.key, p.text))
        .collect()
}

/// Checks the model against the document. A paragraph that vanished must have been empty.
pub(super) fn reconcile(tx: &Tx<'_>, model: &mut Model) -> Result<(), String> {
    let lay = layout(tx);
    for p in &lay.paras {
        match model.get(&p.key) {
            Some(want) if *want == p.text => {}
            Some(want) => {
                return Err(format!(
                    "the Final text of a paragraph is {:?} and the model says {:?}",
                    p.text.iter().collect::<String>(),
                    want.iter().collect::<String>()
                ));
            }
            None => {
                return Err(format!(
                    "a paragraph the model does not know appeared, with text {:?}",
                    p.text.iter().collect::<String>()
                ));
            }
        }
    }
    let live: std::collections::HashSet<&BlockKey> = lay.paras.iter().map(|p| &p.key).collect();
    let vanished: Vec<BlockKey> = model
        .keys()
        .filter(|k| !live.contains(k))
        .cloned()
        .collect();
    for k in vanished {
        if model.get(&k).is_some_and(|t| !t.is_empty()) {
            return Err("a paragraph with text vanished".to_owned());
        }
        model.remove(&k);
    }
    Ok(())
}

fn pick<'a, T>(rng: &mut Rng, xs: &'a [T]) -> Option<&'a T> {
    if xs.is_empty() {
        None
    } else {
        xs.get(rng.below(xs.len()))
    }
}

fn word_text(rng: &mut Rng) -> String {
    match rng.below(5) {
        0 => String::new(),
        1 | 2 => WORDS[rng.below(WORDS.len())].to_owned(),
        3 => format!(
            "{} {}",
            WORDS[rng.below(WORDS.len())],
            WORDS[rng.below(WORDS.len())]
        ),
        _ => MISC[rng.below(MISC.len())].to_owned(),
    }
}

/// A range of a text, on word starts half of the time.
fn range(rng: &mut Rng, chars: &[char]) -> std::ops::Range<usize> {
    let len = chars.len();
    let a = rng.below(len + 1);
    let b = rng.below(len + 1);
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    if rng.below(4) == 0 { a..a } else { a..b }
}

fn has_atom(s: &str) -> bool {
    s.chars()
        .any(|c| matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))
}

/// Draws one compiler call on the current document and makes it, updating the model.
pub(super) fn random_call(
    rng: &mut Rng,
    c: &mut Compiler<'_, '_>,
    model: &mut Model,
    log: &mut Vec<String>,
) -> Result<(), Failure> {
    let lay = layout(c.tx());
    let Some(p) = pick(rng, &lay.paras) else {
        // A document of nothing but blocks that are not paragraphs (an `altChunk`) has nothing to edit.
        return Err(Failure::Refused(ErrorCode::Unsupported));
    };
    let kind = rng.below(100);
    match kind {
        0..=34 => {
            let rg = range(rng, &p.text);
            let slice: String = p.text[rg.clone()].iter().collect();
            let with = match rng.below(4) {
                0 => String::new(),
                1 => word_text(rng),
                _ => mutate(rng, &slice),
            };
            if has_atom(&with) {
                return Err(Failure::Refused(ErrorCode::InvalidPayload));
            }
            log.push(format!("replace {:?} {rg:?} {with:?}", p.key.node()));
            c.replace_text(&p.key, rg.clone(), &with)?;
            let mut m = p.text[..rg.start].to_vec();
            m.extend(with.chars());
            m.extend_from_slice(&p.text[rg.end..]);
            model.insert(p.key.clone(), m);
        }
        35..=44 => {
            let rg = range(rng, &p.text);
            let name = ["b", "i", "u", "strike"][rng.below(4)];
            let edit = if rng.below(3) == 0 {
                clear_edit(&[name])
            } else if name == "u" {
                set_edit(vec![pel("u", &[("val", "single")], vec![])])
            } else {
                set_edit(vec![pel(name, &[], vec![])])
            };
            log.push(format!("format {:?} {rg:?} {name}", p.key.node()));
            c.set_props(
                &PropTarget::Run {
                    block: p.key.clone(),
                    range: rg,
                },
                &edit,
            )?;
        }
        45..=49 => {
            let target = if rng.below(2) == 0 {
                PropTarget::Paragraph {
                    block: p.key.clone(),
                }
            } else {
                PropTarget::ParagraphMark {
                    block: p.key.clone(),
                }
            };
            let edit = match (&target, rng.below(3)) {
                (PropTarget::Paragraph { .. }, 0) => {
                    set_edit(vec![pel("jc", &[("val", "center")], vec![])])
                }
                (PropTarget::Paragraph { .. }, 1) => {
                    set_edit(vec![pel("pStyle", &[("val", "Heading1")], vec![])])
                }
                (PropTarget::Paragraph { .. }, _) => clear_edit(&["jc", "pStyle", "keepNext"]),
                (_, 0) => bold(),
                (_, _) => clear_edit(&["b", "i"]),
            };
            log.push(format!("paragraph props {:?} {target:?}", p.key.node()));
            c.set_props(&target, &edit)?;
        }
        50..=61 => {
            let anchors: Vec<&BlockKey> = lay
                .paras
                .iter()
                .map(|p| &p.key)
                .chain(lay.tables.iter())
                .collect();
            let anchor = anchors[rng.below(anchors.len())].clone();
            let place = if rng.below(2) == 0 {
                Place::Before
            } else {
                Place::After
            };
            let text = word_text(rng);
            let style =
                (rng.below(3) == 0).then(|| ["Body", "Heading1", "List"][rng.below(3)].to_owned());
            let spec = NewParagraph {
                text: text.clone(),
                style,
                ..NewParagraph::default()
            };
            log.push(format!("insert {:?} {place:?} {spec:?}", anchor.node()));
            let fx = c.insert_block(&anchor, place, &spec)?;
            if let Some(k) = fx.new_blocks.first() {
                model.insert(k.clone(), text.chars().collect());
            }
        }
        62..=69 => {
            log.push(format!("delete {:?}", p.key.node()));
            let fx = c.delete_block(&p.key)?;
            model.insert(p.key.clone(), Vec::new());
            // An own inserted mark goes by a join into the next paragraph: the paragraph keeps its ref and
            // takes the next one's text, and the next one is gone.
            if fx.survivor.as_ref() == Some(&p.key)
                && let Some(second) = fx
                    .origins
                    .iter()
                    .find(|o| o.block == p.key)
                    .and_then(|o| o.from.get(1))
                && let Some(text) = model.remove(second)
            {
                model.insert(p.key.clone(), text);
            }
        }
        70..=77 => {
            let at = rng.below(p.text.len() + 1);
            log.push(format!("split {:?} {at}", p.key.node()));
            let fx = c.split_block(&p.key, at)?;
            // A cut before the text that leaves the paragraph untouched is an empty X before it, which keeps the text.
            if let Some(k) = fx.new_blocks.first()
                && !fx.touched.contains(&p.key)
            {
                model.insert(k.clone(), Vec::new());
            } else if let Some(k) = fx.new_blocks.first() {
                model.insert(p.key.clone(), p.text[..at].to_vec());
                model.insert(k.clone(), p.text[at..].to_vec());
            }
        }
        78..=85 => {
            // The next paragraph in document order: neighbours or not, the compiler decides.
            let i = lay.paras.iter().position(|x| x.key == p.key).unwrap_or(0);
            let Some(q) = lay.paras.get(i + 1) else {
                return Err(Failure::Refused(ErrorCode::CrossBlock));
            };
            log.push(format!("merge {:?} {:?}", p.key.node(), q.key.node()));
            let fx = c.merge_blocks(&p.key, &q.key)?;
            if fx.survivor.is_some() {
                let mut m = p.text.clone();
                m.extend_from_slice(&q.text);
                model.insert(p.key.clone(), m);
                model.remove(&q.key);
            }
        }
        86..=89 => {
            let Some(row) = pick(rng, &lay.rows) else {
                return Err(Failure::Refused(ErrorCode::CrossBlock));
            };
            let place = if rng.below(2) == 0 {
                Place::Before
            } else {
                Place::After
            };
            let cells: Vec<String> = (0..rng.below(3)).map(|_| word_text(rng)).collect();
            log.push(format!("insert row {:?} {place:?} {cells:?}", row.node()));
            // The generator draws up to two texts, and a row of one cell refuses them.
            let fx = c
                .insert_row(row, place, &cells)
                .map_err(|e| match e.code() {
                    ErrorCode::InvalidPayload => Failure::Refused(ErrorCode::InvalidPayload),
                    _ => Failure::from(e),
                })?;
            let after = layout(c.tx());
            let mut texts = cells.iter();
            for k in fx.new_blocks.iter().skip(1) {
                if after.paras.iter().any(|x| &x.key == k) {
                    model.insert(
                        k.clone(),
                        texts.next().map_or_else(Vec::new, |t| t.chars().collect()),
                    );
                }
            }
        }
        90..=92 => {
            let target = if rng.below(2) == 0 {
                pick(rng, &lay.rows)
            } else {
                pick(rng, &lay.tables)
            };
            let Some(target) = target else {
                return Err(Failure::Refused(ErrorCode::CrossBlock));
            };
            log.push(format!("delete block {:?}", target.node()));
            let inside: Vec<BlockKey> = {
                let view = c.tx().parts();
                let tree = view.tree(view.id(MAIN).unwrap()).unwrap();
                lay.paras
                    .iter()
                    .filter(|q| super::names::contains(tree, target.node(), q.node))
                    .map(|q| q.key.clone())
                    .collect()
            };
            c.delete_block(target)?;
            for k in inside {
                model.insert(k, Vec::new());
            }
        }
        93..=95 => {
            let target = match rng.below(3) {
                0 => pick(rng, &lay.tables).map(|t| {
                    (
                        PropTarget::Table { table: t.clone() },
                        set_edit(vec![pel("jc", &[("val", "center")], vec![])]),
                    )
                }),
                1 => pick(rng, &lay.rows).map(|r| {
                    (
                        PropTarget::Row { row: r.clone() },
                        set_edit(vec![pel("cantSplit", &[], vec![])]),
                    )
                }),
                _ => pick(rng, &lay.rows).map(|r| {
                    (
                        PropTarget::Cell {
                            row: r.clone(),
                            cell: 0,
                        },
                        set_edit(vec![pel("noWrap", &[], vec![])]),
                    )
                }),
            };
            let Some((t, e)) = target else {
                return Err(Failure::Refused(ErrorCode::CrossBlock));
            };
            log.push(format!("table props {t:?}"));
            c.set_props(&t, &e)?;
        }
        _ => {
            let rg = range(rng, &p.text);
            let id = c.tx.next_annotation_id().map_err(Failure::from)?;
            let id = id.to_string();
            let spec = MarkerSpec {
                start: pel(
                    "bookmarkStart",
                    &[("id", &id), ("name", &format!("lawbm{id}"))],
                    vec![],
                ),
                end: pel("bookmarkEnd", &[("id", &id)], vec![]),
                reference: None,
            };
            log.push(format!("markers {:?} {rg:?}", p.key.node()));
            c.place_markers(&p.key, rg, &spec)?;
        }
    }
    Ok(())
}

/// The bodies that the laws run on: the fixtures of the text tests and a few with structure.
pub(super) fn law_docs() -> Vec<String> {
    let mut docs = super::tests_text_rand::fixtures();
    let cell = |t: &str| {
        format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc>",
            p(&r(t))
        )
    };
    let table = |rows: &str| {
        format!(
            "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>{rows}</w:tbl>"
        )
    };
    docs.push(format!(
        "{}{}{}{}",
        p(&r("Intro paragraph")),
        table(&format!(
            "<w:tr>{}{}</w:tr><w:tr>{}{}</w:tr>",
            cell("a1"),
            cell("b1"),
            cell("a2"),
            cell("b2")
        )),
        p(&r("Middle")),
        p(&r("Outro"))
    ));
    docs.push(format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>{}</w:p>\
         <w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p>\
         <w:p><w:ins {}>{}</w:ins></w:p>\
         <w:p><w:pPr><w:jc w:val=\"center\"/><w:pPrChange {}><w:pPr/></w:pPrChange></w:pPr>{}</w:p>\
         <w:p>{}</w:p>",
        r("Heading"),
        eric(701),
        r("split first half"),
        eric(702),
        r("second half"),
        eric(703),
        r("centred"),
        r("end")
    ));
    docs.push(format!(
        "{}<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr>{}{}</w:tr><w:tr><w:trPr><w:ins {}/></w:trPr><w:tc><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:ins {}>{}</w:ins></w:p></w:tc><w:tc><w:p/></w:tc></w:tr></w:tbl>",
        p(&r("before")),
        cell("x"),
        cell("y"),
        eric(710),
        eric(711),
        eric(712),
        r("theirs")
    ));
    docs.push(format!(
        "<w:p>{}</w:p><w:p><w:pPr><w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:pPr>{}</w:p><w:p>{}</w:p>",
        r("section one"),
        r("end of section"),
        r("section two")
    ));
    // A move over two paragraphs, an inserted field with its separator outside, a deleted field, a
    // deleted paragraph mark, and a pending deletion by another author in the middle of an insertion.
    docs.push(format!(
        "<w:p><w:moveFromRangeStart w:id=\"20\" w:name=\"mv\"/><w:moveFrom {}>{}</w:moveFrom></w:p>\
         <w:p><w:moveFrom {}>{}</w:moveFrom><w:moveFromRangeEnd w:id=\"20\"/></w:p>\
         <w:p>{}<w:moveToRangeStart w:id=\"21\" w:name=\"mv\"/><w:moveTo {}>{}</w:moveTo><w:moveToRangeEnd w:id=\"21\"/></w:p>",
        eric(720),
        r("moved one "),
        eric(721),
        r("moved two"),
        r("kept text "),
        eric(722),
        r("moved one moved two")
    ));
    docs.push(format!(
        "<w:p>{}<w:ins {}><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r></w:ins><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:ins {}>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:ins>{}</w:p>\
         <w:p>{}<w:del {}><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:delInstrText> DATE </w:delInstrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:delText>today</w:delText></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:del>{}</w:p>\
         <w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>\
         <w:p><w:ins {}>{}<w:del {}><w:r><w:delText>nested</w:delText></w:r></w:del>{}</w:ins></w:p>",
        r("see page "),
        eric(730),
        eric(731),
        r("3"),
        r(" here"),
        r("date: "),
        eric(732),
        r(" end"),
        eric(733),
        r("tail of a joined paragraph"),
        r("joined into"),
        eric(734),
        r("their words "),
        rev_by(735, "Zed"),
        r(" more")
    ));
    // Text as a converter from PDF writes it: no `xml:space`, with white space at the ends of the text
    // and a run of nothing but a space between words.
    let bare = |t: &str| format!("<w:r><w:t>{t}</w:t></w:r>");
    docs.push(format!(
        "<w:p>{}{}{}{}{}</w:p><w:p>{}{}{}</w:p>",
        bare("The prohibitions "),
        bare(" "),
        bare("of"),
        bare(" "),
        bare("certain practices; "),
        bare("Second "),
        bare(" "),
        bare(" paragraph")
    ));
    docs
}

/// The attributes of a revision by another author.
fn rev_by(id: u32, author: &str) -> String {
    format!("w:id=\"{id}\" w:author=\"{author}\" w:date=\"2020-02-03T04:05:06Z\"")
}
