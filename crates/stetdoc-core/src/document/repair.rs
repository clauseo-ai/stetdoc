//! The repair of a defect of the input that stops Word opening the file and that the schema allows: a prefix that
//! `mc:Ignorable`, `mc:ProcessContent` or `mc:MustUnderstand` names and no declaration binds (decision 0014).
//!
//! Every write repairs it in every part the check reads, whether or not the edit touched the part, because a file that
//! Word refuses is no use to anyone. A prefix Word knows is declared on the element that names it, after that element's
//! last declaration, and a prefix nobody knows is taken out of the attribute. Nothing else in the part changes.

use crate::api::error::StetError;
use crate::api::view::{Warning, WarningCode};
use crate::check;
use crate::schema::{MC_NS, prefix_namespace, token_prefix};
use crate::state::Tx;

/// Repairs every part the check reads, and returns one warning for each part it changed. It changes nothing when the
/// transaction has changed nothing, so an edit that changes nothing still leaves the bytes as they were.
pub(super) fn repair(tx: &mut Tx<'_>) -> Result<Vec<Warning>, StetError> {
    if tx.changed_parts().is_empty() {
        return Ok(Vec::new());
    }
    let mut warnings = Vec::new();
    for name in check::schema_parts(&tx.parts())? {
        let found = {
            let parts = tx.parts();
            let Some(id) = parts.id(&name) else {
                continue;
            };
            // A part that cannot be read is the check's to report, and the edit's to refuse if it needs it.
            match check::undeclared(&parts, id) {
                Ok(list) if !list.is_empty() => list,
                _ => continue,
            }
        };
        let mut declared: Vec<String> = Vec::new();
        let mut dropped: Vec<String> = Vec::new();
        let tree = tx.part_mut(&name)?;
        for element in found.iter() {
            let mut unknown: Vec<&str> = Vec::new();
            for prefix in element.prefixes() {
                match prefix_namespace(prefix) {
                    Some(uri) => {
                        tree.edit().declare_prefix(element.node, prefix, uri)?;
                        push_once(&mut declared, prefix);
                    }
                    None => {
                        unknown.push(prefix);
                        push_once(&mut dropped, prefix);
                    }
                }
            }
            if unknown.is_empty() {
                continue;
            }
            let attributes: Vec<&'static str> = element.names.iter().map(|(a, _)| *a).collect();
            for attribute in attributes {
                let Some(value) = tree
                    .node_ref(element.node)
                    .attr(MC_NS, attribute)
                    .map(|v| v.into_owned())
                else {
                    continue;
                };
                let kept: Vec<&str> = value
                    .split_whitespace()
                    .filter(|t| token_prefix(attribute, t).is_none_or(|p| !unknown.contains(&p)))
                    .collect();
                let kept = kept.join(" ");
                if kept != value.trim() {
                    tree.edit()
                        .set_attr(element.node, MC_NS, attribute, &kept)?;
                }
            }
        }
        warnings.push(warning(&name, &declared, &dropped));
    }
    Ok(warnings)
}

fn push_once(list: &mut Vec<String>, prefix: &str) {
    if !list.iter().any(|p| p == prefix) {
        list.push(prefix.to_owned());
    }
}

/// The warning for one repaired part, which a receipt shows as `warning repaired: declared w14, wp14 in word/document.xml so
/// Word can open the file: ...`.
fn warning(part: &str, declared: &[String], dropped: &[String]) -> Warning {
    let mut did: Vec<String> = Vec::new();
    if !declared.is_empty() {
        did.push(format!("declared {}", declared.join(", ")));
    }
    if !dropped.is_empty() {
        did.push(format!(
            "removed the unknown {} from the markup-compatibility attributes",
            dropped.join(", ")
        ));
    }
    Warning {
        code: WarningCode::Repaired,
        block: None,
        story: None,
        message: format!(
            "{} in {part} so Word can open the file. The input named them in mc:Ignorable, mc:ProcessContent or mc:MustUnderstand and declared none of them.",
            did.join(" and ")
        ),
        next: None,
    }
}
