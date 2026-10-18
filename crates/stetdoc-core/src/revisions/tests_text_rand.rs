//! `replace_text` over random edits of documents with structure: the string model says what `Final` must
//! read, `Original` must not move, every revision written passes the Word rules (`finish` checks them),
//! the schema has nothing new to say, and the transaction commits. Also the one-map claim of design 2.2.

use super::compiler::Compiler;
use super::errors::internal;
use super::stamp::Track;
use super::tests_diff::{MISC, Rng, WORDS, mutate};
use super::tests_support::*;
use crate::api::error::ErrorCode;
use crate::wordml::Mode;

const ERIC_NA: &str = "w:author=\"Eric\" w:date=\"2020-01-02T03:04:05Z\"";

fn p(inner: &str) -> String {
    format!("<w:p>{inner}</w:p>")
}

fn r(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn rb(text: &str) -> String {
    format!("<w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn field(code: &str, result: &str) -> String {
    format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        r(result)
    )
}

/// The bodies that the random edits run on.
pub(super) fn fixtures() -> Vec<String> {
    let me = AT;
    vec![
        format!(
            "{}{}",
            p(&r("The Party shall pay within 30 days.")),
            p(&r("Second paragraph, with a comma."))
        ),
        p(&format!(
            "{}{}{}{}",
            rb("Bold lead. "),
            r("plain words here "),
            "<w:r><w:rPr><w:i/></w:rPr><w:t>italic tail</w:t></w:r>",
            r(" and more")
        )),
        p(
            "<w:r><w:t>a</w:t><w:tab/><w:t>b c</w:t><w:br/><w:t>d</w:t><w:noBreakHyphen/><w:t>e</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>tail</w:t></w:r>",
        ),
        p(&format!(
            "{}<w:hyperlink w:anchor=\"x\">{}</w:hyperlink>{}<w:smartTag w:uri=\"u\" w:element=\"e\">{}</w:smartTag>{}",
            r("before "),
            r("link text"),
            r(" middle "),
            r("tagged"),
            r(" after")
        )),
        p(&format!(
            "{}<w:ins {}>{}</w:ins>{}<w:del {}><w:r><w:delText>gone words</w:delText></w:r></w:del>{}",
            r("keep "),
            eric(900),
            r("Eric inserted"),
            r(" and "),
            eric(901),
            r(" end")
        )),
        p(&format!(
            "{}<w:ins w:id=\"910\" {me}>{}</w:ins>{}<w:ins {}>{}<w:del w:id=\"911\" {me}><w:r><w:delText>nested</w:delText></w:r></w:del>{}</w:ins>",
            r("start "),
            r("own insertion"),
            r(" mid "),
            eric(912),
            r("their text "),
            r("more")
        )),
        p(&format!(
            "{}{}{}",
            r("pay "),
            field("PAGE", "42"),
            r(" now")
        )),
        p(&format!(
            "{}<w:sdt><w:sdtPr><w:alias w:val=\"c\"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>{}",
            r("lead "),
            r("control text"),
            r(" trail")
        )),
        p(&format!(
            "<w:bookmarkStart w:id=\"1\" w:name=\"b\"/>{}<w:bookmarkEnd w:id=\"1\"/><w:commentRangeStart w:id=\"2\"/>{}<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>{}<w:commentRangeEnd w:id=\"2\"/><w:r><w:commentReference w:id=\"2\"/></w:r>",
            r("marked "),
            r("range "),
            r(" end")
        )),
        p(&(0..12)
            .map(|i| {
                if i % 2 == 0 {
                    rb(&format!("w{i} "))
                } else {
                    r(&format!("v{i} "))
                }
            })
            .collect::<String>()),
        p(&format!(
            "<w:moveFromRangeStart w:id=\"40\" w:name=\"m\"/><w:moveFrom w:id=\"920\" {}><w:r><w:t>moved away</w:t></w:r></w:moveFrom><w:moveFromRangeEnd w:id=\"40\"/>{}<w:moveToRangeStart w:id=\"41\" w:name=\"m\"/><w:moveTo w:id=\"921\" {}>{}</w:moveTo><w:moveToRangeEnd w:id=\"41\"/>",
            ERIC_NA,
            r("stay "),
            ERIC_NA,
            r("moved here")
        )),
        format!("{}{}{}", p(""), p("<w:r><w:tab/></w:r>"), p(&r("last"))),
        format!(
            "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc></w:tr></w:tbl>{}",
            p(&r("cell one")),
            p(&r("cell two")),
            p(&r("after table"))
        ),
        format!(
            "<w:p><w:pPr><w:rPr><w:rFonts w:ascii=\"Arial\"/></w:rPr></w:pPr><w:r><w:rPr><w:b/><w:rPrChange {}><w:rPr/></w:rPrChange></w:rPr><w:t>changed format</w:t></w:r>{}</w:p>",
            eric(930),
            r(" tail")
        ),
    ]
}

fn pick<'a>(rng: &mut Rng, xs: &[&'a str]) -> &'a str {
    xs.get(rng.below(xs.len())).copied().unwrap_or("")
}

