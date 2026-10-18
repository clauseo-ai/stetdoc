//! Styles of imported content (design `compare.md`, 7.3).
//!
//! The content that is copied names styles by id. An id that the revised document has with an equivalent definition maps to
//! itself. An id that it lacks is imported as it is, with the style it is based on. An id that it has with another definition is
//! imported under a new id (`Heading1` becomes `Heading1Original`) and its name gets ` (original)`, so Word shows two styles
//! and the copied content looks as it did. `next` and `link` are dropped, since a dangling one is a violation and neither is
//! needed for how the content looks.

use super::canon::canon;
use super::parts::related;
use super::snip::{Kid, Reader, Snip};
use super::{Importer, numbering};
use crate::api::error::StetError;
use crate::revisions::import_writer::{Detached, ImportWriter};
use crate::revisions::redline_types::ImportedDef;
use crate::xml::{NodeId, NodeRef};
use std::collections::{HashMap, HashSet};

/// What the importer remembers about styles.
#[derive(Default)]
pub(super) struct StyleState {
    ready: bool,
    orig_part: Option<String>,
    rev_part: Option<String>,
    orig: HashMap<String, NodeId>,
    rev: HashMap<String, (NodeId, String)>,
    map: HashMap<String, String>,
    busy: HashSet<String>,
    taken: HashSet<String>,
}

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn style_id(n: &NodeRef<'_>) -> Option<String> {
    n.attrs()
        .find(|a| a.local_name() == "styleId")
        .map(|a| a.value().into_owned())
}

fn child_val(n: &NodeRef<'_>, local: &str) -> Option<String> {
    n.child_elements()
        .find(|c| c.local_name() == local)
        .and_then(|c| {
            c.attrs()
                .find(|a| a.local_name() == "val")
                .map(|a| a.value().into_owned())
        })
}

/// The canonical form of a style without what does not change how content looks, and without `basedOn`, which is compared by
/// what it maps to.
fn style_canon(n: &NodeRef<'_>) -> String {
    let mut out = String::new();
    for c in n.child_elements() {
        if matches!(
            c.local_name(),
            "basedOn"
                | "next"
                | "link"
                | "rsid"
                | "uiPriority"
                | "qFormat"
                | "semiHidden"
                | "unhideWhenUsed"
                | "autoRedefine"
                | "locked"
                | "personal"
                | "personalCompose"
                | "personalReply"
        ) {
            continue;
        }
        out.push_str(&canon(&c, &[]));
    }
    out
}

fn load(imp: &mut Importer<'_>, tx: &ImportWriter<'_, '_>) -> Result<(), StetError> {
    imp.styles.ready = true;
    let (va, vb) = (imp.orig.parts(), tx.parts());
    let (sa, sb) = (va.story_specs()?, vb.story_specs()?);
    let (Some(ba), Some(bb)) = (sa.0.first(), sb.0.first()) else {
        return Ok(());
    };
    imp.styles.orig_part = related(&va, &ba.part, "/styles")?;
    imp.styles.rev_part = related(&vb, &bb.part, "/styles")?;
    if let Some(name) = &imp.styles.orig_part
        && let Some(id) = va.id(name)
    {
        for c in va.tree(id)?.root().child_elements() {
            if c.local_name() == "style"
                && let Some(i) = style_id(&c)
            {
                imp.styles.orig.insert(i, c.id());
            }
        }
    }
    if let Some(name) = &imp.styles.rev_part
        && let Some(id) = vb.id(name)
    {
        for c in vb.tree(id)?.root().child_elements() {
            if c.local_name() == "style"
                && let Some(i) = style_id(&c)
            {
                imp.styles.taken.insert(i.clone());
                imp.styles.rev.insert(i, (c.id(), style_canon(&c)));
            }
        }
    }
    Ok(())
}

/// The id in the revised document of the original's style `id`.
pub(super) fn map(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    id: &str,
) -> Result<String, StetError> {
    if !imp.styles.ready {
        load(imp, tx)?;
    }
    if let Some(done) = imp.styles.map.get(id) {
        return Ok(done.clone());
    }
    let Some(&node) = imp.styles.orig.get(id) else {
        return Ok(id.to_owned());
    };
    if imp.styles.rev_part.is_none() {
        add_styles_part(imp, tx)?;
    }
    if imp.styles.rev_part.is_none() {
        return Ok(id.to_owned());
    }
    if !imp.styles.busy.insert(id.to_owned()) {
        return Ok(id.to_owned());
    }
    let out = define(imp, tx, id, node);
    imp.styles.busy.remove(id);
    let out = out?;
    imp.styles.map.insert(id.to_owned(), out.clone());
    Ok(out)
}

