//! The conformance suite: the laws that every registered format must keep (design 5.9).
//!
//! It runs on every id in `registry()`, over the hand-made models, over generated models that hold every
//! block and inline kind and every class of character, and over byte-mutated renderings. The laws that
//! belong to one syntax (F3 escapes, F4 lines, the tolerated changes and the messages of F5 and F7) are
//! in the tests of the format that has the syntax. After `document` exists, the testkit gets a copy of
//! F1, F2, F5 and F6 that goes through the public interface.

use super::{Format, registry};
use crate::api::limits::Limits;
use crate::projection::draft::{Draft, DraftLine};
use crate::projection::keys::Key;
use crate::projection::model::*;
use crate::projection::testing::*;
use crate::projection::testing_gen::Rng;
use crate::projection::testing_gen_blocks::generate;

fn limits() -> Limits {
    Limits::default()
}

fn parse_of(format: &dyn Format, p: &Projection) -> Result<Draft, crate::api::error::StetError> {
    format.parse(&format.render(p).files, &limits())
}

fn fixtures() -> Vec<Projection> {
    let mut out = vec![sample()];
    let mut empty = files_projection(vec![body_story(Vec::new())]);
    empty.head.common_style = None;
    out.push(empty);
    out.push(files_projection(vec![body_story(vec![
        plain(1, ""),
        plain(2, " leading"),
        plain(3, "{braces} \\ \"quotes\""),
    ])]));
    out
}

#[test]
fn the_registry_is_stable_and_every_format_has_its_own_id_and_file_names() {
    let ids: Vec<&str> = registry().iter().map(|f| f.id()).collect();
    assert_eq!(ids, vec!["stet/1", "json/1"]);
    for format in registry() {
        let policy = format.policy();
        let mut names: Vec<&str> = policy.files.to_vec();
        names.push(policy.view);
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 5, "{}: file names are distinct", format.id());
    }
}

/// F1: rendering one model twice, and two models built the same way, gives equal files.
#[test]
fn f1_rendering_is_deterministic() {
    for format in registry() {
        for p in fixtures() {
            assert_eq!(
                format.render(&p),
                format.render(&p.clone()),
                "{}",
                format.id()
            );
        }
        for seed in 0..60 {
            assert_eq!(
                format.render(&generate(seed)),
                format.render(&generate(seed)),
                "{} seed {seed}",
                format.id()
            );
        }
    }
}

/// The baseline of a format is what rendering the model and making its draft give.
fn same_baseline(format: &dyn Format, p: &Projection, what: &str) {
    let rendered = format.render(p);
    let parts = format.baseline(p.clone());
    let id = format.id();
    assert_eq!(parts.draft, p.draft(&format.policy()), "{id} {what}: draft");
    assert_eq!(parts.wraps, rendered.wraps, "{id} {what}: wraps");
    let hashes: Vec<(String, [u8; 32])> = rendered
        .files
        .iter()
        .map(|(name, text)| (name.clone(), crate::refs::sha256(text.as_bytes())))
        .collect();
    assert_eq!(parts.hashes, hashes, "{id} {what}: hashes");
}

/// F2: `parse(render(P)) == draft(P)`, and the baseline of an edit, made without the text, is the draft, the places
/// the render breaks lines and the hash of each file that the render writes.
#[test]
fn f2_every_model_parses_back_to_its_draft() {
    for format in registry() {
        let policy = format.policy();
        for (n, p) in fixtures().into_iter().enumerate() {
            assert_eq!(
                parse_of(*format, &p).unwrap(),
                p.draft(&policy),
                "{} fixture {n}",
                format.id()
            );
            same_baseline(*format, &p, &format!("fixture {n}"));
        }
        for seed in 0..250 {
            let p = generate(seed);
            assert_eq!(
                parse_of(*format, &p).unwrap(),
                p.draft(&policy),
                "{} seed {seed}",
                format.id()
            );
            same_baseline(*format, &p, &format!("seed {seed}"));
        }
    }
}

fn first_paragraph(p: &mut Projection) -> &mut Paragraph {
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    let story = stories
        .iter_mut()
        .find(|s| s.kind == crate::wordml::StoryKind::Body)
        .unwrap();
    story.units[0]
        .blocks
        .iter_mut()
        .find_map(|block| match block {
            Block::Paragraph(paragraph) => Some(paragraph),
            _ => None,
        })
        .expect("the sample has a paragraph")
}

/// An edit of a model.
type Edit = Box<dyn Fn(&mut Projection)>;

