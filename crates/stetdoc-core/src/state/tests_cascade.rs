//! Tests of the cascade and numbers accessors: where they find their parts, what each is cached under, and
//! what rebuilds when a part changes. They build small packages with styles, numbering, theme and settings
//! parts, and ask through `PartsView`, as `projection` and `index` will.

use super::tests::*;
use super::*;
use crate::api::error::ErrorCode;
use crate::api::view::Mode;
use crate::numbering::{Note, StoryNumbers};
use crate::package::NewPart;
use crate::styles::Cascade;
use stetdoc_testkit::fixtures::{
    parts,
    zip::{Entry, Zip},
};

pub(super) const STYLES: &str = "word/styles.xml";
pub(super) const NUMBERING: &str = "word/numbering.xml";
pub(super) const THEME: &str = "word/theme/theme1.xml";
pub(super) const SETTINGS: &str = "word/settings.xml";
pub(super) const TRANSITIONAL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub(super) const STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// A `w:styles` part with the default styles and `inner` after them.
pub(super) fn styles_xml(inner: &str) -> String {
    format!(
        "<w:styles xmlns:w=\"{W}\"><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"/>\
         <w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"/>{inner}</w:styles>"
    )
}

/// A `w:numbering` part with one decimal list, `numId` 1.
pub(super) fn numbering_xml(indent: u32) -> String {
    format!(
        "<w:numbering xmlns:w=\"{W}\"><w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\">\
         <w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/>\
         <w:pPr><w:ind w:left=\"{indent}\"/></w:pPr></w:lvl></w:abstractNum>\
         <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num></w:numbering>"
    )
}

/// A theme part whose `accent1` is `accent1`.
pub(super) fn theme_xml(accent1: &str) -> String {
    format!(
        "<a:theme xmlns:a=\"{A}\"><a:themeElements><a:clrScheme name=\"t\"><a:accent1>\
         <a:srgbClr val=\"{accent1}\"/></a:accent1></a:clrScheme></a:themeElements></a:theme>"
    )
}

/// A settings part, with `inner` in it.
pub(super) fn settings_xml(inner: &str) -> String {
    format!("<w:settings xmlns:w=\"{W}\">{inner}</w:settings>")
}

/// A package whose main part has `document` as its XML, and the given parts, each related from the main
/// part by the relationship type `base/kind`. `kind` is `styles`, `numbering`, `theme` or `settings`.
pub(super) fn package_of(document: &str, base: &str, related: &[(&str, &str, String)]) -> Vec<u8> {
    let mut entries = parts::entries_doc(document.as_bytes());
    let mut rels = String::new();
    for (i, (kind, name, xml)) in related.iter().enumerate() {
        entries.push(Entry::new(*name, xml.as_str()));
        let target = name.strip_prefix("word/").unwrap_or(name);
        rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"{base}/{kind}\" Target=\"{target}\"/>",
            i + 1
        ));
    }
    entries.push(Entry::new(
        "word/_rels/document.xml.rels",
        format!(
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>"
        ),
    ));
    Zip::new(entries).build()
}

/// A document with `body` and no section properties.
pub(super) fn document_xml(body: &str) -> String {
    format!("<w:document xmlns:w=\"{W}\"><w:body>{body}</w:body></w:document>")
}

/// All four related parts, with paragraphs in the body.
pub(super) fn full(body: &str) -> State {
    state_from(
        package_of(&document_xml(body), TRANSITIONAL, &all_parts()),
        &Limits::default(),
    )
}

pub(super) fn all_parts() -> Vec<(&'static str, &'static str, String)> {
    vec![
        ("styles", STYLES, styles_xml("")),
        ("numbering", NUMBERING, numbering_xml(720)),
        ("theme", THEME, theme_xml("4472C4")),
        ("settings", SETTINGS, settings_xml("")),
    ]
}

pub(super) fn sorted(mut deps: Vec<Dep>) -> Vec<Dep> {
    deps.sort();
    deps
}

pub(super) fn numbered(n: usize) -> String {
    (0..n)
        .map(|_| {
            "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>\
             <w:r><w:t>item</w:t></w:r></w:p>"
        })
        .collect()
}

