//! The `Revisions` scope: the tracked changes of a document, one line each (design 4.6).
//!
//! It is built on `ledger`, so it lists what `Document::revisions` lists. The page is filled to the budget like the
//! threads, and the cursor is the first revision id of the last change shown, `change:14`. A cursor names a change of
//! the whole document, so a filter that keeps other changes continues after it all the same.

use super::build::{Selection, project};
use super::formats::Format;
use super::ledger::{LedgerFilter, Listed, changes_of};
use super::model::{Body, ChangeList, Output, Projection};
use super::page::{estimate, fit, greedy};
use crate::api::error::{ErrorCode, StetError};
use crate::api::revision::RevisionInfo;
use crate::api::view::{Cursor, Position, RevisionFilter, View, ViewSpec};
use crate::refs::BaseToken;
use crate::state::PartsView;

/// The tokens that the head of a page costs, about, for the first guess of how many changes fit.
const OVERHEAD: u32 = 80;

/// The tokens a line of a change costs beyond its text, about.
const LINE: u32 = 12;

/// A page of the changes that `filter` keeps, from the cursor in `spec` if there is one.
pub(super) fn ledger_view(
    parts: PartsView<'_>,
    spec: &ViewSpec,
    filter: &RevisionFilter,
    format: &dyn Format,
    base: BaseToken,
) -> Result<View, StetError> {
    let keep = LedgerFilter::of(parts, filter)?;
    let all = changes_of(parts, &keep)?;
    // Each kept change with its place in the whole ledger, so that a cursor finds where to go on from.
    let kept: Vec<(usize, &RevisionInfo)> = all
        .iter()
        .enumerate()
        .filter_map(|(index, listed)| listed.shown.as_ref().map(|shown| (index, shown)))
        .collect();
    let start = match &spec.after {
        Some(cursor) => start_after(&all, &kept, cursor)?,
        None => 0,
    };
    let head = project(
        parts,
        base,
        spec.mode,
        Output::View {
            name: "ledger".into(),
        },
        &Selection {
            stories: Vec::new(),
        },
    )?;
    let label = label_of(filter);
    let total = u32::try_from(kept.len()).unwrap_or(u32::MAX);
    let page = |n: usize| page_of(format, &head, &kept, (start, n), total, &label, filter.full);
    let budget = spec.budget.0;
    let left = kept.len().saturating_sub(start);
    let guess = greedy(
        kept.iter().skip(start).map(|(_, change)| weight(change)),
        budget,
        OVERHEAD,
    );
    let n = if left == 0 {
        0
    } else {
        fit(left, budget, guess, &mut |n| Ok(estimate(&page(n).text)))?.max(1)
    };
    let shown = page(n);
    let next = (start + n < kept.len())
        .then(|| shown.position.last.clone())
        .flatten()
        .map(Cursor::new);
    Ok(View {
        base: base.to_string(),
        mode: spec.mode,
        tokens: estimate(&shown.text),
        text: shown.text,
        position: shown.position,
        next,
        warnings: head.head.warnings.clone(),
    })
}

/// A rendered page and where it is.
struct Page {
    text: String,
    position: Position,
}

/// What the position line calls the list: `changes`, and the filter that narrows it.
fn label_of(filter: &RevisionFilter) -> String {
    let mut label = "changes".to_string();
    if let Some(author) = &filter.author {
        label.push_str(&format!(" by {author}"));
    }
    if let Some(kind) = &filter.kind {
        label.push_str(&format!(" of kind {kind}"));
    }
    if !filter.find.is_empty() {
        let texts: Vec<String> = filter.find.iter().map(|f| format!("{f:?}")).collect();
        label.push_str(&format!(" with text {}", texts.join(" or ")));
    }
    label
}

/// The cursor that names a change: its first revision id, `change:14`.
fn cursor_of(change: &RevisionInfo) -> Option<String> {
    change.ids.first().map(|id| format!("change:{id}"))
}

/// The weight of a change for the first guess of how many fit: a quarter of its characters and a line's worth.
fn weight(change: &RevisionInfo) -> u32 {
    let chars: usize = [change.inserted.as_deref(), change.deleted.as_deref()]
        .into_iter()
        .flatten()
        .map(|t| t.chars().count())
        .sum();
    u32::try_from(chars.div_ceil(4))
        .unwrap_or(u32::MAX)
        .saturating_add(LINE)
}

/// The page that holds the kept changes `start..start + n`.
fn page_of(
    format: &dyn Format,
    head: &Projection,
    kept: &[(usize, &RevisionInfo)],
    (start, n): (usize, usize),
    total: u32,
    label: &str,
    full: bool,
) -> Page {
    let end = (start + n).min(kept.len());
    let shown: Vec<RevisionInfo> = kept
        .get(start.min(end)..end)
        .unwrap_or_default()
        .iter()
        .map(|(_, change)| (*change).clone())
        .collect();
    let remaining = kept
        .iter()
        .skip(end)
        .map(|(_, change)| weight(change))
        .fold(0u32, u32::saturating_add);
    let position = Position {
        scope: label.to_string(),
        first: shown.first().and_then(cursor_of),
        last: shown.last().and_then(cursor_of),
        shown: u32::try_from(shown.len()).unwrap_or(u32::MAX),
        total,
        remaining_tokens: remaining,
    };
    let mut projection = Projection {
        head: head.head.clone(),
        body: Body::Ledger(ChangeList {
            total,
            changes: shown,
            full,
        }),
        appendix: Vec::new(),
    };
    projection.head.position = Some(position.clone());
    let text = format
        .render(&projection)
        .files
        .into_iter()
        .next()
        .map(|(_, text)| text)
        .unwrap_or_default();
    Page { text, position }
}

/// Where a page that continues after `cursor` starts: after the change whose revision id it names. `change:14` is read.
fn start_after(
    all: &[Listed],
    kept: &[(usize, &RevisionInfo)],
    cursor: &Cursor,
) -> Result<usize, StetError> {
    let text = cursor.as_str();
    let id: i64 = text
        .strip_prefix("change:")
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| {
            StetError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "{text:?} is not a cursor of the ledger, which is a change such as change:14."
                ),
            )
            .with_next("Use the cursor that the last page of the ledger gave.")
        })?;
    let Some(at) = all.iter().position(|listed| listed.ids.contains(&id)) else {
        return Err(StetError::new(
            ErrorCode::NotFound,
            format!("change:{id} is not a change of this document."),
        )
        .with_next("View the changes again from the start."));
    };
    Ok(kept
        .iter()
        .position(|(index, _)| *index > at)
        .unwrap_or(kept.len()))
}
