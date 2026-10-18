//! Hostile input (`docs/design/wordml.md` 9 and 10.2): every shape must be read and never panic.
//!
//! This file holds the sweeps: small XML parts that parse, cut at every byte and with every byte
//! flipped (the sweeps `xml/tests.rs` and `package/hostile_tests.rs` run for their layers), each one
//! scanned as every kind of story and then put through every operation `wordml` has. The shapes
//! built to be slow are in `tests_hostile_slow.rs`.

use super::tests_gen::Gen;
use super::tests_gen_data::Rng;
use super::tests_gen_laws::check_laws;
use super::tests_text::{FINAL, check_invariants};
use super::*;
use crate::api::limits::Limits;
use crate::xml::{Bytes, SpanTree};
use std::sync::Arc;
use std::time::Duration;

/// Runs `work` on another thread and fails the test if it takes longer than `secs` seconds. The core
/// reads no clock, and a thread with a timeout needs only a `Duration`, as in `xml/tests_ns.rs`. A
/// panic in the work is the test's panic.
pub(super) fn within<T: Send + 'static>(secs: u64, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(v) => v,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("no answer in {secs} seconds"),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => match worker.join() {
            Err(panic) => std::panic::resume_unwind(panic),
            Ok(()) => panic!("the work ended without an answer"),
        },
    }
}

const NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" \
xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"";

/// A part with a root element and content.
pub(super) fn wrap(root: &str, content: &str) -> String {
    format!("<w:{root} {NS}>{content}</w:{root}>")
}

/// Parses with the default limits, or with a deeper tree allowed.
pub(super) fn parse_with(depth: u32, xml: &str) -> Result<SpanTree, crate::api::error::StetError> {
    let limits = Limits {
        max_xml_depth: depth,
        ..Limits::default()
    };
    SpanTree::parse(
        "word/document.xml",
        Bytes::from(xml.as_bytes().to_vec()),
        &limits,
    )
}

const KINDS: [StoryKind; 6] = [
    StoryKind::Body,
    StoryKind::Header,
    StoryKind::Footer,
    StoryKind::Footnotes,
    StoryKind::Endnotes,
    StoryKind::Comments,
];

/// Reads a tree as every kind of story, and through every operation. Only the body may fail to scan,
/// and a re-scan, a refresh with nothing touched and a second reading must agree. Returns how many
/// paragraphs were read, so a sweep can say it read something.
pub(super) fn exercise(tree: &SpanTree) -> usize {
    let name: Arc<str> = Arc::from("word/document.xml");
    let mut paragraphs = 0;
    let mut rng = Rng::new(7);
    for kind in KINDS {
        let Ok(story) = Story::scan(&name, kind, tree) else {
            assert_eq!(kind, StoryKind::Body, "only the body may fail to scan");
            continue;
        };
        let again = Story::scan(&name, kind, tree).unwrap();
        assert!(story.same_as(&again));
        let prints = story.fingerprints(tree);
        assert_eq!(prints.len(), story.blocks().len());
        assert_eq!(prints, again.fingerprints(tree));
        let _ = (story.opaque(), story.units(), story.anomalies());
        for i in 0..story.blocks().len() {
            let _ = story.own_parts(tree, i);
            for mode in Mode::ALL {
                let run = story.join_run(tree, i, mode);
                assert!(run.is_empty() || run.start == i);
                if let Some((_, joined)) = story.joined(tree, i, mode) {
                    let _ = (joined.text(), joined.locate(0..joined.len()), joined.gap(0));
                }
            }
            if let Some(p) = story.paragraph(tree, i) {
                let m = p.map();
                if kind == StoryKind::Body {
                    check_laws(tree, p.id(), &m, &mut rng);
                } else {
                    check_invariants(&m);
                }
                let _ = (m.find_all(FINAL, "a"), m.anomalies().len(), p.mark_revs());
                paragraphs += 1;
            }
        }
        let _ = (story.anchors(tree), story.revisions(tree));
        let _ = (story.fields(tree), story.field_anomalies(tree));
        let r = Story::refresh(&story, tree, &[]).unwrap();
        assert!(r.story.same_as(&story));
        assert_eq!(r.story.field_index(tree), story.field_index(tree));
    }
    let styles = StyleDefs::read(tree);
    for s in styles.styles() {
        let _ = (styles.chain(&s.id).count(), styles.by_name(&s.name));
    }
    let _ = styles.issues();
    let numbering = NumberingDefs::read(tree);
    for n in numbering.nums() {
        let _ = (numbering.level(n.id, 0), numbering.abstract_of(n.id));
    }
    let _ = numbering.issues();
    let mut ids = Vec::new();
    for kind in [
        IdKind::ParaId,
        IdKind::TextId,
        IdKind::CommentDurableId,
        IdKind::AnnotationId,
    ] {
        existing_ids(kind, tree, &mut ids);
    }
    let _ = stories("word/document.xml", tree, &Rels::default());
    paragraphs
}

