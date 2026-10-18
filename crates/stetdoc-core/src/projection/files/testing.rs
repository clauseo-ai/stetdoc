//! Helpers for the tests of the alignment: baselines, edits of rendered text, and a string model of the
//! text that operations are applied to.

use super::{Baseline, Fate, Outside, Plan, StyleRef, align};
use crate::api::error::StetError;
use crate::api::files::PlannedOp;
use crate::api::limits::Limits;
use crate::projection::draft::Policy;
use crate::projection::formats::Format;
use crate::projection::formats::stet::StetFormat;
use crate::projection::model::*;
use crate::refs::BlockRef;

/// An `Outside` with a fixed table of next styles: `(style, next name, next id)`.
pub(crate) struct Fake(pub Vec<(&'static str, &'static str, &'static str)>);

impl Outside for Fake {
    fn fate(&self, r: BlockRef) -> Fate {
        if r.number() >= 900 {
            Fate::Gone {
                what: format!("{r} was merged away."),
            }
        } else {
            Fate::Unknown {
                range: "The refs of this file are p1 to p99.".to_string(),
            }
        }
    }

    fn next_style(&self, name: &str) -> Option<StyleRef> {
        self.0
            .iter()
            .find(|(style, _, _)| *style == name)
            .filter(|(style, next, _)| style != next)
            .map(|(_, next, id)| StyleRef {
                name: (*next).to_string(),
                id: (*id).to_string(),
            })
    }
}

/// The usual styles: a heading is followed by body text, a title by a subtitle, a subtitle by body text.
pub(crate) fn styles() -> Fake {
    Fake(vec![
        ("Heading 1", "Body Text", "BodyText"),
        ("Title", "Subtitle", "Subtitle"),
        ("Subtitle", "Body Text", "BodyText"),
        ("Body Text", "Body Text", "BodyText"),
        ("List Paragraph", "List Paragraph", "ListParagraph"),
    ])
}

pub(crate) fn baseline_of(p: &Projection, format: &dyn Format) -> Baseline {
    let rendered = format.render(p);
    Baseline {
        draft: p.draft(&format.policy()),
        wraps: rendered.wraps,
    }
}

/// The plan for edited files of a model, rendered and parsed by a format. `edit` changes the text of each
/// file by name.
pub(crate) fn plan_with(
    p: &Projection,
    format: &dyn Format,
    outside: &dyn Outside,
    edit: impl Fn(&str, &str) -> String,
) -> Result<Plan, StetError> {
    let baseline = baseline_of(p, format);
    let files: Vec<(String, String)> = format
        .render(p)
        .files
        .into_iter()
        .map(|(name, text)| {
            let edited = edit(&name, &text);
            (name, edited)
        })
        .collect();
    let edited = format.parse(&files, &Limits::default())?;
    align(&baseline, &edited, outside)
}

/// The plan for an edit of the text of `document.md`, in `stet/1`.
pub(super) fn plan(p: &Projection, edit: impl Fn(&str) -> String) -> Result<Plan, StetError> {
    plan_with(p, &StetFormat, &styles(), |name, text| {
        if name == "document.md" {
            edit(text)
        } else {
            text.to_string()
        }
    })
}

pub(super) fn policy() -> Policy {
    StetFormat.policy()
}

/// Replaces the first occurrence of `from` in the text, which must be there.
pub(super) fn swap(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "{from:?} is not in\n{text}");
    text.replacen(from, to, 1)
}

/// The ops as short strings, for assertions.
pub(super) fn show(plan: &Plan) -> Vec<String> {
    plan.ops
        .iter()
        .map(|op| match op {
            PlannedOp::Replace {
                block,
                start,
                end,
                with,
            } => format!("replace {block} {start}..{end} {with:?}"),
            PlannedOp::Insert {
                anchor,
                place,
                paragraphs,
            } => {
                let list: Vec<String> = paragraphs
                    .iter()
                    .map(|p| match &p.style {
                        Some(style) => format!("{:?} [{style}]", p.text),
                        None => format!("{:?}", p.text),
                    })
                    .collect();
                format!("insert {place:?} {anchor}: {}", list.join(", "))
            }
            PlannedOp::Delete { block } => format!("delete {block}"),
            PlannedOp::RowDelete { row } => format!("row delete {row}"),
            PlannedOp::Split { block, at } => format!("split {block} {at}"),
            PlannedOp::Merge { first, second } => format!("merge {first} {second}"),
        })
        .collect()
}

/// The text of a paragraph's inlines in the `Final` text, worked out here and not by the model's own
/// functions: the characters of the text, one character for an atom, a field's width, nothing for a deletion.
pub(super) fn final_chars(inlines: &[Inline]) -> Vec<char> {
    let mut out = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.extend(t.chars()),
            Inline::Atom(_) => out.push('\u{1}'),
            Inline::Rev {
                kind: RevKind::Del | RevKind::MoveFrom,
                ..
            } => {}
            Inline::Rev { inlines, .. }
            | Inline::Link { inlines, .. }
            | Inline::Control { inlines, .. }
            | Inline::Hidden { inlines, .. }
            | Inline::LooksHidden { inlines, .. } => out.extend(final_chars(inlines)),
            Inline::Field(f) => {
                let mut shown: Vec<char> = f.text.chars().collect();
                shown.resize(f.width as usize, '?');
                out.extend(shown);
            }
            Inline::Mark(_) => {}
        }
    }
    out
}

/// Applies the replacements of a plan to the text of one paragraph.
pub(super) fn apply_replaces(text: &mut Vec<char>, plan: &Plan, block: &str) {
    for op in &plan.ops {
        if let PlannedOp::Replace {
            block: b,
            start,
            end,
            with,
        } = op
            && b == block
        {
            let replacement: Vec<char> = with.chars().collect();
            text.splice(*start as usize..*end as usize, replacement);
        }
    }
}

pub(super) fn plain_doc(texts: &[&str]) -> Projection {
    use crate::projection::testing::{body_story, files_projection, plain};
    let blocks = texts
        .iter()
        .enumerate()
        .map(|(i, t)| plain(i as u32 + 1, t))
        .collect();
    files_projection(vec![body_story(blocks)])
}
