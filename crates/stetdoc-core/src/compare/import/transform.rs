//! The rewrite of a snip on its way across: everything in it that names something of the source package is made to name the
//! same thing in the target, or is dropped (design `compare.md`, 7.2 to 7.6).

use super::snip::{Kid, R, Snip, W, W_STRICT};
use super::{Cx, Importer, markers, notes, numbering, parts, styles};
use crate::api::error::StetError;
use crate::revisions::import_writer::ImportWriter;
use crate::xml::SpanTree;

const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const VML: &str = "urn:schemas-microsoft-com:vml";
const VML_OFFICE: &str = "urn:schemas-microsoft-com:office:office";

/// The `w` namespace that the target tree uses.
pub(super) fn w_ns(tree: &SpanTree) -> String {
    let uri = tree.root().namespace();
    if uri == W || uri == W_STRICT {
        uri.to_owned()
    } else {
        W.to_owned()
    }
}

/// Elements that are dropped with everything in them.
const DROP: &[&str] = &[
    "proofErr",
    "lastRenderedPageBreak",
    "permStart",
    "permEnd",
    "commentRangeStart",
    "commentRangeEnd",
    "moveFromRangeStart",
    "moveFromRangeEnd",
    "moveToRangeStart",
    "moveToRangeEnd",
    "moveFrom",
    "rPrChange",
    "pPrChange",
    "sectPrChange",
    "tblPrChange",
    "trPrChange",
    "tcPrChange",
    "tblGridChange",
    "cellIns",
    "cellDel",
    "cellMerge",
    "sdtPr",
    "sdtEndPr",
    "annotationRef",
];

/// Elements that are replaced by what is in them.
const UNWRAP: &[&str] = &["moveTo", "sdt", "sdtContent", "customXml"];

/// The elements that hold a drawing: dropped whole when one of their relationships cannot be carried.
const OBJECTS: &[&str] = &["drawing", "pict", "object"];

struct T<'a, 'o, 'w, 's> {
    imp: &'a mut Importer<'o>,
    tx: &'a mut ImportWriter<'w, 's>,
    cx: Cx<'a>,
    /// Relationships that could not be carried so far.
    failed: usize,
}

/// Rewrites `snip` and returns what it becomes: nothing, itself, or several elements.
pub(super) fn run(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    cx: Cx<'_>,
    snip: Snip,
) -> Result<Vec<Kid>, StetError> {
    let mut starts = std::collections::HashSet::new();
    let mut ends = std::collections::HashSet::new();
    snip.walk(&mut |e| {
        if e.is_w("bookmarkStart")
            && let Some(i) = e.w_attr("id")
        {
            starts.insert(i.to_owned());
        }
        if e.is_w("bookmarkEnd")
            && let Some(i) = e.w_attr("id")
        {
            ends.insert(i.to_owned());
        }
    });
    imp.marks.source = cx.src.to_string();
    imp.marks.pairs = starts.intersection(&ends).cloned().collect();
    imp.marks.shapes.clear();
    let mut t = T {
        imp,
        tx,
        cx,
        failed: 0,
    };
    t.node(snip, "")
}

