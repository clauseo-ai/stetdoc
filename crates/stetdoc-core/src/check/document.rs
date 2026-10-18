//! `check::document`: every part of a document, every unit, every finding. It blocks nothing: a
//! report says what is wrong, and a problem that was already there is the answer to a question
//! that was asked.

use super::census::{Census, revision_findings};
use super::found::Found;
use super::keys::schema_code;
use super::report::{Builder, schema_message};
use super::rules::scan_part;
use super::units::{Layout, layout_of};
use super::{audit, fields};
use crate::api::check::{CheckReport, Class, Finding};
use crate::api::error::StetError;
use crate::schema::{PartKind, Report, is_unit_boundary, unit_root, validate_part};
use crate::state::{Dep, PartId, PartsView};
use crate::wordml::{BlockKey, RelTarget};
use crate::xml::{NodeId, SpanTree};
use std::sync::Arc;

/// The variant of the cache that holds the census of a part.
const CENSUS: u64 = 1;

/// Whether a part of this kind has paragraphs: the Word rules are read in it, and its ids and
/// ranges are counted.
pub(super) fn is_story(kind: PartKind) -> bool {
    layout_of(kind) != Layout::Elements
}

/// A part the schema covers: its number, its name and what kind it is.
pub(super) struct SchemaPart {
    pub(super) id: PartId,
    pub(super) name: Arc<str>,
    pub(super) kind: PartKind,
}

/// The parts of a document that the check reads.
pub(super) struct Listing {
    /// The parts that have a schema, in a fixed order: the main part, then the parts the main
    /// part's relationships point at, once each, in the order the relationships list them.
    pub(super) parts: Vec<SchemaPart>,
    /// How many parts the main part names that have none (an image, a theme, a chart), which are
    /// not read.
    pub(super) without_schema: usize,
}

/// The parts of a document that the check reads, found through the relationships of the main part.
pub(super) fn listing(parts: &PartsView<'_>) -> Result<Listing, StetError> {
    let stories = parts.story_specs()?;
    let main = stories.0.first().and_then(|s| parts.id(&s.part));
    let mut out = Listing {
        parts: Vec::new(),
        without_schema: 0,
    };
    let Some(main) = main else {
        return Ok(out);
    };
    if let Some(name) = parts.name(main) {
        out.parts.push(SchemaPart {
            id: main,
            name: name.clone(),
            kind: PartKind::Document,
        });
    }
    let mut skipped: Vec<PartId> = Vec::new();
    for rel in parts.rels(main)?.iter() {
        let RelTarget::Part(target) = &rel.target else {
            continue;
        };
        let Some(id) = parts.id(target) else {
            continue;
        };
        if id == main || out.parts.iter().any(|p| p.id == id) || skipped.contains(&id) {
            continue;
        }
        match (
            PartKind::from_relationship_type(&rel.rel_type),
            parts.name(id),
        ) {
            (Some(kind), Some(name)) => out.parts.push(SchemaPart {
                id,
                name: name.clone(),
                kind,
            }),
            _ => skipped.push(id),
        }
    }
    out.without_schema = skipped.len();
    Ok(out)
}

/// The names of the parts that have a schema, which are the parts the check reads. A block of any other part has no keys, so an
/// origin cannot name one, and `document` drops the origins that do.
pub(crate) fn schema_parts(parts: &PartsView<'_>) -> Result<Vec<Arc<str>>, StetError> {
    Ok(listing(parts)?.parts.into_iter().map(|p| p.name).collect())
}

/// The ref of the block that holds `node`, if that block has one.
pub(super) fn block_ref(
    parts: &PartsView<'_>,
    part: &Arc<str>,
    tree: &SpanTree,
    node: NodeId,
) -> Option<String> {
    let unit = unit_root(tree, node);
    if !is_unit_boundary(tree, unit) {
        return None;
    }
    parts
        .ref_of(&BlockKey::new(part.clone(), unit))
        .map(|r| r.to_string())
}