/// A replacement for `old`: nothing, a word, odd characters, a mutation of it, or a word added to it.
fn replacement(rng: &mut Rng, old: &str) -> String {
    match rng.below(7) {
        0 => String::new(),
        1 => pick(rng, &WORDS).to_owned(),
        2 => format!("{} ", pick(rng, &WORDS)),
        3 => pick(rng, &MISC).to_owned(),
        4 | 5 => mutate(rng, old),
        _ => format!("{old} {}", pick(rng, &WORDS)),
    }
}

/// A range in a text of `len` characters, on word starts half of the time.
fn range(rng: &mut Rng, chars: &[char]) -> std::ops::Range<usize> {
    let len = chars.len();
    let any = |rng: &mut Rng| rng.below(len + 1);
    let word_start = |rng: &mut Rng| {
        let starts: Vec<usize> = (0..=len)
            .filter(|&i| i == 0 || i == len || chars.get(i - 1) == Some(&' '))
            .collect();
        starts.get(rng.below(starts.len())).copied().unwrap_or(0)
    };
    let (a, b) = if rng.below(2) == 0 {
        (any(rng), any(rng))
    } else {
        (word_start(rng), word_start(rng))
    };
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    if rng.below(4) == 0 { a..a } else { a..b }
}

/// What one random batch did, for the failure message.
struct Batch {
    log: Vec<String>,
}

fn allowed(code: ErrorCode) -> bool {
    matches!(code, ErrorCode::ReadonlyRegion | ErrorCode::Unsupported)
}

/// Runs one random batch on `body` and checks the laws that concern text. Returns the number of edits
/// that were refused.
fn one_batch(body: &str, seed: u64, by: Option<&str>) -> usize {
    let s = state(body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let before_orig = texts(&tx, Mode::Original);
    let mut model: Vec<Vec<char>> = texts(&tx, Mode::Final)
        .iter()
        .map(|t| t.chars().collect())
        .collect();
    let mut rng = Rng(seed | 1);
    let mut batch = Batch { log: Vec::new() };
    let mut refused = 0;
    let track = match by {
        Some(a) => Track::Tracked(super::tests_support::stamp_of(a)),
        None => Track::Tracked(stamp()),
    };
    let mut ok_run = true;
    let outcome = {
        let mut c = Compiler::new(&mut tx, track).unwrap();
        for _ in 0..1 + rng.below(4) {
            let i = rng.below(ks.len());
            let old_text: Vec<char> = model[i].clone();
            let rg = range(&mut rng, &old_text);
            let slice: String = old_text[rg.clone()].iter().collect();
            let with = replacement(&mut rng, &slice);
            batch.log.push(format!("p{i} {rg:?} {with:?}"));
            match c.replace_text(&ks[i], rg.clone(), &with) {
                Ok(_) => {
                    let mut m = old_text[..rg.start].to_vec();
                    m.extend(with.chars());
                    m.extend_from_slice(&old_text[rg.end..]);
                    model[i] = m;
                }
                Err(e)
                    if allowed(e.code())
                        || (e.code() == ErrorCode::InvalidPayload
                            && with
                                .chars()
                                .any(|c| matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))) =>
                {
                    refused += 1;
                    ok_run = false;
                    break;
                }
                Err(e) => panic!("seed {seed}: {e}\n{:?}\n{}", batch.log, body),
            }
        }
        c.finish()
    };
    if !ok_run {
        return refused;
    }
    let _out = outcome.unwrap_or_else(|e| {
        panic!(
            "seed {seed}: finish failed: {e}\n{:?}\n{}",
            batch.log,
            body_xml(&tx)
        )
    });
    let fin: Vec<Vec<char>> = texts(&tx, Mode::Final)
        .iter()
        .map(|t| t.chars().collect())
        .collect();
    assert_eq!(
        fin,
        model,
        "seed {seed}: Final differs from the model\n{:?}\n{}",
        batch.log,
        body_xml(&tx)
    );
    assert_eq!(
        texts(&tx, Mode::Original),
        before_orig,
        "seed {seed}: Original moved\n{:?}\n{}",
        batch.log,
        body_xml(&tx)
    );
    let added = schema_added(&s, &tx);
    assert!(
        added.is_empty(),
        "seed {seed}: new schema violations {added:?}\n{:?}\n{}",
        batch.log,
        body_xml(&tx)
    );
    let xml = body_xml(&tx);
    let (after, _) = tx
        .commit()
        .unwrap_or_else(|e| panic!("seed {seed}: commit failed: {e}\n{:?}\n{xml}", batch.log));
    assert_eq!(body_xml_of(&after), xml, "seed {seed}");
    let committed: Vec<Vec<char>> = state_texts(&after, Mode::Final)
        .iter()
        .map(|t| t.chars().collect())
        .collect();
    assert_eq!(committed, model, "seed {seed}");
    refused
}