impl T<'_, '_, '_, '_> {
    fn node(&mut self, mut s: Snip, parent: &str) -> Result<Vec<Kid>, StetError> {
        let is_w = s.ns == W || s.ns == W_STRICT;
        // In a run's properties the alternate content is kept as it stands, as the run's formatting (Word writes a font for a
        // symbol so, with a `w16se` Choice and a Fallback).
        if s.ns == super::snip::MC
            && s.local == "AlternateContent"
            && parent != "r"
            && parent != "rPr"
        {
            // outside a run the branch that is read stands in its place: a deleted copy then carries the text the original
            // shows, in runs that a tracked change wraps, whatever the revised document's namespaces make of the branches
            let taken = self.taken_branch(&s);
            let mut out = Vec::new();
            if let Some(branch) = s.kids.into_iter().find_map(|k| match k {
                Kid::El(e) if Some(e.src) == taken => Some(e),
                _ => None,
            }) {
                for k in branch.kids {
                    match k {
                        Kid::El(e) => out.extend(self.node(e, parent)?),
                        t @ Kid::Text(..) => out.push(t),
                    }
                }
            }
            return Ok(out);
        }
        if (s.ns == super::snip::M || s.ns == super::snip::M_STRICT)
            && matches!(s.local.as_str(), "oMath" | "oMathPara")
        {
            // a tracked change cannot carry an equation (design 7.6): it is left out and counted
            self.imp.stats.math_dropped += 1;
            return Ok(Vec::new());
        }
        if is_w {
            let local = s.local.clone();
            let l = local.as_str();
            let mark_parent = matches!(parent, "rPr" | "trPr" | "pPr");
            if DROP.contains(&l) {
                if l.starts_with("comment") {
                    self.imp.stats.comments_dropped += 1;
                }
                return Ok(Vec::new());
            }
            match l {
                "del" => return Ok(Vec::new()),
                "ins" if mark_parent => return Ok(Vec::new()),
                "sectPr" if parent == "pPr" => return Ok(Vec::new()),
                "r" if s
                    .kids
                    .iter()
                    .any(|k| matches!(k, Kid::El(e) if e.is_w("commentReference"))) =>
                {
                    self.imp.stats.comments_dropped += 1;
                    return Ok(Vec::new());
                }
                "bookmarkStart" => return self.bookmark_start(s),
                "bookmarkEnd" => return self.bookmark_end(s),
                "fldSimple" => return self.field_simple(s),
                "footnoteReference" | "endnoteReference" => return self.note_reference(s),
                _ => {}
            }
            if l == "ins" || UNWRAP.contains(&l) {
                return self.kids_of(s, &local, true);
            }
        }
        let before = self.failed;
        self.attributes(&mut s, is_w)?;
        let local = s.local.clone();
        let hyperlink = is_w && local == "hyperlink";
        if hyperlink && self.failed > before && s.w_attr("anchor").is_none() {
            // a hyperlink whose address cannot be carried keeps its text and loses the link
            self.failed = before;
            self.imp.stats.hyperlinks_unlinked += 1;
            return self.kids_of(s, &local, true);
        }
        if hyperlink && self.failed > before {
            self.failed = before;
            self.imp.stats.hyperlinks_unlinked += 1;
        }
        if is_w {
            match local.as_str() {
                "pStyle" | "rStyle" | "tblStyle" => {
                    if let Some(v) = s.w_attr("val").map(str::to_owned) {
                        let mapped = styles::map(self.imp, self.tx, &v)?;
                        s.set_w_attr("val", &mapped);
                    }
                }
                "numId" => {
                    if let Some(v) = s.w_attr("val").map(str::to_owned) {
                        match numbering::map(self.imp, self.tx, &v)? {
                            Some(m) => s.set_w_attr("val", &m),
                            None => return Ok(Vec::new()),
                        }
                    }
                }
                _ => {}
            }
        }
        let mut kids = self.kids_of(s, &local, false)?;
        // the parts that are made by their children
        if let Some(Kid::El(e)) = kids.first_mut() {
            if e.is_w("numPr") && e.w_child("numId").is_none() {
                return Ok(Vec::new());
            }
            if is_w && OBJECTS.contains(&local.as_str()) && self.failed > before {
                self.failed = before;
                self.imp.stats.objects_dropped += 1;
                return Ok(Vec::new());
            }
        }
        Ok(kids)
    }

    /// The element with its children rewritten, or, with `unwrap`, the rewritten children alone.
    fn kids_of(&mut self, mut s: Snip, name: &str, unwrap: bool) -> Result<Vec<Kid>, StetError> {
        let kids = std::mem::take(&mut s.kids);
        let mut out = Vec::with_capacity(kids.len());
        for k in kids {
            match k {
                Kid::El(e) => {
                    // an `sdt`'s properties are dropped, its content is what is kept
                    out.extend(self.node(e, name)?);
                }
                Kid::Text(id, text) => {
                    if unwrap {
                        // text between elements of a wrapper is white space
                        if !text.trim().is_empty() {
                            out.push(Kid::Text(id, text));
                        }
                    } else {
                        out.push(Kid::Text(id, text));
                    }
                }
            }
        }
        if unwrap {
            return Ok(out);
        }
        // an empty property container says nothing, and Word's rules count one as a defect: a copy leaves it out
        if (s.ns == W || s.ns == W_STRICT)
            && matches!(s.local.as_str(), "rPr" | "pPr")
            && s.attrs.is_empty()
            && !out.iter().any(|k| matches!(k, Kid::El(_)))
        {
            return Ok(Vec::new());
        }
        s.kids = out;
        Ok(vec![Kid::El(s)])
    }