/// F5, for models: an edit of the model is an edit of the draft that a format parses, and nothing else.
#[test]
fn f5_an_edit_of_a_model_is_an_edit_of_what_the_format_parses() {
    for format in registry() {
        let policy = format.policy();
        let base_draft = sample().draft(&policy);
        let edits: Vec<(&str, Edit)> = vec![
            (
                "a changed slot",
                Box::new(|p| first_paragraph(p).inlines = vec![text("DEFINITIONS AND MORE")]),
            ),
            (
                "a changed tag",
                Box::new(|p| first_paragraph(p).style = Some("Heading 2".into())),
            ),
            (
                "a removed tag",
                Box::new(|p| first_paragraph(p).label = None),
            ),
            (
                "an added seam",
                Box::new(|p| {
                    first_paragraph(p)
                        .inlines
                        .push(Inline::Atom(Atom::PageBreak))
                }),
            ),
            (
                "a changed seam",
                Box::new(|p| first_paragraph(p).inlines.push(del(77, "gone"))),
            ),
            (
                "a changed ref",
                Box::new(|p| first_paragraph(p).r#ref = Some(pref(99))),
            ),
        ];
        for (what, edit) in edits {
            let mut edited = sample();
            edit(&mut edited);
            let parsed = parse_of(*format, &edited).unwrap();
            assert_eq!(parsed, edited.draft(&policy), "{} {what}", format.id());
            assert_ne!(parsed, base_draft, "{} {what} must be seen", format.id());
        }
    }
}

/// A format may vary spelling and never information: every part of every key reaches the draft that a
/// parse gives. Each mutation changes one thing that a reader sees, and the draft must change.
#[test]
fn a_format_loses_nothing_that_a_reader_can_see() {
    let mut rng = Rng::new(5);
    let mut mutated = 0;
    for seed in 0..120 {
        for format in registry() {
            let policy = format.policy();
            let p = generate(seed);
            let mut q = p.clone();
            if !mutate_one(&mut q, &mut rng) {
                continue;
            }
            mutated += 1;
            let before = parse_of(*format, &p).unwrap();
            let after = parse_of(*format, &q).unwrap();
            assert_eq!(after, q.draft(&policy), "{} seed {seed}", format.id());
            assert_ne!(
                after,
                before,
                "{} seed {seed}: a change was not seen",
                format.id()
            );
        }
    }
    assert!(mutated > 100, "{mutated}");
}

/// Changes one thing in the first inline of the model that has one, if it is a thing a reader sees.
fn mutate_one(p: &mut Projection, rng: &mut Rng) -> bool {
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    for story in stories {
        for unit in &mut story.units {
            for block in &mut unit.blocks {
                if let Block::Paragraph(paragraph) = block {
                    if paragraph.inlines.is_empty() {
                        continue;
                    }
                    let at = rng.below(paragraph.inlines.len());
                    if let Some(inline) = paragraph.inlines.get_mut(at)
                        && mutate_inline(inline)
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn mutate_inline(inline: &mut Inline) -> bool {
    match inline {
        Inline::Text(t) => {
            t.push('x');
            true
        }
        Inline::Atom(atom) => {
            *atom = match atom {
                Atom::NoteRef { kind, ordinal } => Atom::NoteRef {
                    kind: *kind,
                    ordinal: *ordinal + 1,
                },
                Atom::Math(m) => Atom::Math(format!("{m}x").into()),
                Atom::Object(o) => Atom::Object(ObjectInfo {
                    alt: Some(format!("{}x", o.alt.as_deref().unwrap_or("")).into()),
                    ..o.clone()
                }),
                Atom::Auto(AutoKind::Page) => Atom::Auto(AutoKind::Date),
                Atom::Auto(_) => Atom::Auto(AutoKind::Page),
                Atom::PageBreak => Atom::ColumnBreak,
                Atom::ColumnBreak => Atom::PageBreak,
                Atom::NoteMark { kind } => Atom::NoteMark {
                    kind: if *kind == NoteKind::Footnote {
                        NoteKind::Endnote
                    } else {
                        NoteKind::Footnote
                    },
                },
            };
            true
        }
        Inline::Rev { tag, .. } => {
            tag.id += 1000;
            true
        }
        Inline::Link { target, .. } => {
            let LinkTarget::External(url) = target;
            *url = format!("{url}x").into();
            true
        }
        Inline::Control { info, .. } => {
            info.alias = Some(format!("{}x", info.alias.as_deref().unwrap_or("")).into());
            true
        }
        Inline::Field(field) => {
            field.text = format!("{}x", field.text).into();
            true
        }
        Inline::Hidden { inlines, .. } | Inline::LooksHidden { inlines, .. } => {
            inlines.push(Inline::Text("x".to_string()));
            true
        }
        Inline::Mark(Mark::CommentStart(id) | Mark::CommentEnd(id)) => {
            *id += 1000;
            true
        }
    }
}

/// F8: arbitrary and mutated files give `Ok` or an error and never panic.
#[test]
fn f8_mutated_renderings_and_arbitrary_text_never_panic() {
    let mut rng = Rng::new(21);
    for format in registry() {
        let names = format.policy();
        for seed in 0..60 {
            for (name, text) in format.render(&generate(seed)).files {
                let mut chars: Vec<char> = text.chars().collect();
                for _ in 0..1 + rng.below(8) {
                    let at = rng.below(chars.len().max(1));
                    match rng.below(3) {
                        0 => {
                            if at < chars.len() {
                                chars.remove(at);
                            }
                        }
                        1 => chars.insert(
                            at,
                            *rng.pick(&['{', '}', '"', '\\', '\n', '[', ']', ',', ':', 'x']),
                        ),
                        _ => chars.truncate(at),
                    }
                }
                let mutated: String = chars.into_iter().collect();
                let _ = format.parse(&[(name, mutated)], &limits());
            }
        }
        for junk in [
            "",
            "\u{0}",
            "{",
            "[",
            "{\"head\":",
            "\u{feff}",
            "\n\n\n",
            &"[".repeat(5000),
        ] {
            for file in names.files.iter().chain(std::iter::once(&names.view)) {
                let _ = format.parse(&[((*file).to_string(), junk.to_string())], &limits());
            }
        }
    }
}

/// Every line kind of a draft appears in the generated models, so the laws above exercise them all.
#[test]
fn the_generated_models_reach_every_kind_of_line_and_tag() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..120 {
        let p = generate(seed);
        for file in &p.draft(&registry()[0].policy()).files {
            for line in &file.lines {
                match line {
                    DraftLine::Block(b) => {
                        seen.insert("block");
                        for key in b.tags.iter().chain(b.seams.iter().map(|s| &s.key)) {
                            seen.insert(variant_name(key));
                        }
                    }
                    DraftLine::Structure(s) => {
                        seen.insert(variant_name(&s.key));
                    }
                }
            }
        }
    }
    for want in [
        "block",
        "Style",
        "Label",
        "Mark",
        "Rev",
        "HiddenPara",
        "JoinsNext",
        "CellPlus",
        "CellSpan",
        "CellVMerge",
        "CellChange",
        "Open",
        "Close",
        "Deleted",
        "Field",
        "Hidden",
        "LooksHidden",
        "Control",
        "Comment",
        "NoteRef",
        "NoteMark",
        "Object",
        "Math",
        "PageBreak",
        "ColumnBreak",
        "Auto",
        "Table",
        "Row",
        "EndTable",
        "Section",
        "Opaque",
        "Unit",
        "Anchored",
    ] {
        assert!(
            seen.contains(want),
            "no generated model reaches {want}: {seen:?}"
        );
    }
}

fn variant_name(key: &Key) -> &'static str {
    match key {
        Key::Style(_) => "Style",
        Key::Label(_) => "Label",
        Key::Mark { .. } => "Mark",
        Key::Rev(_) => "Rev",
        Key::HiddenPara => "HiddenPara",
        Key::JoinsNext => "JoinsNext",
        Key::CellPlus => "CellPlus",
        Key::CellSpan(_) => "CellSpan",
        Key::CellVMerge { .. } => "CellVMerge",
        Key::CellChange { .. } => "CellChange",
        Key::Open(_) => "Open",
        Key::Close(_) => "Close",
        Key::Deleted { .. } => "Deleted",
        Key::Field(_) => "Field",
        Key::Hidden(_) => "Hidden",
        Key::LooksHidden { .. } => "LooksHidden",
        Key::Control { .. } => "Control",
        Key::Comment { .. } => "Comment",
        Key::NoteRef { .. } => "NoteRef",
        Key::NoteMark(_) => "NoteMark",
        Key::Object(_) => "Object",
        Key::Math(_) => "Math",
        Key::PageBreak => "PageBreak",
        Key::ColumnBreak => "ColumnBreak",
        Key::Auto(_) => "Auto",
        Key::Table(_) => "Table",
        Key::Row(_) => "Row",
        Key::EndTable => "EndTable",
        Key::Section(_) => "Section",
        Key::Opaque(_) => "Opaque",
        Key::Margin(_) => "Margin",
        Key::Unit(_) => "Unit",
        Key::Anchored(_) => "Anchored",
    }
}
