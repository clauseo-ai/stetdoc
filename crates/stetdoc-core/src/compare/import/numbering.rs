//! Lists of imported content (design `compare.md`, 7.4).
//!
//! A paragraph's list is its `numId`. For each `numId` of the copied content: if the revised document has a `w:num` with the
//! same definition (the abstract numbering's levels, formats, texts and starts, and the overrides, compared with the ids,
//! `w:nsid` and `w:tmpl` left out) the original's id maps to it, so a deleted clause is counted in its own list and shows the
//! number it had. Otherwise the abstract numbering and the `w:num` are copied under fresh ids.

use super::Importer;
use super::canon::canon;
use super::parts::related;
use super::snip::{Kid, Reader, Snip};
use crate::api::error::StetError;
use crate::revisions::import_writer::{Detached, ImportWriter};
use crate::revisions::redline_types::ImportedDef;
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::collections::HashMap;

/// What the importer remembers about numbering.
#[derive(Default)]
pub(super) struct NumState {
    ready: bool,
    orig_part: Option<String>,
    rev_part: Option<String>,
    orig_nums: HashMap<String, NodeId>,
    orig_abs: HashMap<String, NodeId>,
    /// The revised document's lists: id and canonical definition, in the order of the part.
    rev: Vec<(String, String)>,
    map: HashMap<String, Option<String>>,
    next_abs: i64,
    next_num: i64,
}

fn attr(n: &NodeRef<'_>, local: &str) -> Option<String> {
    n.attrs()
        .find(|a| a.local_name() == local)
        .map(|a| a.value().into_owned())
}

fn abs_of(n: &NodeRef<'_>) -> Option<String> {
    n.child_elements()
        .find(|c| c.local_name() == "abstractNumId")
        .and_then(|c| attr(&c, "val"))
}

/// The canonical definition of the list `num`: its abstract numbering without the identifiers and style links, and its overrides.
fn definition(tree: &SpanTree, num: &NodeRef<'_>, abs: &HashMap<String, NodeId>) -> String {
    let mut out = String::new();
    if let Some(id) = abs_of(num)
        && let Some(&a) = abs.get(&id)
    {
        let a = tree.node_ref(a);
        let mut s = String::new();
        for c in a.child_elements() {
            if matches!(c.local_name(), "nsid" | "tmpl" | "name") {
                continue;
            }
            if c.local_name() == "lvl" {
                s.push_str(&lvl_canon(&c));
            } else {
                s.push_str(&canon(&c, &[]));
            }
        }
        out.push_str(&s);
    }
    out.push('#');
    for c in num.child_elements() {
        if c.local_name() == "lvlOverride" {
            out.push_str(&canon(&c, &[]));
        }
    }
    out
}

/// A level without its link to a style, which Word manages from the style's side.
fn lvl_canon(lvl: &NodeRef<'_>) -> String {
    let mut out = String::new();
    out.push_str("<lvl");
    if let Some(i) = attr(lvl, "ilvl") {
        out.push_str(&i);
    }
    out.push('>');
    for c in lvl.child_elements() {
        if c.local_name() == "pStyle" {
            continue;
        }
        out.push_str(&canon(&c, &[]));
    }
    out
}