/// Every prefix of a part, and every byte flipped three ways. Those that still parse are exercised.
/// Returns how many parsed and how many paragraphs the exercise read.
fn sweep(seed: &str) -> (usize, usize) {
    let bytes = seed.as_bytes();
    let (mut parsed, mut read) = (0, 0);
    let mut try_one = |b: Vec<u8>| {
        let Ok(tree) = SpanTree::parse("word/document.xml", Bytes::from(b), &Limits::default())
        else {
            return;
        };
        parsed += 1;
        read += exercise(&tree);
    };
    for cut in 0..=bytes.len() {
        try_one(bytes.get(..cut).unwrap_or_default().to_vec());
    }
    for i in 0..bytes.len() {
        for flip in [0x01u8, 0x20, 0xFF] {
            let mut m = bytes.to_vec();
            if let Some(b) = m.get_mut(i) {
                *b ^= flip;
            }
            try_one(m);
        }
    }
    (parsed, read)
}

/// A small body with the shapes of section 9 in it.
const BODY: &str = "<w:body><w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr></w:pPr><w:r><w:t>one &amp; </w:t></w:r>\
<w:ins w:id=\"2\" w:author=\"a\"><w:r><w:t>two</w:t></w:r></w:ins><w:del w:id=\"3\"><w:r><w:delText>x</w:delText></w:r></w:del>\
<w:bookmarkStart w:id=\"4\" w:name=\"b\"/></w:p><w:p><w:r><w:fldChar w:fldCharType=\"begin\"/><w:instrText> PAGE </w:instrText>\
<w:fldChar w:fldCharType=\"separate\"/><w:t>3</w:t><w:fldChar w:fldCharType=\"end\"/></w:r><w:bookmarkEnd w:id=\"4\"/>\
<w:fldSimple w:instr=\" REF a \"><w:r><w:t>\u{1F600}</w:t></w:r></w:fldSimple></w:p>\
<w:tbl><w:tr><w:trPr><w:ins w:id=\"5\"/></w:trPr><w:tc><w:p><w:r><w:tab/><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>\
<w:sdt><w:sdtPr/><w:sdtContent><w:p><w:smartTag><w:r><w:t><![CDATA[c<d]]></w:t></w:r></w:smartTag></w:p></w:sdtContent></w:sdt>\
<mc:AlternateContent><mc:Choice Requires=\"wps\"><w:p/></mc:Choice><mc:Fallback><w:p/></mc:Fallback></mc:AlternateContent>\
<w:altChunk r:id=\"r1\"/><w:sectPr><w:headerReference r:id=\"r2\"/></w:sectPr></w:body>";

const STYLES: &str = "<w:style w:styleId=\"A\" w:type=\"paragraph\"><w:name w:val=\"a\"/><w:basedOn w:val=\"B\"/><w:pPr><w:jc w:val=\"left\"/></w:pPr></w:style>\
<w:style w:styleId=\"B\"><w:basedOn w:val=\"A\"/><w:rPr><w:b/><w14:shadow xmlns:w14=\"urn:x\"/></w:rPr></w:style><w:docDefaults/>";

const NUMBERING: &str = "<w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/></w:lvl>\
<w:lvl w:ilvl=\"0\"/></w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"4\"/></w:lvlOverride></w:num>";

const NOTES: &str = "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p/></w:footnote><w:footnote w:id=\"2\"><w:p><w:r><w:footnoteRef/><w:t>note</w:t></w:r></w:p></w:footnote>\
<w:comment w:id=\"3\" w:author=\"x\"><w:p><w:commentRangeStart w:id=\"3\"/><w:r><w:t>c</w:t></w:r><w:commentRangeEnd w:id=\"3\"/></w:p></w:comment>";

fn generated_body(seed: u64, about: usize) -> String {
    let mut g = Gen::new(seed);
    let mut xml = String::new();
    while xml.len() < about {
        xml.push_str(&g.body(1).xml);
    }
    format!("<w:body>{xml}</w:body>")
}

#[test]
fn cutting_and_flipping_every_byte_of_a_body_never_panics() {
    let (parsed, read) = within(120, || sweep(&wrap("document", BODY)));
    assert!(parsed > 500 && read > 2000, "parsed {parsed}, read {read}");
}

#[test]
fn cutting_and_flipping_every_byte_of_a_generated_body_never_panics() {
    for seed in [1, 2] {
        let xml = wrap("document", &generated_body(seed, 700));
        let (parsed, read) = within(120, move || sweep(&xml));
        assert!(parsed > 200 && read > 500, "parsed {parsed}, read {read}");
    }
}

#[test]
fn cutting_and_flipping_every_byte_of_the_other_parts_never_panics() {
    for (root, content) in [
        ("styles", STYLES),
        ("numbering", NUMBERING),
        ("footnotes", NOTES),
        (
            "hdr",
            "<w:p><w:r><w:t>h</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
        ),
        ("document", "<w:body><w:p/></w:body>"),
    ] {
        let xml = wrap(root, content);
        let (parsed, _) = within(120, move || sweep(&xml));
        assert!(parsed > 50, "{root}: parsed {parsed}");
    }
}

#[test]
fn what_the_sweeps_read_is_deterministic_and_the_unmutated_seed_is_whole() {
    let tree = parse_with(256, &wrap("document", BODY)).unwrap();
    let a = exercise(&tree);
    assert_eq!(a, exercise(&tree));
    assert_eq!(
        a, 5,
        "five body paragraphs, the Choice taken and the Fallback not"
    );
}