fn define(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    id: &str,
    node: NodeId,
) -> Result<String, StetError> {
    let orig_part = imp.styles.orig_part.clone().unwrap_or_default();
    let rev_part = imp.styles.rev_part.clone().unwrap_or_default();
    let view = imp.orig.parts();
    let pid = view
        .id(&orig_part)
        .ok_or_else(|| super::internal("the original's styles part is gone"))?;
    let tree = view.tree(pid)?;
    let n = tree.node_ref(node);
    let based = child_val(&n, "basedOn");
    let canon_here = style_canon(&n);
    let name = child_val(&n, "name");
    // the list ids in the style, mapped, so that two styles that use the same list in two documents compare equal
    let mut reader = Reader::new(tree);
    let Some(mut snip) = reader.read(node) else {
        return Ok(id.to_owned());
    };
    let based_mapped = match &based {
        Some(b) => Some(map(imp, tx, b)?),
        None => None,
    };
    rewrite(imp, tx, &mut snip)?;
    // equivalence: same id there, same body, same base
    if let Some((_, rev_canon)) = imp.styles.rev.get(id).cloned() {
        let rev_base = {
            let v = tx.parts();
            let rid = v.id(&rev_part);
            match (rid, imp.styles.rev.get(id)) {
                (Some(rid), Some((rn, _))) => {
                    let t = v.tree(rid)?;
                    child_val(&t.node_ref(*rn), "basedOn")
                }
                _ => None,
            }
        };
        let mapped_canon = {
            let mut s = String::new();
            for k in &snip.kids {
                if let Kid::El(e) = k
                    && !matches!(
                        e.local.as_str(),
                        "basedOn"
                            | "next"
                            | "link"
                            | "rsid"
                            | "uiPriority"
                            | "qFormat"
                            | "semiHidden"
                            | "unhideWhenUsed"
                            | "autoRedefine"
                            | "locked"
                            | "personal"
                            | "personalCompose"
                            | "personalReply"
                    )
                {
                    s.push_str(&snip_canon(e));
                }
            }
            s
        };
        let _ = canon_here;
        if mapped_canon == rev_canon && rev_base == based_mapped {
            return Ok(id.to_owned());
        }
        // another definition under this id: import under a new one
        let mut new_id = format!("{id}Original");
        let mut i = 2usize;
        while imp.styles.taken.contains(&new_id) {
            new_id = format!("{id}Original{i}");
            i += 1;
        }
        snip.set_w_attr("styleId", &new_id);
        if let Some(nm) = snip.kids.iter_mut().find_map(|k| match k {
            Kid::El(e) if e.is_w("name") => Some(e),
            _ => None,
        }) {
            let old = nm
                .w_attr("val")
                .unwrap_or_else(|| name.as_deref().unwrap_or(id))
                .to_owned();
            nm.set_w_attr("val", &format!("{old} (original)"));
        }
        imp.stats.styles_renamed += 1;
        append(imp, tx, &rev_part, snip, based_mapped.as_deref(), &reader)?;
        imp.styles.taken.insert(new_id.clone());
        return Ok(new_id);
    }
    append(imp, tx, &rev_part, snip, based_mapped.as_deref(), &reader)?;
    imp.styles.taken.insert(id.to_owned());
    Ok(id.to_owned())
}

/// The canonical form of a snip, as `canon` makes it of a node.
fn snip_canon(s: &Snip) -> String {
    let mut out = String::new();
    out.push('<');
    out.push_str(&s.ns);
    out.push('|');
    out.push_str(&s.local);
    let mut attrs: Vec<&(String, String, String)> = s
        .attrs
        .iter()
        .filter(|(_, l, _)| !(l.starts_with("rsid") || l == "paraId" || l == "textId"))
        .collect();
    attrs.sort();
    for (ns, l, v) in attrs {
        out.push(' ');
        out.push_str(ns);
        out.push('|');
        out.push_str(l);
        out.push('=');
        out.push_str(v);
    }
    out.push('>');
    for k in &s.kids {
        match k {
            Kid::El(e) => out.push_str(&snip_canon(e)),
            Kid::Text(_, t) => {
                let t = t.trim();
                if !t.is_empty() {
                    out.push_str(t);
                }
            }
        }
    }
    out.push_str("</>");
    out
}

