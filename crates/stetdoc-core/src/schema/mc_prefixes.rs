//! Prefixes that a markup-compatibility attribute names and that no declaration binds.
//!
//! `mc:Ignorable` and `mc:MustUnderstand` list prefixes, and `mc:ProcessContent` lists qualified names. ECMA-376 Part 3
//! requires each prefix to be declared in scope at the element that carries the attribute. The Open XML SDK reports one
//! that is not as `MC_InvalidIgnorableAttribute`, and Word for Mac and for Windows refuse the file ("Word found unreadable
//! content"). The schema allows the attribute and its value, so validation does not catch it: this file finds them, and
//! knows the namespace Word writes for a prefix, so that a writer can declare it (decision 0014).

use super::tables;
use super::tree::SchemaTree;
use super::validate::MC_NS;

/// The markup-compatibility attributes whose values name prefixes, by local name.
const ATTRIBUTES: [&str; 3] = ["Ignorable", "ProcessContent", "MustUnderstand"];

/// The namespaces Word declares on the root of the parts it writes, under the prefixes it declares them with. A prefix
/// here wins over the schema tables, whose preferred prefix for a namespace can differ from Word's: they call the
/// namespace of Word 2018 `w16cur` where Word writes `w16`, and number the chart extension namespaces from 2016 one lower
/// than Word does. Taken from [MS-DOCX] and [MS-ODRAWXML] and checked against the roots of the Word-authored documents of
/// the curated corpus.
const WORD_PREFIXES: [(&str, &str); 40] = [
    ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    (
        "a14",
        "http://schemas.microsoft.com/office/drawing/2010/main",
    ),
    (
        "aink",
        "http://schemas.microsoft.com/office/drawing/2016/ink",
    ),
    (
        "am3d",
        "http://schemas.microsoft.com/office/drawing/2017/model3d",
    ),
    (
        "cx",
        "http://schemas.microsoft.com/office/drawing/2014/chartex",
    ),
    (
        "cx1",
        "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex",
    ),
    (
        "cx2",
        "http://schemas.microsoft.com/office/drawing/2015/10/21/chartex",
    ),
    (
        "cx3",
        "http://schemas.microsoft.com/office/drawing/2016/5/9/chartex",
    ),
    (
        "cx4",
        "http://schemas.microsoft.com/office/drawing/2016/5/10/chartex",
    ),
    (
        "cx5",
        "http://schemas.microsoft.com/office/drawing/2016/5/11/chartex",
    ),
    (
        "cx6",
        "http://schemas.microsoft.com/office/drawing/2016/5/12/chartex",
    ),
    (
        "cx7",
        "http://schemas.microsoft.com/office/drawing/2016/5/13/chartex",
    ),
    (
        "cx8",
        "http://schemas.microsoft.com/office/drawing/2016/5/14/chartex",
    ),
    (
        "m",
        "http://schemas.openxmlformats.org/officeDocument/2006/math",
    ),
    (
        "mc",
        "http://schemas.openxmlformats.org/markup-compatibility/2006",
    ),
    ("o", "urn:schemas-microsoft-com:office:office"),
    ("oel", "http://schemas.microsoft.com/office/2019/extlst"),
    (
        "pic",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ),
    (
        "r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ),
    ("v", "urn:schemas-microsoft-com:vml"),
    (
        "w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ),
    ("w10", "urn:schemas-microsoft-com:office:word"),
    (
        "w14",
        "http://schemas.microsoft.com/office/word/2010/wordml",
    ),
    (
        "w15",
        "http://schemas.microsoft.com/office/word/2012/wordml",
    ),
    (
        "w16",
        "http://schemas.microsoft.com/office/word/2018/wordml",
    ),
    (
        "w16cex",
        "http://schemas.microsoft.com/office/word/2018/wordml/cex",
    ),
    (
        "w16cid",
        "http://schemas.microsoft.com/office/word/2016/wordml/cid",
    ),
    (
        "w16du",
        "http://schemas.microsoft.com/office/word/2023/wordml/word16du",
    ),
    (
        "w16sdtdh",
        "http://schemas.microsoft.com/office/word/2020/wordml/sdtdatahash",
    ),
    (
        "w16sdtfl",
        "http://schemas.microsoft.com/office/word/2024/wordml/sdtformatlock",
    ),
    (
        "w16se",
        "http://schemas.microsoft.com/office/word/2015/wordml/symex",
    ),
    (
        "wne",
        "http://schemas.microsoft.com/office/word/2006/wordml",
    ),
    (
        "wp",
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    ),
    (
        "wp14",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing",
    ),
    (
        "wp15",
        "http://schemas.microsoft.com/office/word/2012/wordprocessingDrawing",
    ),
    (
        "wpc",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas",
    ),
    (
        "wpg",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup",
    ),
    (
        "wpi",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingInk",
    ),
    (
        "wps",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
    ),
    (
        "wpsCustomData",
        "http://www.wps.cn/officeDocument/2013/wpsCustomData",
    ),
];

/// One element whose markup-compatibility attributes name prefixes that are not declared at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Undeclared<N> {
    /// The element that carries the attributes.
    pub node: N,
    /// Each attribute (by local name: `Ignorable`, `ProcessContent` or `MustUnderstand`) and the undeclared prefix it
    /// names, each pair once, in the order of the attributes and of their tokens.
    pub names: Vec<(&'static str, String)>,
}

impl<N> Undeclared<N> {
    /// The undeclared prefixes, each once, in order.
    pub fn prefixes(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for (_, p) in &self.names {
            if !out.contains(&p.as_str()) {
                out.push(p);
            }
        }
        out
    }
}

/// The prefix that one token of a markup-compatibility attribute names: the token itself for `Ignorable` and
/// `MustUnderstand`, and what is before the colon for `ProcessContent`, whose tokens are qualified names. `None` for a
/// name of `ProcessContent` that has no prefix, which names no namespace to declare.
pub fn token_prefix<'t>(attribute: &str, token: &'t str) -> Option<&'t str> {
    if attribute == "ProcessContent" {
        token.split_once(':').map(|(p, _)| p)
    } else {
        Some(token)
    }
}

