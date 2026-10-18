//! Property containers read into a generic shape: `Props` and `PropsRef`.
//!
//! `w:pPr`, `w:rPr`, `w:tblPr`, `w:trPr`, `w:tcPr` and `w:sectPr` hold a mix of vocabularies, so
//! every name keeps its namespace. The `w` namespace of either dialect is `Ns::Word`, and the reads
//! `get`, `on_off` and `val` look at it only, so `w14:shadow` never answers `shadow`.
//!
//! Two forms give the same answers. `Props` is owned, with no borrow of the span tree, for the
//! definitions (a style's formatting). `PropsRef` reads straight over a `NodeRef` and allocates
//! nothing: it is one `NodeRef` (a reference and an id, `Copy`), every method returns a borrowed
//! value or an iterator over the tree, and attribute values are `Cow::Borrowed` unless the source
//! wrote a character reference in them. Direct formatting in the document is read through it,
//! because hidden-text detection visits every run.

use super::names::{is_word_ns, on_off as st_on_off, w_attr};
use crate::xml::NodeRef;
use std::borrow::Cow;

/// How many levels below the container are read. The container's children are level one, so `w:pBdr`
/// is level one and its `w:top` is level two. Four covers the mark's `w:rPr` inside `w:pPr`, and the
/// old-property snapshots inside `w:rPrChange` and `w:pPrChange`.
const DEPTH: usize = 4;

/// The namespace of a property or attribute name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ns {
    /// No namespace: an unprefixed attribute.
    None,
    /// The `w` namespace of either dialect, Transitional or Strict.
    Word,
    /// Any other namespace URI, such as `w14` or `w15`.
    Ext(Box<str>),
}

impl Ns {
    /// The namespace for a URI as the span tree reports it (empty for none).
    pub fn from_uri(uri: &str) -> Ns {
        if uri.is_empty() {
            Ns::None
        } else if is_word_ns(uri) {
            Ns::Word
        } else {
            Ns::Ext(uri.into())
        }
    }

    /// True if `uri` is in this namespace. `Ns::Word` matches both dialects.
    pub fn matches_uri(&self, uri: &str) -> bool {
        match self {
            Ns::None => uri.is_empty(),
            Ns::Word => is_word_ns(uri),
            Ns::Ext(u) => &**u == uri,
        }
    }
}

/// One attribute of a property element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    /// The attribute's namespace.
    pub ns: Ns,
    /// The local name, such as `val`.
    pub name: Box<str>,
    /// The decoded value.
    pub value: Box<str>,
}

/// One property element: its name, attributes and the property elements inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prop {
    /// The element's namespace.
    pub ns: Ns,
    /// The local name, such as `b` or `pBdr`.
    pub name: Box<str>,
    /// The attributes in the order written, without namespace declarations.
    pub attrs: Vec<Attr>,
    /// The child elements, in document order, to the depth `Props` reads.
    pub children: Vec<Prop>,
}

impl Prop {
    /// The value of the attribute `name` in the Word namespace, such as `w:val`.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attr_in(&Ns::Word, name)
    }

    /// The value of the attribute `name` in `ns`.
    pub fn attr_in(&self, ns: &Ns, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|a| a.ns == *ns && &*a.name == name)
            .map(|a| &*a.value)
    }

    /// The `w:val` attribute.
    pub fn val(&self) -> Option<&str> {
        self.attr("val")
    }

    /// This element read as an on/off property: present without `w:val`, or `true`, `1`, `on` is
    /// true, and `false`, `0`, `off` is false.
    pub fn on_off(&self) -> bool {
        st_on_off(self.val())
    }

    /// The first child with this name in the Word namespace.
    pub fn child(&self, name: &str) -> Option<&Prop> {
        self.children
            .iter()
            .find(|c| c.ns == Ns::Word && &*c.name == name)
    }
}

/// The contents of a property container, in document order with duplicates kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Props(Vec<Prop>);

impl Props {
    /// An empty container.
    pub fn new() -> Props {
        Props(Vec::new())
    }

    /// Reads the container element `node` (a `w:pPr`, `w:rPr` and so on).
    pub fn read(node: NodeRef<'_>) -> Props {
        PropsRef::new(node).to_owned()
    }