    /// Attributes that name something of the source: relationship ids, drawing ids, and the ids of revision sessions and paragraphs,
    /// which the target does not want.
    fn attributes(&mut self, s: &mut Snip, is_w: bool) -> Result<(), StetError> {
        let is_docpr = s.ns == WP && s.local == "docPr";
        let is_shape = s.ns == VML && s.local != "shapetype";
        let attrs = std::mem::take(&mut s.attrs);
        let mut kept = Vec::with_capacity(attrs.len());
        for (ns, l, v) in attrs {
            if ns == R {
                match parts::map(self.imp, self.tx, self.cx, &v)? {
                    Some(new) => kept.push((ns, l, new)),
                    None => self.failed += 1,
                }
            } else if ((ns == W || ns == W_STRICT) && l.starts_with("rsid"))
                || (ns == W14 && (l == "paraId" || l == "textId"))
            {
                // an id of a revision session or a paragraph: the target makes its own
            } else if is_shape
                && ((ns.is_empty() && l == "id") || (ns == VML_OFFICE && l == "spid"))
                && markers::shape_number(&v).is_some()
            {
                let id = markers::fresh_shape_id(self.imp, self.tx, &v)?;
                kept.push((ns, l, id));
            } else if is_docpr && ns.is_empty() && l == "id" {
                let id = markers::fresh_drawing_id(self.imp, self.tx)?;
                kept.push((ns, l, id.to_string()));
            } else {
                kept.push((ns, l, v));
            }
        }
        let _ = is_w;
        s.attrs = kept;
        Ok(())
    }

    /// The branch of an `mc:AlternateContent` of the original that is read, by its node in the original.
    fn taken_branch(&self, s: &Snip) -> Option<crate::xml::NodeId> {
        let view = self.imp.orig.parts();
        let tree = view.tree(view.id(self.cx.src)?).ok()?;
        crate::schema::taken_branch(&tree.node_ref(s.src)).map(|b| b.id())
    }

    fn bookmark_start(&mut self, s: Snip) -> Result<Vec<Kid>, StetError> {
        let (Some(id), Some(name)) = (s.w_attr("id"), s.w_attr("name")) else {
            return Ok(Vec::new());
        };
        match markers::start(self.imp, self.tx, self.cx, id, name)? {
            Some(new) => {
                let mut s = s;
                s.set_w_attr("id", &new.to_string());
                s.attrs
                    .retain(|(_, l, _)| l != "colFirst" && l != "colLast");
                Ok(vec![Kid::El(s)])
            }
            None => Ok(Vec::new()),
        }
    }

    fn bookmark_end(&mut self, mut s: Snip) -> Result<Vec<Kid>, StetError> {
        let Some(id) = s.w_attr("id") else {
            return Ok(Vec::new());
        };
        match markers::end(self.imp, id) {
            Some(new) => {
                s.set_w_attr("id", &new.to_string());
                Ok(vec![Kid::El(s)])
            }
            None => Ok(Vec::new()),
        }
    }

    fn note_reference(&mut self, mut s: Snip) -> Result<Vec<Kid>, StetError> {
        let endnote = s.local == "endnoteReference";
        let Some(id) = s.w_attr("id").and_then(|v| v.parse::<i64>().ok()) else {
            return Ok(Vec::new());
        };
        match notes::import(self.imp, self.tx, endnote, id)? {
            Some(new) => {
                s.set_w_attr("id", &new.to_string());
                Ok(vec![Kid::El(s)])
            }
            None => {
                self.imp.stats.notes_dropped += 1;
                Ok(Vec::new())
            }
        }
    }

    /// A `w:fldSimple` as the runs of a complex field: begin, code, separate, the result, end.
    fn field_simple(&mut self, s: Snip) -> Result<Vec<Kid>, StetError> {
        let instr = s.w_attr("instr").unwrap_or("").to_owned();
        let rpr = s.kids.iter().find_map(|k| match k {
            Kid::El(r) if r.is_w("r") => r.w_child("rPr").cloned(),
            _ => None,
        });
        let make = |children: Vec<Kid>| {
            let mut kids: Vec<Kid> = Vec::new();
            if let Some(p) = &rpr {
                kids.push(Kid::El(p.clone()));
            }
            kids.extend(children);
            Kid::El(Snip {
                src: s.src,
                ns: W.to_owned(),
                local: "r".to_owned(),
                attrs: Vec::new(),
                kids,
            })
        };
        let el = |local: &str, attrs: Vec<(String, String, String)>, kids: Vec<Kid>| {
            Kid::El(Snip {
                src: s.src,
                ns: W.to_owned(),
                local: local.to_owned(),
                attrs,
                kids,
            })
        };
        let fld = |kind: &str| {
            el(
                "fldChar",
                vec![(W.to_owned(), "fldCharType".to_owned(), kind.to_owned())],
                Vec::new(),
            )
        };
        let code = el(
            "instrText",
            vec![(
                crate::compare::import::snip::XML.to_owned(),
                "space".to_owned(),
                "preserve".to_owned(),
            )],
            vec![Kid::Text(s.src, instr)],
        );
        let mut out = vec![
            make(vec![fld("begin")]),
            make(vec![code]),
            make(vec![fld("separate")]),
        ];
        let inner = Snip {
            kids: s.kids.clone(),
            ..s.clone()
        };
        let mut body = self.kids_of(inner, "fldSimple", true)?;
        out.append(&mut body);
        out.push(make(vec![fld("end")]));
        Ok(out)
    }
}