/// Every element of the tree whose `mc:Ignorable`, `mc:ProcessContent` or `mc:MustUnderstand` names a prefix that no
/// declaration in scope at the element binds, in document order.
pub fn undeclared_prefixes<T: SchemaTree>(tree: &T) -> Vec<Undeclared<T::Node>> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root()];
    while let Some(node) = stack.pop() {
        if tree.has_attribute_in(node, MC_NS) {
            let mut names: Vec<(&'static str, String)> = Vec::new();
            for a in tree.attributes(node) {
                let Some(attribute) = ATTRIBUTES
                    .iter()
                    .find(|n| a.namespace == MC_NS && **n == a.local)
                else {
                    continue;
                };
                for token in a.value.split_whitespace() {
                    let Some(prefix) = token_prefix(attribute, token) else {
                        continue;
                    };
                    if prefix.is_empty()
                        || tree.resolve_prefix(node, prefix).is_some()
                        || names.iter().any(|(n, p)| n == attribute && p == prefix)
                    {
                        continue;
                    }
                    names.push((attribute, prefix.to_owned()));
                }
            }
            if !names.is_empty() {
                out.push(Undeclared { node, names });
            }
        }
        stack.extend(tree.children(node).into_iter().rev());
    }
    out
}

/// The namespace a writer declares for an undeclared prefix: the one Word writes under it, else the namespace whose
/// preferred prefix in the schema tables it is. `None` for a prefix neither knows, which has no namespace to declare.
pub fn prefix_namespace(prefix: &str) -> Option<&'static str> {
    WORD_PREFIXES
        .iter()
        .find(|(p, _)| *p == prefix)
        .map(|(_, uri)| *uri)
        .or_else(|| tables::ns_by_prefix(prefix))
}