    /// The properties, in document order.
    pub fn iter(&self) -> impl Iterator<Item = &Prop> {
        self.0.iter()
    }

    /// The same properties without those named in the Word namespace in `names`, such as `rPr` and
    /// `sectPr` in a paragraph's properties. Comparing two paragraphs' formatting is comparing what
    /// is left.
    pub fn without(mut self, names: &[&str]) -> Props {
        self.0
            .retain(|p| !(p.ns == Ns::Word && names.contains(&&*p.name)));
        self
    }

    /// True if the container has no property.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The first property `name` of the Word namespace.
    pub fn get(&self, name: &str) -> Option<&Prop> {
        self.get_in(&Ns::Word, name)
    }

    /// The first property `name` of any namespace.
    pub fn get_in(&self, ns: &Ns, name: &str) -> Option<&Prop> {
        self.0.iter().find(|p| p.ns == *ns && &*p.name == name)
    }

    /// The Word-namespace property `name` as an on/off value, or `None` if it is absent.
    pub fn on_off(&self, name: &str) -> Option<bool> {
        self.get(name).map(Prop::on_off)
    }

    /// The `w:val` of the Word-namespace property `name`.
    pub fn val(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(Prop::val)
    }

    /// The attribute `attr` (Word namespace) of the Word-namespace property `name`.
    pub fn attr(&self, name: &str, attr: &str) -> Option<&str> {
        self.get(name).and_then(|p| p.attr(attr))
    }
}

/// The same reads as `Props`, straight over the container element and without allocating.
#[derive(Debug, Clone, Copy)]
pub struct PropsRef<'a> {
    node: NodeRef<'a>,
}

impl<'a> PropsRef<'a> {
    /// Reads over the container element `node`.
    pub fn new(node: NodeRef<'a>) -> PropsRef<'a> {
        PropsRef { node }
    }

    /// The property elements, in document order.
    pub fn iter(self) -> impl Iterator<Item = NodeRef<'a>> {
        self.node.children().filter(NodeRef::is_element)
    }

    /// The first property `name` of the Word namespace.
    pub fn get(self, name: &str) -> Option<NodeRef<'a>> {
        self.get_in(&Ns::Word, name)
    }

    /// The first property `name` of any namespace.
    pub fn get_in(self, ns: &Ns, name: &str) -> Option<NodeRef<'a>> {
        self.iter()
            .find(|n| n.local_name() == name && ns.matches_uri(n.namespace()))
    }

    /// The Word-namespace property `name` as an on/off value, or `None` if it is absent.
    pub fn on_off(self, name: &str) -> Option<bool> {
        let n = self.get(name)?;
        Some(st_on_off(w_attr(&n, "val").as_deref()))
    }

    /// The `w:val` of the Word-namespace property `name`.
    pub fn val(self, name: &str) -> Option<Cow<'a, str>> {
        w_attr(&self.get(name)?, "val")
    }

    /// The attribute `attr` (Word namespace) of the Word-namespace property `name`.
    pub fn attr(self, name: &str, attr: &str) -> Option<Cow<'a, str>> {
        w_attr(&self.get(name)?, attr)
    }

    /// Reads the whole container into an owned `Props`.
    pub fn to_owned(self) -> Props {
        Props(read_children(self.node, DEPTH))
    }
}

/// Reads the child elements of `node`, `levels` deep.
///
/// This recurses, which is safe here: the depth is the constant `DEPTH` (four), not the depth of
/// the input, so the stack use is bounded whatever the part holds.
fn read_children(node: NodeRef<'_>, levels: usize) -> Vec<Prop> {
    let Some(next) = levels.checked_sub(1) else {
        return Vec::new();
    };
    node.child_elements()
        .map(|c| Prop {
            ns: Ns::from_uri(c.namespace()),
            name: c.local_name().into(),
            attrs: c
                .attrs()
                .filter(|a| !a.is_namespace_declaration())
                .map(|a| Attr {
                    ns: Ns::from_uri(a.namespace()),
                    name: a.local_name().into(),
                    value: a.value().into(),
                })
                .collect(),
            children: read_children(c, next),
        })
        .collect()
}