/// The census of a story part, from the cache when the part has not changed since it was built.
pub(super) fn census_of(
    parts: &PartsView<'_>,
    id: PartId,
    tree: &SpanTree,
) -> Result<Arc<Census>, StetError> {
    let build = |_: Option<&Census>| -> Result<Census, StetError> { Ok(Census::of(tree)) };
    parts.derived(CENSUS, &[Dep::Part(id)], &build)
}

/// What a report needs to turn a finding into a line: the view, to find refs, and the part.
struct Source<'a, 'v> {
    parts: &'a PartsView<'v>,
    name: &'a Arc<str>,
    tree: &'a SpanTree,
}

impl Source<'_, '_> {
    fn finding(&self, found: &Found) -> Finding {
        Finding {
            code: found.code,
            class: found.code.class(),
            part: self.name.to_string(),
            block: found
                .node
                .and_then(|n| block_ref(self.parts, self.name, self.tree, n)),
            message: found.message.clone(),
        }
    }
}

/// The schema findings of a part: counts for every key, and the first ones in detail.
fn schema(builder: &mut Builder, source: &Source<'_, '_>, report: &Report<NodeId>) {
    for (key, n) in report.set.iter() {
        builder.count(schema_code(key.kind), n as usize);
    }
    for v in &report.violations {
        let code = schema_code(v.kind);
        if !builder.has_room(Class::Violation) {
            break;
        }
        builder.list(Finding {
            code,
            class: Class::Violation,
            part: source.name.to_string(),
            block: block_ref(source.parts, source.name, source.tree, v.node),
            message: schema_message(source.tree, v),
        });
    }
}

fn add_found(builder: &mut Builder, source: &Source<'_, '_>, found: &Found) {
    builder.count(found.code, 1);
    if builder.has_room(found.code.class()) {
        builder.list(source.finding(found));
    }
}

/// Everything the check knows about a document: the schema, the Word rules, the comment audit and
/// the package rules, over every part. A part that cannot be read is a note.
pub(crate) fn document(parts: &PartsView<'_>) -> CheckReport {
    let mut builder = Builder::new();
    let list = match listing(parts) {
        Ok(list) => list,
        Err(why) => {
            builder.unreadable("the main part", &why);
            return builder.finish();
        }
    };
    builder.skipped(list.without_schema);
    let mut censuses: Vec<Arc<Census>> = Vec::new();
    for part in &list.parts {
        let tree = match parts.tree(part.id) {
            Ok(tree) => tree,
            Err(why) => {
                builder.unreadable(&part.name, &why);
                continue;
            }
        };
        builder.examined();
        let source = Source {
            parts,
            name: &part.name,
            tree,
        };
        schema(&mut builder, &source, &validate_part(part.kind, tree));
        if !is_story(part.kind) {
            continue;
        }
        for found in scan_part(tree) {
            add_found(&mut builder, &source, &found);
        }
        match census_of(parts, part.id, tree) {
            Ok(census) => {
                for found in census.part_findings(&part.name) {
                    add_found(&mut builder, &source, &found);
                }
                // the glossary is not one of the stories, and has no field index
                if let Some(story) = census
                    .has_fields()
                    .then(|| parts.story(part.id).ok())
                    .flatten()
                {
                    for paragraph in fields::split_paragraphs(&story, tree) {
                        add_found(&mut builder, &source, &fields::finding(paragraph));
                    }
                }
                censuses.push(census);
            }
            Err(why) => builder.unreadable(&part.name, &why),
        }
    }
    let refs: Vec<&Census> = censuses.iter().map(|c| &**c).collect();
    for found in revision_findings(&refs) {
        builder.count(found.code, 1);
        builder.list(Finding {
            code: found.code,
            class: found.code.class(),
            part: String::new(),
            block: None,
            message: found.message,
        });
    }
    match audit::audit_of(parts, &list.parts) {
        Ok(audited) => {
            for item in &audited.0 {
                let (at, found) = item.found(&list.parts);
                builder.add(Finding {
                    code: item.code,
                    class: item.class,
                    part: at
                        .and_then(|at| list.parts.get(at))
                        .map(|p| p.name.to_string())
                        .unwrap_or_default(),
                    block: None,
                    message: found.message,
                });
            }
        }
        Err(why) => builder.unreadable("the comment parts", &why),
    }
    builder.finish()
}
