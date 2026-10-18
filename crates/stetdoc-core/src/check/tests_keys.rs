//! Keys, the finding vocabulary and the report builder.

use super::keys::schema_code;
use super::report::Builder;
use super::{Key, KeySet};
use crate::api::check::{CheckReport, Class, Finding, FindingCode};
use crate::schema::ViolationKind;

fn rule(code: FindingCode, subject: &str) -> Key {
    Key::rule(code, subject)
}

fn finding(code: FindingCode, n: usize) -> Finding {
    Finding {
        code,
        class: code.class(),
        part: "word/document.xml".to_string(),
        block: None,
        message: format!("finding {n}"),
    }
}

#[test]
fn codes_have_unique_snake_case_wire_names_that_round_trip() {
    let mut seen = std::collections::BTreeSet::new();
    for &code in FindingCode::ALL {
        let name = code.as_str();
        assert!(seen.insert(name), "{name} twice");
        assert!(
            !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{name} is not snake_case"
        );
        assert!(!name.starts_with('_') && !name.ends_with('_') && !name.contains("__"));
        assert_eq!(FindingCode::from_name(name), Some(code));
        assert!(
            code.description().ends_with('.') && code.description().len() > 12,
            "{name} has no sentence"
        );
    }
    assert_eq!(FindingCode::from_name("no_such_code"), None);
    assert_eq!(FindingCode::from_name(""), None);
}

#[test]
fn codes_are_in_the_order_of_the_enum() {
    // by_code in a report is sorted by this order, so it must be the order of Ord
    let mut sorted = FindingCode::ALL.to_vec();
    sorted.sort();
    assert_eq!(sorted, FindingCode::ALL);
}

#[test]
fn a_schema_violation_has_the_code_schema_and_its_name() {
    let kinds = [
        ViolationKind::WrongRoot,
        ViolationKind::UnexpectedChild,
        ViolationKind::UnknownElement,
        ViolationKind::OutOfOrder,
        ViolationKind::TooMany,
        ViolationKind::NonCanonicalOrder,
        ViolationKind::MissingChild,
        ViolationKind::MustUnderstand,
        ViolationKind::UnexpectedText,
        ViolationKind::UnknownAttribute,
        ViolationKind::MissingAttribute,
        ViolationKind::InvalidAttributeValue,
    ];
    for kind in kinds {
        // a surplus element and a misplaced one are one key, so they are one code
        let name = format!("schema_{}", kind.keyed().name());
        assert_eq!(schema_code(kind).as_str(), name);
        assert_eq!(schema_code(kind).class(), Class::Violation);
    }
    assert_eq!(
        schema_code(ViolationKind::TooMany),
        schema_code(ViolationKind::OutOfOrder)
    );
    let schema = FindingCode::ALL
        .iter()
        .filter(|c| c.as_str().starts_with("schema_"))
        .count();
    assert_eq!(
        schema, 11,
        "twelve kinds, and TooMany is keyed as OutOfOrder"
    );
}

#[test]
fn only_the_notes_the_design_names_are_notes() {
    let notes: Vec<&str> = FindingCode::ALL
        .iter()
        .filter(|c| c.class() == Class::Note)
        .map(|c| c.as_str())
        .collect();
    assert_eq!(
        notes,
        [
            "unanchored",
            "row_missing",
            "para_id_missing",
            "para_id_range",
            "parent_missing",
            "part_unreadable"
        ]
    );
}

#[test]
fn a_new_key_is_one_that_the_origin_lacked_and_counts_do_not_matter() {
    let (a, b, c) = (
        rule(FindingCode::EmptyProperties, "rPr"),
        rule(FindingCode::EmptyProperties, "pPr"),
        rule(FindingCode::TextInDeletion, "t"),
    );
    let before: KeySet = [a.clone(), b.clone()].into_iter().collect();
    // the same key twice is one key
    let mut after = KeySet::default();
    assert!(after.insert(a.clone()));
    assert!(!after.insert(a.clone()));
    assert_eq!(after.len(), 1);
    assert!(after.added_since(&before).is_empty());
    after.insert(c.clone());
    assert_eq!(after.added_since(&before), vec![c.clone()]);
    // a subset adds nothing, and the empty set adds nothing
    assert!(KeySet::default().added_since(&before).is_empty());
    assert_eq!(
        before.added_since(&KeySet::default()),
        vec![b.clone(), a.clone()]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );
}

