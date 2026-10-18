//! The properties of the original that a change records as its snapshot (design `compare.md`, 5.4 and 6.1): the paragraph's
//! properties, its mark's, and the run properties of a stretch of text. They are read from the original and written as
//! [`PropElement`]s, with style ids and list ids mapped to the revised document's.

use super::{Cx, Importer, numbering, styles};
use crate::api::error::StetError;
use crate::revisions::import_writer::ImportWriter;
use crate::revisions::types::PropElement;
use crate::wordml::{Mode, NodeId, Paragraph};
use crate::xml::NodeRef;

/// Children of `pPr` that are not properties of the paragraph.
const NOT_PARAGRAPH: &[&str] = &["rPr", "sectPr", "pPrChange"];
/// Children of the mark's `rPr` that are revisions.
const NOT_MARK: &[&str] = &["ins", "del", "moveFrom", "moveTo", "rPrChange"];

impl Importer<'_> {
    /// The paragraph properties of the original paragraph `para`, and its mark's properties.
    pub(in crate::compare) fn paragraph_snapshot(
        &mut self,
        tx: &mut ImportWriter<'_, '_>,
        cx: Cx<'_>,
        para: NodeId,
    ) -> Result<(Vec<PropElement>, Vec<PropElement>), StetError> {
        let view = self.orig.parts();
        let id = view
            .id(cx.src)
            .ok_or_else(|| super::internal("the original has no such part"))?;
        let tree = view.tree(id)?;
        let p = tree.node_ref(para);
        let Some(ppr) = p.child_elements().find(|c| c.local_name() == "pPr") else {
            return Ok((Vec::new(), Vec::new()));
        };
        let mut own = Vec::new();
        let mut mark = Vec::new();
        for c in ppr.child_elements() {
            if c.local_name() == "rPr" {
                for m in c.child_elements() {
                    if !NOT_MARK.contains(&m.local_name())
                        && let Some(e) = self.convert(tx, &m)?
                    {
                        mark.push(e);
                    }
                }
            } else if !NOT_PARAGRAPH.contains(&c.local_name())
                && let Some(e) = self.convert(tx, &c)?
            {
                own.push(e);
            }
        }
        Ok((own, mark))
    }

    /// The run properties of the run that holds the character at `at` of the original paragraph's `Final` text.
    pub(in crate::compare) fn run_snapshot(
        &mut self,
        tx: &mut ImportWriter<'_, '_>,
        cx: Cx<'_>,
        para: NodeId,
        at: usize,
    ) -> Result<Vec<PropElement>, StetError> {
        let view = self.orig.parts();
        let id = view
            .id(cx.src)
            .ok_or_else(|| super::internal("the original has no such part"))?;
        let tree = view.tree(id)?;
        let p = Paragraph::new(tree, para)
            .ok_or_else(|| super::internal("an original block is not a paragraph"))?;
        let map = p.map();
        let mut pos = 0usize;
        let mut found = None;
        for item in map.items() {
            if !item.vis.in_mode(Mode::Final) || item.len == 0 {
                continue;
            }
            let end = pos + item.len as usize;
            if at >= pos && at < end {
                found = Some(*item);
                break;
            }
            pos = end;
        }
        let Some(item) = found else {
            return Ok(Vec::new());
        };
        let Some(props) = p.run_props(&item) else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        for c in props.iter() {
            if !NOT_MARK.contains(&c.local_name())
                && let Some(e) = self.convert(tx, &c)?
            {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// An element of a properties container as data, with style and list ids mapped. `None` when it has to go (a list the revised
    /// document has no numbering for).
    fn convert(
        &mut self,
        tx: &mut ImportWriter<'_, '_>,
        n: &NodeRef<'_>,
    ) -> Result<Option<PropElement>, StetError> {
        let mut attrs: Vec<(String, String, String)> = Vec::new();
        for a in n.attrs() {
            if a.is_namespace_declaration() || a.local_name().starts_with("rsid") {
                continue;
            }
            attrs.push((
                a.namespace().to_owned(),
                a.local_name().to_owned(),
                a.value().into_owned(),
            ));
        }
        let local = n.local_name();
        if matches!(local, "pStyle" | "rStyle" | "tblStyle")
            && let Some(v) = attrs.iter_mut().find(|(_, l, _)| l == "val")
        {
            v.2 = styles::map(self, tx, &v.2)?;
        }
        if local == "numId"
            && let Some(v) = attrs.iter_mut().find(|(_, l, _)| l == "val")
        {
            match numbering::map(self, tx, &v.2)? {
                Some(m) => v.2 = m,
                None => return Ok(None),
            }
        }
        let mut children = Vec::new();
        for c in n.child_elements() {
            if let Some(e) = self.convert(tx, &c)? {
                children.push(e);
            }
        }
        if local == "numPr" && !children.iter().any(|c| c.local == "numId") {
            return Ok(None);
        }
        Ok(Some(PropElement {
            ns: n.namespace().to_owned(),
            local: local.to_owned(),
            attrs,
            children,
        }))
    }
}
