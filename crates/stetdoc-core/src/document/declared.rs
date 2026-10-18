//! `Declared` for `check::edit`, from the origins (design `document-and-ops.md` 3.1 step 7 and `check.md` 7.1).
//!
//! `revisions` says where each new or surviving block came from (`Outcome::origins`, `Resolution::origins`), and `ops` keeps them
//! in order in `Applied::origins`. `check` may not depend on `revisions` and `ops` may not depend on `check`, so `document` converts
//! them, and drops what `check` would call a bug of the converter:
//!
//! - an origin that names a block of a **part with no schema** (a block has keys only in a part the schema covers), and
//! - a source that is **not in the base and was made from nothing** by an earlier call of the same edit, which has no keys to give.
//!
//! A source that an earlier call of the edit made from something stays, and `check` follows it through that call's origin.

use crate::api::error::StetError;
use crate::check::{self, Declared};
use crate::revisions::outcome::Origin;
use crate::state::PartsView;
use crate::wordml::BlockKey;
use std::collections::HashSet;

/// The declarations of an edit, for `check::edit`. `before` is the base's view and `after` the transaction's.
pub(super) fn build(
    before: PartsView<'_>,
    after: PartsView<'_>,
    origins: &[Origin],
) -> Result<Declared, StetError> {
    let schema = check::schema_parts(&after)?;
    let has_schema = |key: &BlockKey| schema.iter().any(|name| **name == **key.part());
    let in_base = |key: &BlockKey| {
        before
            .id(key.part())
            .and_then(|id| before.tree(id).ok())
            .is_some_and(|tree| tree.node_ref(key.node()).is_element())
    };
    // The blocks that some origin says it made, wherever it stands in the list.
    let made: HashSet<&BlockKey> = origins
        .iter()
        .map(|o| &o.block)
        .filter(|b| has_schema(b))
        .collect();
    let converted = origins
        .iter()
        .filter(|o| has_schema(&o.block))
        .map(|o| check::Origin {
            block: o.block.clone(),
            from: o
                .from
                .iter()
                .filter(|from| has_schema(from) && (in_base(from) || made.contains(from)))
                .cloned()
                .collect(),
        })
        .collect();
    Ok(Declared {
        origins: converted,
        foreign: Vec::new(),
        parts: Vec::new(),
    })
}
