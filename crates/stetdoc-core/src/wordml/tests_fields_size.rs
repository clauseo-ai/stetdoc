//! The size of what the field index holds, which must follow the input and not grow with the product
//! of a long instruction and the paragraphs a field is open across.

use super::tests_fields::{begin, instr, separate, text};
use super::*;
use std::sync::Arc;

#[test]
fn a_field_open_across_many_paragraphs_does_not_copy_its_instruction_into_each() {
    // one field with a 10,000 byte instruction stays open over 500 paragraphs: what the index holds
    // must stay a small multiple of the input, not 2 * 500 * 10,000 bytes
    let big = "A".repeat(10_000);
    let mut body = format!(
        "<w:p>{}</w:p>",
        begin() + &instr(&format!(" REF {big} ")) + &separate()
    );
    for _ in 0..500 {
        body.push_str(&format!("<w:p>{}</w:p>", text("entry")));
    }
    let tree = super::testing::document(&body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let index = story.field_index(&tree);
    let held: usize = index
        .paras
        .values()
        .flat_map(|s| s.start.iter().chain(&s.end))
        .map(|f| f.instr.len())
        .sum();
    assert_eq!(held, 0);
    assert_eq!(
        story.fields(&tree)[0].instr.len(),
        10_000 + 6,
        "the field list keeps it once"
    );
    assert_eq!(index.paras.len(), 501);
}
