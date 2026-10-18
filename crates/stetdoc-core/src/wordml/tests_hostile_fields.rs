//! The field pass must not copy the instruction of an open field into the state of every paragraph it
//! spans (a part of 116 KB once held 160 MB). The index may hold a small multiple of the input.

use super::tests_hostile::{parse_with, within, wrap};
use super::*;
use std::sync::Arc;

#[test]
fn a_field_open_across_many_paragraphs_does_not_copy_its_instruction_into_each() {
    within(90, || {
        for (instr, paras) in [(10_000usize, 500usize), (10_000, 2000), (40_000, 2000)] {
            let code = "A".repeat(instr);
            let mut body = format!(
                "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/><w:instrText>{code}</w:instrText>\
                 <w:fldChar w:fldCharType=\"separate\"/></w:r></w:p>"
            );
            body.push_str(&"<w:p><w:r><w:t>entry</w:t></w:r></w:p>".repeat(paras));
            let xml = wrap("document", &format!("<w:body>{body}</w:body>"));
            let tree = parse_with(256, &xml).unwrap();
            let story =
                Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
            let held: usize = story
                .field_index(&tree)
                .paras
                .values()
                .map(|p| {
                    p.start
                        .iter()
                        .chain(&p.end)
                        .map(|f| f.instr.len())
                        .sum::<usize>()
                })
                .sum();
            assert!(
                held <= 4 * xml.len(),
                "the index holds {held} bytes of instruction for {} bytes of input",
                xml.len()
            );
        }
    });
}