#[test]
fn the_origin_of_a_merged_block_is_the_union_of_its_parts() {
    let a = rule(FindingCode::EmptyProperties, "rPr");
    let b = rule(FindingCode::EmptyProperties, "pPr");
    let left: KeySet = [a.clone()].into_iter().collect();
    let right: KeySet = [b.clone()].into_iter().collect();
    let merged = left.union(&right);
    assert_eq!(merged.len(), 2);
    assert!(merged.contains(&a) && merged.contains(&b));
    let mut more = left.clone();
    more.extend(&right);
    assert_eq!(more, merged);
    assert_eq!(left.union(&KeySet::default()), left);
    assert!(KeySet::default().is_empty());
}

#[test]
fn the_keys_of_a_schema_report_are_its_violation_keys() {
    use crate::schema::{PartKind, validate_part};
    let tree = super::tests::tree(&super::tests::document_xml(
        "<w:p><w:r/><w:pPr/><w:r/></w:p><w:p><w:pPr><w:jc/></w:pPr></w:p>",
    ));
    let report = validate_part(PartKind::Document, &tree);
    assert!(!report.set.is_empty());
    let keys = KeySet::of_schema(&report.set);
    assert_eq!(keys.len(), report.set.iter().count());
    for (key, _) in report.set.iter() {
        assert!(keys.contains(&Key::Schema(key.clone())));
    }
    let listed: Vec<&Key> = keys.iter().collect();
    assert!(
        listed.windows(2).all(|w| w[0] < w[1]),
        "in key order, once each"
    );
}

#[test]
fn a_key_names_its_code_and_a_schema_key_does_too() {
    assert_eq!(
        rule(FindingCode::BookmarkUnpaired, "3").code(),
        FindingCode::BookmarkUnpaired
    );
    let schema = Key::Schema(crate::schema::ViolationKey {
        kind: ViolationKind::OutOfOrder,
        parent: "w:CT_P/w:p".to_string(),
        subject: "w:r".to_string(),
        extra: String::new(),
    });
    assert_eq!(schema.code(), FindingCode::SchemaOutOfOrder);
    // keys sort, so that anything made from a set is the same every time
    let mut keys = vec![schema.clone(), rule(FindingCode::EmptyProperties, "rPr")];
    keys.sort();
    let again = {
        let mut k = keys.clone();
        k.reverse();
        k.sort();
        k
    };
    assert_eq!(keys, again);
}

#[test]
fn a_report_keeps_counts_for_everything_and_a_list_of_the_first_findings() {
    let mut builder = Builder::new();
    let n = CheckReport::MAX_FINDINGS + 5;
    for i in 0..n {
        builder.add(finding(FindingCode::EmptyProperties, i));
    }
    builder.add(finding(FindingCode::Unanchored, 0));
    let report = builder.finish();
    assert!(!report.ok);
    assert_eq!((report.violations, report.notes), (n, 1));
    assert_eq!(
        report.by_code,
        vec![
            (FindingCode::EmptyProperties, n),
            (FindingCode::Unanchored, 1)
        ]
    );
    assert_eq!(report.findings.len(), CheckReport::MAX_FINDINGS);
    assert_eq!(report.findings[0].message, "finding 0");
}

#[test]
fn violations_are_listed_before_notes_and_notes_never_crowd_them_out() {
    let mut builder = Builder::new();
    for i in 0..CheckReport::MAX_FINDINGS + 10 {
        builder.add(finding(FindingCode::Unanchored, i));
    }
    for i in 0..3 {
        builder.add(finding(FindingCode::TableWithoutRow, i));
    }
    let report = builder.finish();
    assert_eq!(report.violations, 3);
    assert_eq!(report.notes, CheckReport::MAX_FINDINGS + 10);
    assert_eq!(report.findings.len(), CheckReport::MAX_FINDINGS);
    let classes: Vec<Class> = report.findings.iter().take(4).map(|f| f.class).collect();
    assert_eq!(
        classes,
        [
            Class::Violation,
            Class::Violation,
            Class::Violation,
            Class::Note
        ]
    );
    // a report with notes only is ok
    let mut builder = Builder::new();
    builder.add(finding(FindingCode::Unanchored, 0));
    assert!(builder.finish().ok);
    // an empty one is too, and says nothing
    let empty = Builder::new().finish();
    assert!(empty.ok);
    assert_eq!(
        empty,
        CheckReport {
            ok: true,
            ..CheckReport::default()
        }
    );
}

#[test]
fn counting_without_listing_is_how_the_schema_report_keeps_its_totals() {
    let mut builder = Builder::new();
    builder.count(FindingCode::SchemaOutOfOrder, 4000);
    builder.count(FindingCode::SchemaOutOfOrder, 0);
    let report = builder.finish();
    assert_eq!(report.violations, 4000);
    assert!(report.findings.is_empty());
    assert_eq!(report.by_code, vec![(FindingCode::SchemaOutOfOrder, 4000)]);
}