/// The list ids of a style, mapped, and `next` and `link` dropped.
fn rewrite(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    s: &mut Snip,
) -> Result<(), StetError> {
    s.kids
        .retain(|k| !matches!(k, Kid::El(e) if e.is_w("next") || e.is_w("link")));
    let mut ids: Vec<String> = Vec::new();
    s.walk(&mut |e| {
        if e.is_w("numId")
            && let Some(v) = e.w_attr("val")
        {
            ids.push(v.to_owned());
        }
    });
    let mut mapped: HashMap<String, Option<String>> = HashMap::new();
    for i in ids {
        let m = numbering::map(imp, tx, &i)?;
        mapped.insert(i, m);
    }
    fn go(s: &mut Snip, mapped: &HashMap<String, Option<String>>) {
        s.attrs.retain(|(_, l, _)| !l.starts_with("rsid"));
        s.kids
            .retain(|k| !matches!(k, Kid::El(e) if e.is_w("rsid")));
        let kids = std::mem::take(&mut s.kids);
        let mut out = Vec::with_capacity(kids.len());
        for k in kids {
            match k {
                Kid::El(mut e) => {
                    if e.is_w("numId") {
                        match e.w_attr("val").and_then(|v| mapped.get(v)).cloned() {
                            Some(Some(m)) => e.set_w_attr("val", &m),
                            Some(None) => continue,
                            None => {}
                        }
                    }
                    go(&mut e, mapped);
                    if e.is_w("numPr") && e.w_child("numId").is_none() {
                        continue;
                    }
                    out.push(Kid::El(e));
                }
                t => out.push(t),
            }
        }
        s.kids = out;
    }
    go(s, &mapped);
    Ok(())
}

/// Writes the style into the revised styles part, with `basedOn` as mapped.
fn append(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    part: &str,
    mut snip: Snip,
    based: Option<&str>,
    reader: &Reader<'_>,
) -> Result<(), StetError> {
    // a second default style of its kind is a violation
    snip.attrs.retain(|(_, l, _)| l != "default");
    if let Some(b) = based
        && let Some(e) = snip.kids.iter_mut().find_map(|k| match k {
            Kid::El(e) if e.is_w("basedOn") => Some(e),
            _ => None,
        })
    {
        e.set_w_attr("val", b);
    }
    let src = snip.src;
    let tree = tx.part_mut(part)?;
    let node = {
        let mut d = Detached::over(tree);
        super::snip::bind_used(&mut d, &reader.used)?;
        let w = super::transform::w_ns(d.tree());
        super::snip::write(&mut d, &snip, &w, &reader.requires)?
    };
    let root = tree.root().id();
    tree.edit().append_child(root, node)?;
    tx.declare(ImportedDef {
        part: std::sync::Arc::from(part),
        node: Some(node),
        source_part: std::sync::Arc::from(imp.styles.orig_part.as_deref().unwrap_or_default()),
        source_node: Some(src),
    });
    imp.stats.styles_imported += 1;
    let _ = W;
    Ok(())
}

/// The revised document has no styles part: an empty one is added, so that the styles of deleted content have somewhere to go.
fn add_styles_part(imp: &mut Importer<'_>, tx: &mut ImportWriter<'_, '_>) -> Result<(), StetError> {
    let Some(main) = tx
        .parts()
        .story_specs()?
        .0
        .first()
        .map(|s| std::sync::Arc::clone(&s.part))
    else {
        return Ok(());
    };
    let name = "word/styles.xml";
    if tx.parts().contains(name) {
        return Ok(());
    }
    let bytes = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"{W}\"/>"
    );
    tx.add_empty_part(crate::package::NewPart {
        name,
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
        bytes: bytes.into_bytes(),
        related_from: Some((
            &main,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles",
        )),
    })?;
    imp.styles.rev_part = Some(name.to_owned());
    Ok(())
}
