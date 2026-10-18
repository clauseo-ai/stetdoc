//! Relationships and the parts they name (design `compare.md`, 7.5).
//!
//! An attribute of the `r` namespace in imported content names a relationship of the source part. A hyperlink's address is
//! carried when the revised part already has a relationship to the same address, and otherwise the link is left out (a
//! relationship with no part is `Tx::add_relationship`, which the transaction does not have yet). A part that the content names
//! (a picture) is copied with `Tx::add_part`, under a name the revised package does not have, once for the same bytes.

use super::{Cx, Importer};
use crate::api::error::StetError;
use crate::package::NewPart;
use crate::revisions::import_writer::ImportWriter;
use crate::wordml::RelTarget;
use crate::xml::Bytes;
use std::collections::HashMap;

/// What the importer remembers about relationships.
#[derive(Default)]
pub(super) struct RelState {
    /// (source part, target part, relationship id) to the id in the target part.
    ids: HashMap<(String, String, String), Option<String>>,
    /// Parts copied: the type, the bytes and the name they were given, with the relationship ids made for each target part.
    copied: Vec<Copy>,
}

struct Copy {
    content_type: String,
    bytes: Bytes,
    name: String,
    by_dst: HashMap<String, String>,
}

/// The part that `source` has a relationship of this type to, by the end of the type's URI.
pub(super) fn related(
    view: &crate::state::PartsView<'_>,
    source: &str,
    suffix: &str,
) -> Result<Option<String>, StetError> {
    let Some(id) = view.id(source) else {
        return Ok(None);
    };
    let rels = view.rels(id)?;
    Ok(rels
        .iter()
        .filter(|r| r.rel_type.ends_with(suffix))
        .find_map(|r| match &r.target {
            RelTarget::Part(name) => Some(name.clone()),
            _ => None,
        }))
}

fn free_name(tx: &ImportWriter<'_, '_>, imp: &Importer<'_>, name: &str) -> String {
    let taken = |n: &str| {
        tx.parts().contains(n)
            || imp
                .rels
                .copied
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(n))
    };
    if !taken(name) {
        return name.to_owned();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_owned(), format!(".{e}")),
        None => (name.to_owned(), String::new()),
    };
    let mut i = 1usize;
    loop {
        let candidate = format!("{stem}_o{i}{ext}");
        if !taken(&candidate) {
            return candidate;
        }
        i += 1;
    }
}

/// The id in the revised part for relationship `rid` of the original's part, or `None` when it cannot be carried.
pub(super) fn map(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    cx: Cx<'_>,
    rid: &str,
) -> Result<Option<String>, StetError> {
    let key = (cx.src.to_string(), cx.dst.to_string(), rid.to_owned());
    if let Some(done) = imp.rels.ids.get(&key) {
        return Ok(done.clone());
    }
    let out = map_uncached(imp, tx, cx, rid)?;
    imp.rels.ids.insert(key, out.clone());
    Ok(out)
}

fn map_uncached(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    cx: Cx<'_>,
    rid: &str,
) -> Result<Option<String>, StetError> {
    let orig = imp.orig.parts();
    let Some(src_id) = orig.id(cx.src) else {
        return Ok(None);
    };
    let rels = orig.rels(src_id)?;
    let Some(rel) = rels.get(rid) else {
        return Ok(None);
    };
    match &rel.target {
        RelTarget::External(url) => {
            let view = tx.parts();
            let Some(dst_id) = view.id(cx.dst) else {
                return Ok(None);
            };
            let there = view.rels(dst_id)?;
            Ok(there
                .iter()
                .find(|r| {
                    r.rel_type == rel.rel_type
                        && matches!(&r.target, RelTarget::External(u) if u == url)
                })
                .map(|r| r.id.clone()))
        }
        RelTarget::Part(name) => {
            // a part with relationships of its own (a chart, a diagram) is not carried
            if let Some(pid) = orig.id(name)
                && orig.rels(pid)?.iter().next().is_some()
            {
                return Ok(None);
            }
            let Some(bytes) = orig.part_bytes(name)? else {
                return Ok(None);
            };
            let content_type = orig
                .part_content_type(name)?
                .unwrap_or_else(|| "application/octet-stream".to_owned());
            let dst = cx.dst.to_string();
            if let Some(c) = imp
                .rels
                .copied
                .iter()
                .find(|c| c.content_type == content_type && c.bytes.as_ref() == bytes.as_ref())
                && let Some(id) = c.by_dst.get(&dst)
            {
                return Ok(Some(id.clone()));
            }
            let new_name = match imp
                .rels
                .copied
                .iter()
                .find(|c| c.content_type == content_type && c.bytes.as_ref() == bytes.as_ref())
            {
                Some(c) => c.name.clone(),
                None => free_name(tx, imp, name),
            };
            let id = tx.add_part(
                NewPart {
                    name: &free_name_for_add(tx, imp, &new_name),
                    content_type: &content_type,
                    bytes: bytes.as_ref().to_vec(),
                    related_from: Some((cx.dst, &rel.rel_type)),
                },
                name,
            )?;
            let Some(id) = id else { return Ok(None) };
            match imp
                .rels
                .copied
                .iter_mut()
                .find(|c| c.content_type == content_type && c.bytes.as_ref() == bytes.as_ref())
            {
                Some(c) => {
                    c.by_dst.insert(dst, id.clone());
                }
                None => {
                    imp.stats.parts_copied += 1;
                    let mut by_dst = HashMap::new();
                    by_dst.insert(dst, id.clone());
                    imp.rels.copied.push(Copy {
                        content_type,
                        bytes,
                        name: new_name,
                        by_dst,
                    });
                }
            }
            Ok(Some(id))
        }
        RelTarget::Invalid => Ok(None),
    }
}

/// `add_part` of a part that already exists under this name in the package would replace it, so a second relationship to a part
/// that was copied for another target part gets a name of its own.
fn free_name_for_add(tx: &ImportWriter<'_, '_>, imp: &Importer<'_>, name: &str) -> String {
    if tx.parts().contains(name) {
        free_name(tx, imp, name)
    } else {
        name.to_owned()
    }
}
