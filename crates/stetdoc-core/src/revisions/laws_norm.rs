//! `normalize`: the form in which two documents are compared (design `revisions.md`, 7.1). It is written
//! once, here, and the laws call it. It is a function of one part's XML, and it shares no code with the
//! compiler or the resolver.
//!
//! The rules: drop `w:rsid*`, `w14:paraId`, `w14:textId`, `w:proofErr`, `w:lastRenderedPageBreak` and a
//! bookmark named `_GoBack`; merge adjacent runs with equal properties (an `rPrChange` equal up to its id)
//! and adjacent insertions, deletions or moves of one side with equal author and date; unwrap a wrapper with no child; drop an
//! empty `w:rPr`, `w:pPr`, `w:numPr`, `w:trPr`, `w:tcPr`, wrapper and hyperlink or smart tag, and `xml:space`; drop a property change whose snapshot equals the
//! properties it sits in; renumber the revision ids by document order; sort attributes and drop white space
//! between elements. The order of elements is left alone, except among the properties of a container.

use crate::xml::{NodeRef, SpanTree};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";

/// True for the `w` namespace of either dialect.
fn is_word(ns: &str) -> bool {
    ns == W || ns == W_STRICT
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum N {
    El {
        ns: String,
        local: String,
        attrs: Vec<(String, String, String)>,
        kids: Vec<N>,
    },
    Text(String),
}

/// A bookmark that is not part of the content: Word's `_GoBack`, and the ones the laws place to exercise
/// `place_markers`, which writes no revision and so cannot be rejected.
fn is_noise_bookmark(name: &str) -> bool {
    name == "_GoBack" || name.starts_with("lawbm")
}

fn is(n: &N, local: &str) -> bool {
    matches!(n, N::El { ns, local: l, .. } if is_word(ns) && l == local)
}

/// The containers that group runs and nothing else.
fn is_shell(n: &N) -> bool {
    ["hyperlink", "smartTag", "customXml"]
        .iter()
        .any(|l| is(n, l))
}

fn same_attrs(a: &N, b: &N) -> bool {
    matches!((a, b), (N::El { attrs: x, .. }, N::El { attrs: y, .. }) if x == y)
}

fn local_of(n: &N) -> &str {
    match n {
        N::El { local, .. } => local,
        N::Text(_) => "",
    }
}

/// Text nodes of a `w:t` are kept even if they are white space only.
fn load(n: &NodeRef<'_>) -> N {
    let l = n.local_name();
    if matches!(l, "t" | "delText" | "instrText" | "delInstrText") {
        return N::El {
            ns: n.namespace().to_owned(),
            local: l.to_owned(),
            attrs: Vec::new(),
            kids: vec![N::Text(n.text_content())],
        };
    }
    let mut attrs: Vec<(String, String, String)> = n
        .attrs()
        .filter(|a| !a.is_namespace_declaration())
        .map(|a| {
            (
                a.namespace().to_owned(),
                a.local_name().to_owned(),
                a.value().into_owned(),
            )
        })
        .collect();
    attrs.sort();
    N::El {
        ns: n.namespace().to_owned(),
        local: l.to_owned(),
        attrs,
        kids: n.child_elements().map(|c| load(&c)).collect(),
    }
}

/// The namespaces in which Word writes `paraId`, `textId` and `editId`: the 2010 one, and the ones of the
/// 2007 previews, which some documents still have.
fn is_word_ids_namespace(ns: &str) -> bool {
    ns.starts_with("http://schemas.microsoft.com/office/word/") && ns.ends_with("/wordml")
}

fn drop_attr(attrs: &mut Vec<(String, String, String)>) {
    attrs.retain(|(ns, local, _)| {
        !(local.starts_with("rsid") && is_word(ns)
            || matches!(local.as_str(), "paraId" | "textId" | "editId")
                && is_word_ids_namespace(ns)
            || local == "space" && ns == "http://www.w3.org/XML/1998/namespace")
    });
}

fn is_rev_name(local: &str) -> bool {
    matches!(
        local,
        "ins"
            | "del"
            | "moveFrom"
            | "moveTo"
            | "rPrChange"
            | "pPrChange"
            | "sectPrChange"
            | "tblPrChange"
            | "tblPrExChange"
            | "tblGridChange"
            | "trPrChange"
            | "tcPrChange"
            | "numberingChange"
            | "cellIns"
            | "cellDel"
            | "cellMerge"
    )
}

fn without_id(n: &N) -> N {
    match n {
        N::El {
            ns,
            local,
            attrs,
            kids,
        } => N::El {
            ns: ns.clone(),
            local: local.clone(),
            attrs: attrs
                .iter()
                .filter(|(ans, al, _)| !(al == "id" && is_word(ans) && is_rev_name(local)))
                .cloned()
                .collect(),
            kids: kids.iter().map(without_id).collect(),
        },
        t => t.clone(),
    }
}

fn attr<'a>(n: &'a N, local: &str) -> Option<&'a str> {
    match n {
        N::El { attrs, .. } => attrs
            .iter()
            .find(|(_, l, _)| l == local)
            .map(|(_, _, v)| v.as_str()),
        N::Text(_) => None,
    }
}

