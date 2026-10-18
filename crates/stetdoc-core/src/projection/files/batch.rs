//! `to_batch`: the only way a hand edit becomes operations (design 6.1).
//!
//! It checks that the files were written for this state, renders the baseline from the state with the
//! format the files name, parses the edited files with the same format, and aligns the two drafts. What the
//! drafts cannot say is asked of the state: what became of a ref that no file holds, and the style that Enter
//! gives after a heading.

use super::{Baseline, Fate, Outside, StyleRef, align};
use crate::api::error::{ErrorCode, StetError};
use crate::api::files::{Planned, ReadableFiles};
use crate::api::limits::Limits;
use crate::projection::draft::{Draft, DraftLine};
use crate::projection::formats::{Format, files_format};
use crate::projection::keys::Key;
use crate::projection::render::files_model;
use crate::refs::{BaseToken, BlockRef, Claim, RefKind};
use crate::state::State;
use crate::wordml::{StyleDefs, StyleKind};
use std::sync::Arc;

/// What the state knows that the drafts do not.
struct StateOutside<'a> {
    state: &'a State,
    defs: Arc<StyleDefs>,
    /// The refs the files hold, as a sentence for the message of a ref that never existed.
    range: String,
}

/// The ref a line holds, if it is a paragraph, a table or a row.
fn ref_of(line: &DraftLine) -> Option<BlockRef> {
    match line {
        DraftLine::Block(block) => block.r#ref,
        DraftLine::Structure(s) => match &s.key {
            Key::Table(t) => t.r#ref,
            Key::Row(r) => r.r#ref,
            _ => None,
        },
    }
}

/// `p1 to p812, t1 to t14, r1 to r88`: the refs that the files of a baseline hold, by kind.
fn ranges(draft: &Draft) -> String {
    let mut low = [u32::MAX; 3];
    let mut high = [0u32; 3];
    for file in &draft.files {
        for line in &file.lines {
            let Some(r) = ref_of(line) else { continue };
            let k = match r.kind() {
                RefKind::Paragraph => 0,
                RefKind::Table => 1,
                RefKind::Row => 2,
            };
            if let (Some(lo), Some(hi)) = (low.get_mut(k), high.get_mut(k)) {
                *lo = (*lo).min(r.number());
                *hi = (*hi).max(r.number());
            }
        }
    }
    let mut parts = Vec::new();
    for (k, letter) in ["p", "t", "r"].iter().enumerate() {
        if let (Some(lo), Some(hi)) = (low.get(k), high.get(k))
            && *lo <= *hi
        {
            parts.push(format!("{letter}{lo} to {letter}{hi}"));
        }
    }
    if parts.is_empty() {
        "The files hold no refs.".to_string()
    } else {
        format!("The refs of these files are {}.", parts.join(", "))
    }
}

impl Outside for StateOutside<'_> {
    fn fate(&self, r: BlockRef) -> Fate {
        match self.state.resolve(Claim::Implied, &r.to_string()) {
            Ok(_) => Fate::Unknown {
                range: format!(
                    "{r} is a block of this document that these files do not hold. {}",
                    self.range
                ),
            },
            Err(e) if e.code() == ErrorCode::StaleRef => Fate::Gone {
                what: e.message().to_string(),
            },
            Err(_) => Fate::Unknown {
                range: self.range.clone(),
            },
        }
    }

    fn next_style(&self, name: &str) -> Option<StyleRef> {
        let def = self.defs.by_name(name).or_else(|| self.defs.get(name))?;
        let next = self
            .defs
            .get(def.next.as_deref()?)
            .filter(|n| n.kind == StyleKind::Paragraph && n.id != def.id)?;
        Some(StyleRef {
            name: if next.name.is_empty() {
                next.id.to_string()
            } else {
                next.name.to_string()
            },
            id: next.id.to_string(),
        })
    }
}

/// What an edit of the readable files comes to, against `state`.
///
/// `limits` are the limits the state was opened with, which bound the size of a file and its number of lines.
/// `json/1` is never a working-copy format and is refused as `unsupported`.
pub(crate) fn to_batch(
    state: &State,
    files: &ReadableFiles,
    limits: &Limits,
) -> Result<Planned, StetError> {
    let format = files_format(files.format.as_str())?;
    to_batch_in(state, files, format, limits)
}

/// `to_batch` in any registered format, `json/1` included, which the alignment tests use so that a bug in
/// `stet/1`'s parser cannot hide one in the alignment (design 5.8).
pub(crate) fn to_batch_in(
    state: &State,
    files: &ReadableFiles,
    format: &dyn Format,
    limits: &Limits,
) -> Result<Planned, StetError> {
    let recorded = BaseToken::parse(&files.base)?;
    state.check_files_base(&recorded)?;
    // The model of the whole story is the biggest thing in the call. The format drops it as soon as the draft is
    // made, and the text it writes is hashed and dropped file by file, so the baseline costs the draft and no more.
    let model = files_model(state.parts(), state.base_token())?;
    let parts = format.baseline(model);
    let baseline = Baseline {
        draft: parts.draft,
        wraps: parts.wraps,
    };
    let edited = format.parse(&files.files, limits)?;
    let outside = StateOutside {
        state,
        defs: state.parts().style_defs()?,
        range: ranges(&baseline.draft),
    };
    let plan = align(&baseline, &edited, &outside)?;
    Ok(Planned {
        ops: plan.ops,
        origins: plan.origins,
        notes: plan.notes,
        baseline: parts.hashes,
    })
}