/// Changes a part in a transaction so that its generation moves, and nothing it says.
fn touch(tx: &mut Tx<'_>, name: &str) {
    let tree = tx.part_mut(name).unwrap();
    let root = tree.root().id();
    tree.edit().set_attr(root, W, "touched", "1").unwrap();
}

#[test]
fn a_document_with_none_of_the_parts_has_an_empty_cascade_and_no_numbers() {
    let s = state(BODY);
    let view = s.parts();
    let main = id(&s, MAIN);
    assert!(view.style_defs().unwrap().styles().is_empty());
    assert!(view.numbering_defs().unwrap().nums().is_empty());
    assert_eq!(view.cascade().unwrap().default_paragraph_style(), None);
    assert_eq!(view.numbers(main, Mode::Final).unwrap().items().count(), 0);
    assert_eq!(view.cascade_deps().unwrap(), [Dep::Package]);
    assert_eq!(
        sorted(view.numbers_deps(main).unwrap()),
        sorted(vec![Dep::Package, Dep::Part(main)])
    );
}

#[test]
fn the_dependencies_are_the_parts_the_relationships_name() {
    let s = full(&numbered(1));
    let view = s.parts();
    let main = id(&s, MAIN);
    let part = |name| Dep::Part(id(&s, name));
    assert_eq!(
        sorted(view.cascade_deps().unwrap()),
        sorted(vec![
            Dep::Package,
            part(STYLES),
            part(THEME),
            part(SETTINGS)
        ])
    );
    // The numbers read the numbering part and the styles part, and not the theme or the settings.
    assert_eq!(
        sorted(view.numbers_deps(main).unwrap()),
        sorted(vec![
            Dep::Package,
            Dep::Part(main),
            part(NUMBERING),
            part(STYLES)
        ])
    );
}

#[test]
fn the_strict_spelling_of_a_relationship_type_names_the_same_parts() {
    let strict = state_from(
        package_of(&document_xml(&numbered(1)), STRICT, &all_parts()),
        &Limits::default(),
    );
    let plain = full(&numbered(1));
    // The parts have the same names, so the same numbers, in the same order.
    assert_eq!(
        sorted(strict.parts().cascade_deps().unwrap()),
        sorted(plain.parts().cascade_deps().unwrap())
    );
    assert_eq!(
        strict
            .parts()
            .numbers(id(&strict, MAIN), Mode::Final)
            .unwrap()
            .items()
            .count(),
        1
    );
}

#[test]
fn a_relationship_type_that_only_looks_like_one_is_not_one() {
    // A type that names another kind, spells the kind with another case, or sits under another base finds
    // no part, so the cascade has only the package to depend on.
    for (base, kind) in [
        (TRANSITIONAL, "stylesWithEffects"),
        (TRANSITIONAL, "Styles"),
        (TRANSITIONAL, "styles/"),
        ("http://example.com/relationships", "styles"),
    ] {
        let s = state_from(
            package_of(&document_xml(BODY), base, &[(kind, STYLES, styles_xml(""))]),
            &Limits::default(),
        );
        assert_eq!(
            s.parts().cascade_deps().unwrap(),
            [Dep::Package],
            "{base}/{kind}"
        );
    }
}

#[test]
fn the_builders_are_cached_under_the_exported_lists() {
    // Asking `derived` for the same type, variant and the exported dependencies finds the value that the
    // accessor built, so a list is not copied anywhere: a builder that is never run proves it.
    let s = full(&numbered(2));
    let view = s.parts();
    let main = id(&s, MAIN);
    let cascade = view.cascade().unwrap();
    let never = |_: Option<&Cascade>| -> Result<Cascade, StetError> {
        Err(StetError::new(ErrorCode::Internal, "rebuilt"))
    };
    let found = view
        .derived(0, &view.cascade_deps().unwrap(), &never)
        .unwrap();
    assert!(Arc::ptr_eq(&cascade, &found));
    for (mode, variant) in [(Mode::Final, 0), (Mode::Original, 1), (Mode::Redline, 2)] {
        let numbers = view.numbers(main, mode).unwrap();
        let never = |_: Option<&StoryNumbers>| -> Result<StoryNumbers, StetError> {
            Err(StetError::new(ErrorCode::Internal, "rebuilt"))
        };
        let found = view
            .derived(variant, &view.numbers_deps(main).unwrap(), &never)
            .unwrap();
        assert!(Arc::ptr_eq(&numbers, &found), "{mode:?}");
    }
}

