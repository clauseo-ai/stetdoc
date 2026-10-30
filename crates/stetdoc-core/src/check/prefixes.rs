//! The rule that no markup-compatibility attribute names a prefix that nothing declares (`mc_prefix_undeclared`). The
//! schema allows such an attribute and Word refuses the file, so the schema keys cannot catch it. `document` reports it on
//! every part the check reads, and the edit pipeline repairs what [`undeclared`] lists (decision 0014).

use super::found::Found;
use super::keys::Key;
use crate::api::check::FindingCode;
use crate::api::error::StetError;
use crate::schema::{Undeclared, prefix_namespace, undeclared_prefixes};
use crate::state::{Dep, PartId, PartsView};
use crate::xml::NodeId;
use std::sync::Arc;

/// The variant of the cache that holds the undeclared prefixes of a part.
const UNDECLARED: u64 = 4;

/// The elements of a part whose markup-compatibility attributes name undeclared prefixes.
pub(crate) type Prefixes = Vec<Undeclared<NodeId>>;

/// The elements of a part whose `mc:Ignorable`, `mc:ProcessContent` or `mc:MustUnderstand` names a prefix that no
/// declaration binds, from the cache when the part has not changed since they were found.
pub(crate) fn undeclared(parts: &PartsView<'_>, id: PartId) -> Result<Arc<Prefixes>, StetError> {
    let tree = parts.tree(id)?;
    let build =
        |_: Option<&Prefixes>| -> Result<Prefixes, StetError> { Ok(undeclared_prefixes(tree)) };
    parts.derived(UNDECLARED, &[Dep::Part(id)], &build)
}

/// One finding for each element of `list`.
pub(super) fn findings(list: &Prefixes) -> Vec<Found> {
    let code = FindingCode::McPrefixUndeclared;
    list.iter()
        .map(|u| {
            let mut said: Vec<String> = Vec::new();
            for (attribute, _) in &u.names {
                let named: Vec<&str> = u
                    .names
                    .iter()
                    .filter(|(a, _)| a == attribute)
                    .map(|(_, p)| p.as_str())
                    .collect();
                let line = format!("mc:{attribute} names {}", named.join(", "));
                if !said.contains(&line) {
                    said.push(line);
                }
            }
            let prefixes = u.prefixes().join(", ");
            let (known, unknown): (Vec<&str>, Vec<&str>) = u
                .prefixes()
                .into_iter()
                .partition(|p| prefix_namespace(p).is_some());
            let mut repair: Vec<String> = Vec::new();
            if !known.is_empty() {
                repair.push(format!("declares {}", known.join(", ")));
            }
            if !unknown.is_empty() {
                repair.push(format!("removes {} from the attribute", unknown.join(", ")));
            }
            Found {
                key: Key::rule(code, prefixes.clone()),
                code,
                node: Some(u.node),
                message: format!(
                    "{}, which no namespace declaration binds, so Word refuses the file or offers to repair it. stetdoc {} when it writes the document.",
                    said.join(" and "),
                    repair.join(" and ")
                ),
            }
        })
        .collect()
}
