//! `accept` and `reject`: which revisions, and the step that resolves them (design `document-and-ops.md` 2.3 and 3.3).
//!
//! A run of consecutive accepts and rejects is one call of `resolve`, and never moves across an edit. Under a scope, `all` and
//! `author` are narrowed to the changes in the scope's blocks, and an id outside it is `scope_violation`, because resolving can
//! change `styles.xml`, `numbering.xml` and the body's section properties, which no scope can name.

use super::context::Context;
use super::scope::{InScope, changes_in_scope};
use crate::api::error::{ErrorCode, StetError};
use crate::api::op::ReviewOp;
use crate::revisions::types::{Action, Selector, Step};
use crate::state::Tx;

fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The name of the author a payload means: a full name wins over a code when both match.
fn author_name(cx: &Context<'_>, given: &str) -> Result<String, StetError> {
    let by_name = cx.authors.iter().find(|(_, name)| name == given);
    let by_code = cx.authors.iter().find(|(code, _)| code == given);
    match by_name.or(by_code) {
        Some((_, name)) => Ok(name.clone()),
        None => {
            let known: Vec<String> = cx
                .authors
                .iter()
                .map(|(code, name)| format!("{code} ({name})"))
                .collect();
            Err(StetError::new(
                ErrorCode::NotFound,
                format!(
                    "no revision or comment in the document is by `{given}`. The authors are {}.",
                    if known.is_empty() {
                        "none".to_string()
                    } else {
                        known.join(", ")
                    }
                ),
            )
            .with_next("Name an author as the view shows it, by code or by name."))
        }
    }
}

/// The selector of a payload, before any scope narrows it.
fn selector(cx: &Context<'_>, op: &ReviewOp) -> Result<Selector, StetError> {
    match (op.all, &op.author, &op.ids) {
        (Some(true), None, None) => Ok(Selector::All),
        (None, Some(given), None) => {
            let name = author_name(cx, given)?;
            // A revision with no author is shown as `Unknown`, and selected by an empty author.
            Ok(Selector::Author(if name == "Unknown" {
                String::new()
            } else {
                name
            }))
        }
        (None, None, Some(ids)) => Ok(Selector::Ids(ids.clone())),
        (Some(false), ..) => Err(invalid("`all` is true, or left out.")),
        _ => Err(invalid(
            "`accept` and `reject` take exactly one of `all`, `author` and `ids`.",
        )),
    }
}

/// The step of one operation. Under a scope the selector is narrowed to the changes in its blocks.
pub(super) fn step(
    tx: &Tx<'_>,
    cx: &Context<'_>,
    op: &ReviewOp,
    action: Action,
) -> Result<Step, StetError> {
    let selector = selector(cx, op)?;
    let Some(scope) = cx.scope else {
        return Ok(Step { action, selector });
    };
    let InScope {
        changes: in_scope,
        section,
    } = changes_in_scope(tx, scope)?;
    let selector = match selector {
        Selector::All => Selector::Ids(
            in_scope
                .iter()
                .flat_map(|c| c.ids.iter().copied())
                .collect(),
        ),
        Selector::Author(name) => Selector::Ids(
            in_scope
                .iter()
                .filter(|c| c.author.as_deref().map(str::trim).unwrap_or_default() == name)
                .flat_map(|c| c.ids.iter().copied())
                .collect(),
        ),
        Selector::Ids(ids) => {
            for id in &ids {
                if section.contains(id) {
                    return Err(StetError::new(
                        ErrorCode::ScopeViolation,
                        format!(
                            "revision #{id} changes the last section's properties, which name no paragraph, so no scope ({}) holds it.",
                            scope.text()
                        ),
                    )
                    .with_next("Accept or reject it in a batch without a scope."));
                }
                if !in_scope.iter().any(|c| c.ids.contains(id)) {
                    return Err(StetError::new(
                        ErrorCode::ScopeViolation,
                        format!(
                            "revision #{id} is not in the scope ({}), or does not exist.",
                            scope.text()
                        ),
                    )
                    .with_next(
                        "Widen the scope to hold the change, or name a revision that is in it.",
                    ));
                }
            }
            Selector::Ids(ids)
        }
        #[cfg(test)]
        other => other,
    };
    Ok(Step { action, selector })
}