#[test]
fn a_value_is_built_once_and_the_state_wrappers_give_the_same_one() {
    let s = full(&numbered(1));
    let main = id(&s, MAIN);
    let a = s.cascade().unwrap();
    assert!(Arc::ptr_eq(&a, &s.parts().cascade().unwrap()));
    let n = s.numbers(main, Mode::Final).unwrap();
    assert!(Arc::ptr_eq(
        &n,
        &s.parts().numbers(main, Mode::Final).unwrap()
    ));
    // The three modes are three values.
    let o = s.numbers(main, Mode::Original).unwrap();
    let r = s.numbers(main, Mode::Redline).unwrap();
    assert!(!Arc::ptr_eq(&n, &o) && !Arc::ptr_eq(&n, &r) && !Arc::ptr_eq(&o, &r));
}

#[test]
fn what_a_part_changes_is_rebuilt_and_nothing_else() {
    let s = full(&numbered(1));
    let view = s.parts();
    let main = id(&s, MAIN);
    let (defs, numbering) = (view.style_defs().unwrap(), view.numbering_defs().unwrap());
    let (cascade, numbers) = (
        view.cascade().unwrap(),
        view.numbers(main, Mode::Final).unwrap(),
    );
    // (the part that changes, whether the cascade, the numbers, the style definitions and the numbering
    // definitions are rebuilt)
    let cases = [
        (STYLES, true, true, true, false),
        (THEME, true, false, false, false),
        (SETTINGS, true, false, false, false),
        (NUMBERING, false, true, false, true),
        (MAIN, false, true, false, false),
    ];
    for (name, c, n, d, g) in cases {
        let mut tx = s.begin();
        touch(&mut tx, name);
        let v = tx.parts();
        let same = |a: bool| if a { "rebuilt" } else { "kept" };
        assert_eq!(
            !Arc::ptr_eq(&cascade, &v.cascade().unwrap()),
            c,
            "{name}: the cascade is {}",
            same(c)
        );
        assert_eq!(
            !Arc::ptr_eq(&numbers, &v.numbers(main, Mode::Final).unwrap()),
            n,
            "{name}: the numbers are {}",
            same(n)
        );
        assert_eq!(
            !Arc::ptr_eq(&defs, &v.style_defs().unwrap()),
            d,
            "{name}: style definitions"
        );
        assert_eq!(
            !Arc::ptr_eq(&numbering, &v.numbering_defs().unwrap()),
            g,
            "{name}: numbering definitions"
        );
    }
}

#[test]
fn what_a_transaction_builds_stays_in_it_until_commit() {
    let s = full(&numbered(1));
    let main = id(&s, MAIN);
    let before = s.cascade().unwrap();
    s.numbers(main, Mode::Final).unwrap();
    let at_start = s.derived.borrow().len();
    let mut tx = s.begin();
    touch(&mut tx, STYLES);
    let staged = tx.parts().cascade().unwrap();
    tx.parts().numbers(main, Mode::Final).unwrap();
    assert!(!Arc::ptr_eq(&before, &staged));
    assert_eq!(
        s.derived.borrow().len(),
        at_start,
        "nothing reached the base"
    );
    assert!(Arc::ptr_eq(&before, &s.cascade().unwrap()));
    // After commit the staged cascade is the committed one, and the theme-free numbers were rebuilt once.
    let next = tx.commit().unwrap().0;
    assert!(Arc::ptr_eq(&staged, &next.cascade().unwrap()));
}

#[test]
fn a_text_edit_keeps_the_cascade_and_the_committed_state_shares_it() {
    let s = full(&numbered(1));
    let main = id(&s, MAIN);
    let cascade = s.cascade().unwrap();
    let numbers = s.numbers(main, Mode::Final).unwrap();
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    assert!(Arc::ptr_eq(&cascade, &tx.parts().cascade().unwrap()));
    assert!(!Arc::ptr_eq(
        &numbers,
        &tx.parts().numbers(main, Mode::Final).unwrap()
    ));
    let next = tx.commit().unwrap().0;
    assert!(Arc::ptr_eq(&cascade, &next.cascade().unwrap()));
}

