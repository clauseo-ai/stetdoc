//! Tests of `new_list_instance`.

use super::tests_support::*;
use crate::api::error::ErrorCode;

const NUMBERING: &str = "<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:abstractNum w:abstractNumId=\"4\"><w:multiLevelType w:val=\"multilevel\"/><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/></w:lvl></w:abstractNum>\
<w:num w:numId=\"1\"><w:abstractNumId w:val=\"4\"/></w:num>\
<w:num w:numId=\"7\"><w:abstractNumId w:val=\"4\"/></w:num>\
<w:numIdMacAtCleanup w:val=\"7\"/>\
</w:numbering>";

#[test]
fn a_restart_is_a_new_num_after_the_last_one_and_before_the_cleanup_element() {
    let s = state_with_numbering(&p(&r("item")), NUMBERING);
    let mut tx = s.begin();
    let mut c = tracked(&mut tx);
    let id = c.new_list_instance(1, 0, 5).unwrap();
    c.finish().unwrap();
    assert_eq!(id, 8);
    let xml = part_xml(&tx, "word/numbering.xml");
    assert!(
        xml.contains("<w:num w:numId=\"7\"><w:abstractNumId w:val=\"4\"/></w:num><w:num w:numId=\"8\"><w:abstractNumId w:val=\"4\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"5\"/></w:lvlOverride></w:num><w:numIdMacAtCleanup"),
        "{xml}"
    );
    assert!(schema_added(&s, &tx).is_empty());
}

#[test]
fn the_override_is_for_the_level_asked() {
    let s = state_with_numbering(&p(&r("item")), NUMBERING);
    let mut tx = s.begin();
    let mut c = untracked(&mut tx);
    c.new_list_instance(1, 3, 1).unwrap();
    let xml = part_xml(&tx, "word/numbering.xml");
    assert!(
        xml.contains("<w:lvlOverride w:ilvl=\"3\"><w:startOverride w:val=\"1\"/>"),
        "{xml}"
    );
}

#[test]
fn a_list_that_is_not_there_or_a_bad_number_is_refused() {
    let s = state_with_numbering(&p(&r("item")), NUMBERING);
    let mut tx = s.begin();
    let mut c = tracked(&mut tx);
    assert_eq!(
        c.new_list_instance(99, 0, 1).unwrap_err().code(),
        ErrorCode::NotFound
    );
    assert_eq!(
        c.new_list_instance(1, 9, 1).unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
    assert_eq!(
        c.new_list_instance(1, 0, -1).unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
    let plain = state(&p(&r("no numbering")));
    let mut tx = plain.begin();
    let mut c = tracked(&mut tx);
    assert_eq!(
        c.new_list_instance(1, 0, 1).unwrap_err().code(),
        ErrorCode::NotFound
    );
}

#[test]
fn two_restarts_in_one_transaction_take_two_numbers() {
    let s = state_with_numbering(&p(&r("item")), NUMBERING);
    let mut tx = s.begin();
    let mut c = tracked(&mut tx);
    assert_eq!(c.new_list_instance(1, 0, 1).unwrap(), 8);
    assert_eq!(c.new_list_instance(8, 0, 2).unwrap(), 9);
    let xml = part_xml(&tx, "word/numbering.xml");
    assert!(xml.find("w:numId=\"8\"").unwrap() < xml.find("w:numId=\"9\"").unwrap());
}