fn kids_of(n: &mut N) -> Option<&mut Vec<N>> {
    match n {
        N::El { kids, .. } => Some(kids),
        N::Text(_) => None,
    }
}

/// The run properties of a run, with any `rPrChange` taken as equal up to its id.
fn run_props(run: &N) -> Option<N> {
    let N::El { kids, .. } = run else { return None };
    kids.iter().find(|k| is(k, "rPr")).map(without_id)
}

/// The run properties of a run as they are, with the ids of a change: the first of two runs that merge
/// keeps them, so that the ids are renumbered the same as in a document that never had the two.
fn run_own_props(run: &N) -> Option<N> {
    let N::El { kids, .. } = run else { return None };
    kids.iter().find(|k| is(k, "rPr")).cloned()
}

fn run_body(run: &N) -> Vec<N> {
    let N::El { kids, .. } = run else {
        return Vec::new();
    };
    kids.iter().filter(|k| !is(k, "rPr")).cloned().collect()
}

/// Appends the content of `b` to `a`, joining a text element with the text element before it.
fn append_body(a: &mut Vec<N>, b: Vec<N>) {
    for k in b {
        let join = match (a.last_mut(), &k) {
            (
                Some(N::El {
                    ns, local, kids, ..
                }),
                N::El {
                    ns: ns2,
                    local: l2,
                    kids: k2,
                    ..
                },
            ) if ns == ns2
                && local == l2
                && matches!(
                    local.as_str(),
                    "t" | "delText" | "instrText" | "delInstrText"
                ) =>
            {
                if let (Some(N::Text(x)), Some(N::Text(y))) = (kids.last_mut(), k2.first()) {
                    x.push_str(y);
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if !join {
            a.push(k);
        }
    }
}

fn clean(n: &mut N, bookmarks_gone: &[String]) {
    let N::El {
        ns,
        local,
        attrs,
        kids,
    } = n
    else {
        return;
    };
    drop_attr(attrs);
    for k in kids.iter_mut() {
        clean(k, bookmarks_gone);
    }
    // Elements that are dropped.
    kids.retain(|k| {
        !(is(k, "proofErr")
            || is(k, "lastRenderedPageBreak")
            || (is(k, "bookmarkStart") && attr(k, "name").is_some_and(is_noise_bookmark))
            || (is(k, "bookmarkEnd")
                && attr(k, "id").is_some_and(|id| bookmarks_gone.iter().any(|g| g == id))))
    });
    // The order of the properties in a container is the schema's, which law R5 checks, so two containers
    // with the same properties are the same here, which a document that Word did not write may not have
    // in the order that the schema gives (a `tblLook` before the borders). Equal names keep their order.
    if is_word(ns)
        && matches!(
            local.as_str(),
            "pPr" | "rPr" | "sectPr" | "tblPr" | "trPr" | "tcPr" | "tblPrEx"
        )
    {
        kids.sort_by(|a, b| local_of(a).cmp(local_of(b)));
    }
    // A run properties container with a change whose snapshot equals what it holds is clean.
    if is_word(ns)
        && matches!(
            local.as_str(),
            "pPr" | "rPr" | "sectPr" | "tblPr" | "trPr" | "tcPr" | "tblPrEx"
        )
    {
        let change = format!("{local}Change");
        if let Some(pos) = kids.iter().position(|k| is(k, &change)) {
            let snap: Vec<N> = match &kids[pos] {
                N::El { kids: sk, .. } => sk.iter().find(|k| local_of(k) == local.as_str()).map_or(
                    Vec::new(),
                    |s| match s {
                        N::El { kids, .. } => kids.clone(),
                        N::Text(_) => Vec::new(),
                    },
                ),
                N::Text(_) => Vec::new(),
            };
            let skip = |k: &N| {
                is_rev_name(local_of(k))
                    || (local == "pPr" && matches!(local_of(k), "rPr" | "sectPr"))
                    || (local == "sectPr"
                        && matches!(local_of(k), "headerReference" | "footerReference"))
            };
            let mut held: Vec<String> = kids
                .iter()
                .filter(|k| !skip(k))
                .map(|k| format!("{k:?}"))
                .collect();
            let mut old: Vec<String> = snap
                .iter()
                .filter(|k| !is_rev_name(local_of(k)))
                .map(|k| format!("{k:?}"))
                .collect();
            held.sort();
            old.sort();
            if held == old {
                kids.remove(pos);
            }
        }
    }
    // Adjacent runs and adjacent revision wrappers merge.
    let mut out: Vec<N> = Vec::new();
    for k in std::mem::take(kids) {
        let merged = match (out.last_mut(), &k) {
            (Some(prev), cur)
                if is(prev, "r") && is(cur, "r") && run_props(prev) == run_props(cur) =>
            {
                let body = run_body(cur);
                let mut pb = run_body(prev);
                append_body(&mut pb, body);
                let props = run_own_props(prev);
                if let Some(pk) = kids_of(prev) {
                    pk.clear();
                    if let Some(p) = props {
                        pk.push(p);
                    }
                    pk.extend(pb);
                }
                true
            }
            (Some(prev), cur)
                if ["ins", "del", "moveFrom", "moveTo"]
                    .iter()
                    .any(|l| is(prev, l) && is(cur, l))
                    && attr(prev, "author") == attr(cur, "author")
                    && attr(prev, "date") == attr(cur, "date") =>
            {
                if let (Some(pk), N::El { kids: ck, .. }) = (kids_of(prev), cur) {
                    pk.extend(ck.iter().cloned());
                }
                true
            }
            // Two halves of a hyperlink or a smart tag with the same attributes are one.
            (Some(prev), cur)
                if is_shell(prev) && local_of(prev) == local_of(cur) && same_attrs(prev, cur) =>
            {
                if let (Some(pk), N::El { kids: ck, .. }) = (kids_of(prev), cur) {
                    pk.extend(ck.iter().cloned());
                }
                true
            }
            _ => false,
        };
        if !merged {
            out.push(k);
        }
    }
    *kids = out;
    // Merging wrappers may have made new adjacent runs inside them.
    for k in kids.iter_mut() {
        if is(k, "ins") || is(k, "del") || is(k, "moveFrom") || is(k, "moveTo") || is_shell(k) {
            let mut tmp = std::mem::replace(k, N::Text(String::new()));
            reclean_runs(&mut tmp);
            *k = tmp;
        }
    }
    // Empty containers and wrappers go.
    kids.retain(|k| {
        let empty = matches!(k, N::El { kids, .. } if kids.is_empty());
        // An empty hyperlink or smart tag holds nothing and is dropped as the resolver drops it.
        !(empty
            && (is(k, "rPr")
                || is(k, "pPr")
                || is(k, "numPr")
                || is(k, "trPr")
                || is(k, "tcPr")
                || is(k, "ins")
                || is(k, "del")
                || is(k, "moveFrom")
                || is(k, "moveTo")
                || is_shell(k)))
    });
}

fn reclean_runs(n: &mut N) {
    let N::El { kids, .. } = n else { return };
    let mut out: Vec<N> = Vec::new();
    for k in std::mem::take(kids) {
        let merged = match (out.last_mut(), &k) {
            (Some(prev), cur)
                if is(prev, "r") && is(cur, "r") && run_props(prev) == run_props(cur) =>
            {
                let mut pb = run_body(prev);
                append_body(&mut pb, run_body(cur));
                let props = run_own_props(prev);
                if let Some(pk) = kids_of(prev) {
                    pk.clear();
                    if let Some(p) = props {
                        pk.push(p);
                    }
                    pk.extend(pb);
                }
                true
            }
            _ => false,
        };
        if !merged {
            out.push(k);
        }
    }
    *kids = out;
}

/// Renumbers the revision ids by order of appearance.
fn renumber(n: &mut N, next: &mut u32) {
    let N::El {
        local,
        attrs,
        kids,
        ns,
    } = n
    else {
        return;
    };
    if is_word(ns) && is_rev_name(local) {
        for (ans, al, v) in attrs.iter_mut() {
            if al == "id" && is_word(ans) {
                *next += 1;
                *v = next.to_string();
            }
        }
    }
    for k in kids.iter_mut() {
        renumber(k, next);
    }
}

fn short(ns: &str) -> String {
    if is_word(ns) {
        "w:".to_owned()
    } else if ns.is_empty() {
        String::new()
    } else {
        format!("{{{ns}}}")
    }
}

/// Removes the `w:pPr` of the last paragraph that is a child of the body.
fn drop_last_paragraph_properties(root: &mut N) {
    let N::El { kids, .. } = root else { return };
    let Some(N::El { kids: body, .. }) = kids.iter_mut().find(|k| is(k, "body")) else {
        return;
    };
    if let Some(N::El { kids: para, .. }) = body.iter_mut().rev().find(|k| is(k, "p")) {
        para.retain(|k| !is(k, "pPr"));
    }
}

fn write(n: &N, out: &mut String) {
    match n {
        N::Text(t) => out.push_str(t),
        N::El {
            ns,
            local,
            attrs,
            kids,
        } => {
            out.push('<');
            out.push_str(&format!("{}{local}", short(ns)));
            for (ans, al, v) in attrs {
                out.push_str(&format!(" {}{al}=\"{v}\"", short(ans)));
            }
            out.push('>');
            for k in kids {
                write(k, out);
            }
            out.push_str("</>");
        }
    }
}

/// The comparison form of a part.
pub(super) fn normalize(tree: &SpanTree) -> String {
    normalize_with(tree, false)
}

/// The comparison form of a part with the properties of the last paragraph of the body left out. They are
/// left out for a document that ends in a paragraph whose mark is an insertion: rejecting the insertion
/// cannot remove the last mark, and which paragraph's properties the text before it keeps is a convention
/// of the resolver that the laws do not fix.
pub(super) fn normalize_without_last_ppr(tree: &SpanTree) -> String {
    normalize_with(tree, true)
}

fn normalize_with(tree: &SpanTree, drop_last_ppr: bool) -> String {
    let mut root = load(&tree.root());
    // The ids of the `_GoBack` bookmarks, whose ends go too.
    let mut gone = Vec::new();
    let mut stack = vec![&root];
    while let Some(n) = stack.pop() {
        if is(n, "bookmarkStart")
            && attr(n, "name").is_some_and(is_noise_bookmark)
            && let Some(id) = attr(n, "id")
        {
            gone.push(id.to_owned());
        }
        if let N::El { kids, .. } = n {
            stack.extend(kids.iter());
        }
    }
    clean(&mut root, &gone);
    if drop_last_ppr {
        drop_last_paragraph_properties(&mut root);
    }
    let mut next = 0;
    renumber(&mut root, &mut next);
    let mut s = String::new();
    write(&root, &mut s);
    s
}