#[test]
fn adding_a_numbering_part_changes_what_depends_on_the_package() {
    // The body is numbered and the document has no numbering part, so there is nothing to number by.
    // Adding the part, related from the main part, moves the package generation, and the numbers of the
    // story and the empty definitions are rebuilt although no part they were built from changed.
    let s = state_from(
        package_of(
            &document_xml(&numbered(2)),
            TRANSITIONAL,
            &[("styles", STYLES, styles_xml(""))],
        ),
        &Limits::default(),
    );
    let main = id(&s, MAIN);
    assert_eq!(s.numbers(main, Mode::Final).unwrap().items().count(), 0);
    let mut tx = s.begin();
    tx.add_part(NewPart {
        name: NUMBERING,
        content_type:
            "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
        bytes: numbering_xml(360).into_bytes(),
        related_from: Some((MAIN, &format!("{TRANSITIONAL}/numbering"))),
    })
    .unwrap();
    let numbers = tx.parts().numbers(main, Mode::Final).unwrap();
    let labels: Vec<_> = numbers
        .items()
        .filter_map(|(n, _)| numbers.clause_number(n).map(str::to_owned))
        .collect();
    assert_eq!(labels, ["1.", "2."]);
    assert!(
        tx.parts()
            .numbers_deps(main)
            .unwrap()
            .contains(&Dep::Part(tx.parts().id(NUMBERING).unwrap()))
    );
    // The committed state of before is as it was.
    assert_eq!(s.numbers(main, Mode::Final).unwrap().items().count(), 0);
}

#[test]
fn each_mode_is_numbered_as_that_mode() {
    // The first paragraph's mark is a tracked insertion, so in Original it does not exist and the second
    // is the first.
    let inserted = "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>\
        <w:rPr><w:ins w:id=\"1\" w:author=\"a\"/></w:rPr></w:pPr><w:r><w:t>new</w:t></w:r></w:p>";
    let s = full(&format!("{inserted}{}", numbered(1)));
    let main = id(&s, MAIN);
    let story = s.parts().story(main).unwrap();
    let nodes: Vec<_> = story.blocks().iter().map(|e| e.node).collect();
    let labels = |mode| {
        let numbers = s.numbers(main, mode).unwrap();
        nodes
            .iter()
            .map(|n| numbers.clause_number(*n).map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let some = |s: &str| Some(s.to_owned());
    assert_eq!(labels(Mode::Final), [some("1."), some("2.")]);
    assert_eq!(labels(Mode::Original), [None, some("1.")]);
    assert_eq!(labels(Mode::Redline), [some("1."), some("2.")]);
}

#[test]
fn a_story_with_list_num_fields_and_numbered_paragraphs_is_noted() {
    let field = "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>\
        <w:r><w:instrText xml:space=\"preserve\"> LISTNUM x </w:instrText></w:r>\
        <w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>";
    let noted = |body: &str| {
        let s = full(body);
        let main = id(&s, MAIN);
        let numbers = s.numbers(main, Mode::Final).unwrap();
        numbers.notes().contains(&Note::ListNumField)
    };
    assert!(noted(&format!("{field}{}", numbered(1))));
    assert!(!noted(&numbered(1)), "no field, no note");
    assert!(
        !noted(field),
        "no numbered paragraph, nothing to disagree with"
    );
}

#[test]
fn a_part_that_is_not_a_story_is_a_caller_bug() {
    let s = full(&numbered(1));
    let styles = id(&s, STYLES);
    let err = s.numbers(styles, Mode::Final).err().unwrap();
    assert_eq!(err.code(), ErrorCode::Internal);
}

#[test]
fn a_malformed_styles_part_is_an_error_that_names_it() {
    let s = state_from(
        package_of(
            &document_xml(BODY),
            TRANSITIONAL,
            &[("styles", STYLES, "<w:styles></w:oops>".to_owned())],
        ),
        &Limits::default(),
    );
    let err = s.cascade().err().unwrap();
    assert!(err.to_string().contains("styles.xml"), "{err}");
    // The body still reads: only what needs the styles fails.
    assert!(s.parts().story(id(&s, MAIN)).is_ok());
}