fn load(imp: &mut Importer<'_>, tx: &ImportWriter<'_, '_>) -> Result<(), StetError> {
    imp.nums.ready = true;
    let (va, vb) = (imp.orig.parts(), tx.parts());
    let (sa, sb) = (va.story_specs()?, vb.story_specs()?);
    let (Some(ba), Some(bb)) = (sa.0.first(), sb.0.first()) else {
        return Ok(());
    };
    imp.nums.orig_part = related(&va, &ba.part, "/numbering")?;
    imp.nums.rev_part = related(&vb, &bb.part, "/numbering")?;
    if let Some(name) = &imp.nums.orig_part
        && let Some(id) = va.id(name)
    {
        for c in va.tree(id)?.root().child_elements() {
            match c.local_name() {
                "num" => {
                    if let Some(i) = attr(&c, "numId") {
                        imp.nums.orig_nums.insert(i, c.id());
                    }
                }
                "abstractNum" => {
                    if let Some(i) = attr(&c, "abstractNumId") {
                        imp.nums.orig_abs.insert(i, c.id());
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(name) = &imp.nums.rev_part
        && let Some(id) = vb.id(name)
    {
        let tree = vb.tree(id)?;
        let mut abs: HashMap<String, NodeId> = HashMap::new();
        let (mut max_abs, mut max_num) = (-1i64, 0i64);
        for c in tree.root().child_elements() {
            if c.local_name() == "abstractNum"
                && let Some(i) = attr(&c, "abstractNumId")
            {
                max_abs = max_abs.max(i.parse().unwrap_or(-1));
                abs.insert(i, c.id());
            }
        }
        for c in tree.root().child_elements() {
            if c.local_name() == "num"
                && let Some(i) = attr(&c, "numId")
            {
                max_num = max_num.max(i.parse().unwrap_or(0));
                let def = definition(tree, &c, &abs);
                imp.nums.rev.push((i, def));
            }
        }
        imp.nums.next_abs = max_abs + 1;
        imp.nums.next_num = max_num + 1;
    }
    Ok(())
}

/// The id in the revised document of the original's list `id`, or `None` when the content has to lose its list.
pub(super) fn map(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    id: &str,
) -> Result<Option<String>, StetError> {
    if id == "0" {
        return Ok(Some("0".to_owned()));
    }
    if !imp.nums.ready {
        load(imp, tx)?;
    }
    if let Some(done) = imp.nums.map.get(id) {
        return Ok(done.clone());
    }
    let out = define(imp, tx, id)?;
    imp.nums.map.insert(id.to_owned(), out.clone());
    Ok(out)
}

fn define(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    id: &str,
) -> Result<Option<String>, StetError> {
    let rev_part = match imp.nums.rev_part.clone() {
        Some(p) => p,
        None => match add_numbering_part(imp, tx)? {
            Some(p) => p,
            None => {
                imp.stats.lists_dropped += 1;
                return Ok(None);
            }
        },
    };
    let (Some(&num_node), Some(orig_part)) =
        (imp.nums.orig_nums.get(id), imp.nums.orig_part.clone())
    else {
        return Ok(None);
    };
    let view = imp.orig.parts();
    let pid = view
        .id(&orig_part)
        .ok_or_else(|| super::internal("the original's numbering part is gone"))?;
    let tree = view.tree(pid)?;
    let num = tree.node_ref(num_node);
    let def = definition(tree, &num, &imp.nums.orig_abs);
    // the same definition there: the same id if it has one, else the lowest
    let same: Vec<&(String, String)> = imp.nums.rev.iter().filter(|(_, d)| *d == def).collect();
    if let Some(found) = same.iter().find(|(i, _)| i == id).or_else(|| {
        same.iter()
            .min_by_key(|(i, _)| i.parse::<i64>().unwrap_or(i64::MAX))
    }) {
        return Ok(Some(found.0.clone()));
    }
    // a new definition: the abstract numbering, then the list
    let abs_id = abs_of(&num);
    let mut reader = Reader::new(tree);
    let abs_snip = abs_id
        .as_ref()
        .and_then(|i| imp.nums.orig_abs.get(i))
        .and_then(|&n| reader.read(n));
    let Some(mut abs_snip) = abs_snip else {
        return Ok(None);
    };
    let Some(mut num_snip) = reader.read(num_node) else {
        return Ok(None);
    };
    let new_abs = imp.nums.next_abs;
    let new_num = imp.nums.next_num;
    imp.nums.next_abs += 1;
    imp.nums.next_num += 1;
    abs_snip.set_w_attr("abstractNumId", &new_abs.to_string());
    abs_snip
        .kids
        .retain(|k| !matches!(k, Kid::El(e) if e.is_w("nsid") || e.is_w("tmpl")));
    strip_pstyle(&mut abs_snip);
    num_snip.set_w_attr("numId", &new_num.to_string());
    if let Some(Kid::El(e)) = num_snip
        .kids
        .iter_mut()
        .find(|k| matches!(k, Kid::El(e) if e.is_w("abstractNumId")))
    {
        e.set_w_attr("val", &new_abs.to_string());
    }
    strip_rsid(&mut abs_snip);
    strip_rsid(&mut num_snip);
    let (abs_src, num_src) = (abs_snip.src, num_snip.src);
    let tree = tx.part_mut(&rev_part)?;
    let (a, n) = {
        let mut d = Detached::over(tree);
        super::snip::bind_used(&mut d, &reader.used)?;
        let w = super::transform::w_ns(d.tree());
        let a = super::snip::write(&mut d, &abs_snip, &w, &reader.requires)?;
        let n = super::snip::write(&mut d, &num_snip, &w, &reader.requires)?;
        (a, n)
    };
    let root = tree.root().id();
    // abstract numberings come before the lists; the cleanup mark, if there is one, stays last
    let first_num = tree
        .root()
        .child_elements()
        .find(|c| c.local_name() == "num")
        .map(|c| c.id());
    let cleanup = tree
        .root()
        .child_elements()
        .find(|c| c.local_name() == "numIdMacAtCleanup")
        .map(|c| c.id());
    let mut ed = tree.edit();
    ed.insert_before(root, first_num.or(cleanup), a)?;
    ed.insert_before(root, cleanup, n)?;
    let part: std::sync::Arc<str> = std::sync::Arc::from(rev_part.as_str());
    for (node, src) in [(a, abs_src), (n, num_src)] {
        tx.declare(ImportedDef {
            part: std::sync::Arc::clone(&part),
            node: Some(node),
            source_part: std::sync::Arc::from(orig_part.as_str()),
            source_node: Some(src),
        });
    }
    imp.nums.rev.push((new_num.to_string(), def));
    imp.stats.lists_imported += 1;
    Ok(Some(new_num.to_string()))
}

fn strip_pstyle(s: &mut Snip) {
    s.kids
        .retain(|k| !matches!(k, Kid::El(e) if e.is_w("pStyle")));
    for k in &mut s.kids {
        if let Kid::El(e) = k {
            strip_pstyle(e);
        }
    }
}

fn strip_rsid(s: &mut Snip) {
    s.attrs.retain(|(_, l, _)| !l.starts_with("rsid"));
    s.kids
        .retain(|k| !matches!(k, Kid::El(e) if e.is_w("rsid")));
    for k in &mut s.kids {
        if let Kid::El(e) = k {
            strip_rsid(e);
        }
    }
}

/// The lists of two documents compared by definition: whether an original list is the revised document's list.
pub(in crate::compare) struct ListMatch {
    orig: HashMap<i64, String>,
    rev: HashMap<i64, String>,
}

/// The definition of each list of a document, by id: what the laws compare lists by.
pub(in crate::compare) fn list_definitions(
    state: &crate::state::State,
) -> Result<HashMap<i64, String>, StetError> {
    let view = state.parts();
    let specs = view.story_specs()?;
    let mut out = HashMap::new();
    let Some(body) = specs.0.first() else {
        return Ok(out);
    };
    let Some(name) = related(&view, &body.part, "/numbering")? else {
        return Ok(out);
    };
    let Some(id) = view.id(&name) else {
        return Ok(out);
    };
    let tree = view.tree(id)?;
    let mut abs: HashMap<String, NodeId> = HashMap::new();
    for c in tree.root().child_elements() {
        if c.local_name() == "abstractNum"
            && let Some(i) = attr(&c, "abstractNumId")
        {
            abs.insert(i, c.id());
        }
    }
    for c in tree.root().child_elements() {
        if c.local_name() == "num"
            && let Some(i) = attr(&c, "numId").and_then(|v| v.parse::<i64>().ok())
        {
            out.insert(i, definition(tree, &c, &abs));
        }
    }
    Ok(out)
}

impl ListMatch {
    /// The lists of both documents.
    pub(in crate::compare) fn new(
        orig: &crate::state::State,
        rev: &crate::state::State,
    ) -> Result<ListMatch, StetError> {
        Ok(ListMatch {
            orig: list_definitions(orig)?,
            rev: list_definitions(rev)?,
        })
    }

    /// True if list `a` of the original has the definition of list `b` of the revised document.
    pub(in crate::compare) fn same(&self, a: i64, b: i64) -> bool {
        if a == 0 || b == 0 {
            return a == b;
        }
        matches!((self.orig.get(&a), self.rev.get(&b)), (Some(x), Some(y)) if x == y)
    }
}

/// The revised document has no numbering part: an empty one is added, so that the lists of deleted content have somewhere to go.
fn add_numbering_part(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
) -> Result<Option<String>, StetError> {
    let Some(main) = tx
        .parts()
        .story_specs()?
        .0
        .first()
        .map(|s| std::sync::Arc::clone(&s.part))
    else {
        return Ok(None);
    };
    let name = "word/numbering.xml";
    if tx.parts().contains(name) {
        return Ok(None);
    }
    let bytes = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"{}\"/>",
        super::snip::W
    );
    tx.add_empty_part(crate::package::NewPart {
        name,
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
        bytes: bytes.into_bytes(),
        related_from: Some((
            &main,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering",
        )),
    })?;
    imp.nums.rev_part = Some(name.to_owned());
    imp.nums.next_abs = 0;
    imp.nums.next_num = 1;
    Ok(Some(name.to_owned()))
}