#[test]
fn random_tracked_edits_agree_with_the_string_model_on_every_fixture() {
    let mut total = 0;
    let mut refused = 0;
    for (n, body) in fixtures().iter().enumerate() {
        for k in 0..120u64 {
            refused += one_batch(
                body,
                0x9E37_79B9_7F4A_7C15u64.wrapping_mul(1 + n as u64 * 1000 + k),
                None,
            );
            total += 1;
        }
    }
    eprintln!("batches {total}, refused {refused}");
    assert!(
        refused * 4 < total,
        "too many refusals: {refused} of {total}"
    );
}

#[test]
fn a_second_author_editing_the_first_authors_output_keeps_original_and_the_model() {
    for (n, body) in fixtures().iter().enumerate() {
        for k in 0..40u64 {
            one_batch(
                body,
                0xA5A5_1234_0000_0001u64.wrapping_mul(1 + n as u64 * 100 + k),
                Some("Zed"),
            );
        }
    }
}

/// `replace_text` with the map rebuilt before each piece, kept in the tests only.
fn replace_rebuilding(
    c: &mut Compiler<'_, '_>,
    block: &crate::wordml::BlockKey,
    range: std::ops::Range<usize>,
    with: &str,
) -> Result<(), crate::api::error::StetError> {
    c.begin();
    let (_, pieces) = c.plan_text(block, range, with, true)?;
    for piece in pieces.iter().rev() {
        let p = c.paragraph(block)?;
        let starts = super::text::final_starts(&p.map);
        c.apply_piece(&p, &starts, piece)?;
    }
    c.touch(block);
    let _ = c.end();
    Ok(())
}

#[test]
fn one_map_per_call_gives_the_bytes_of_rebuilding_the_map_for_each_piece() {
    let mut compared = 0;
    for (n, body) in fixtures().iter().enumerate() {
        for k in 0..60u64 {
            let seed = 0x1357_9BDF_0246_8ACEu64.wrapping_mul(1 + n as u64 * 100 + k) | 1;
            let run = |rebuild: bool| -> Option<String> {
                let s = state(body);
                let mut tx = s.begin();
                let ks = para_keys(&tx);
                let mut rng = Rng(seed);
                let i = rng.below(ks.len());
                let old: Vec<char> = text(&tx, i, Mode::Final).chars().collect();
                let rg = range(&mut rng, &old);
                let slice: String = old[rg.clone()].iter().collect();
                // A replacement that is several edits away from the old text.
                let once = mutate(&mut rng, &slice);
                let with = mutate(&mut rng, &once);
                let mut c = tracked(&mut tx);
                let res = if rebuild {
                    replace_rebuilding(&mut c, &ks[i], rg, &with).map(|_| ())
                } else {
                    c.replace_text(&ks[i], rg, &with).map(|_| ())
                };
                match res {
                    Ok(()) => {
                        let _ = c.finish().map_err(|e| internal(e.to_string()));
                        Some(body_xml(&tx))
                    }
                    Err(_) => None,
                }
            };
            let (one, many) = (run(false), run(true));
            assert_eq!(one, many, "fixture {n} seed {seed}");
            compared += usize::from(one.is_some());
        }
    }
    assert!(compared > 300, "{compared}");
}
