//! Content types and relationships: reading them, and adding one element to each.
//!
//! Both parts are XML, so they go through the span tree. Reading looks at the root's child
//! elements. Adding appends one `Override` or `Relationship` element as a new last child of the
//! root, which re-emits only the root's end tag and the new element: everything else in the part,
//! its prefixes, whitespace, BOM and encoding included, keeps its original bytes. OPC parts have no
//! element-order constraints, so nothing here needs `schema::insert_in_order`.

use super::errors::{invalid, malformed};
use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::xml::{Bytes, NodeRef, SpanTree};

/// The relationship type that points at the main document part.
pub const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";

/// The content type of a relationships part, which has a default registration by extension.
pub(super) const CT_RELATIONSHIPS: &str =
    "application/vnd.openxmlformats-package.relationships+xml";

/// The name of the content types part.
pub(super) const CONTENT_TYPES_PART: &str = "[Content_Types].xml";

/// Parses a part and checks that its root element has the expected name.
fn parse_root(
    name: &str,
    part: &Bytes,
    root: &str,
    limits: &Limits,
) -> Result<SpanTree, StetError> {
    let tree = SpanTree::parse(name, part.clone(), limits)?;
    if tree.root().local_name() != root {
        return Err(malformed(format!(
            "{name}: expected a {root} part, but the root element is {}",
            tree.root().name()
        )));
    }
    Ok(tree)
}

fn attr(el: NodeRef<'_>, name: &str) -> Option<String> {
    el.attr("", name).map(|v| v.into_owned())
}

/// Maps part names and extensions to content types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentTypes {
    /// (lower-case extension, content type)
    pub defaults: Vec<(String, String)>,
    /// (part name with a leading slash, content type)
    pub overrides: Vec<(String, String)>,
}

impl ContentTypes {
    /// Reads `[Content_Types].xml`. A DOCTYPE or malformed markup is `malformed_input`.
    pub fn parse(part: &Bytes, limits: &Limits) -> Result<Self, StetError> {
        let tree = parse_root(CONTENT_TYPES_PART, part, "Types", limits)?;
        let mut ct = ContentTypes::default();
        for el in tree.root().child_elements() {
            match el.local_name() {
                "Default" => {
                    if let (Some(e), Some(t)) = (attr(el, "Extension"), attr(el, "ContentType")) {
                        ct.defaults.push((e.to_ascii_lowercase(), t));
                    }
                }
                "Override" => {
                    if let (Some(p), Some(t)) = (attr(el, "PartName"), attr(el, "ContentType")) {
                        ct.overrides.push((p, t));
                    }
                }
                _ => {}
            }
        }
        Ok(ct)
    }

    /// The content type of a part: an override by name, else the default for its extension.
    pub fn type_of(&self, part: &str) -> Option<&str> {
        let want = format!("/{part}");
        if let Some((_, t)) = self
            .overrides
            .iter()
            .find(|(p, _)| p.eq_ignore_ascii_case(&want))
        {
            return Some(t);
        }
        let ext = part.rsplit_once('.')?.1.to_ascii_lowercase();
        self.defaults
            .iter()
            .find(|(e, _)| *e == ext)
            .map(|(_, t)| t.as_str())
    }

    /// True if there is an override for exactly this part.
    pub fn has_override(&self, part: &str) -> bool {
        let want = format!("/{part}");
        self.overrides
            .iter()
            .any(|(p, _)| p.eq_ignore_ascii_case(&want))
    }
}

/// One relationship: a typed, identified link from a source part to a target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// The relationship id, such as `rId3`.
    pub id: String,
    /// The relationship type URI.
    pub rel_type: String,
    /// The target as written: a path relative to the source part, or an external URL.
    pub target: String,
    /// True when `TargetMode` is `External`.
    pub external: bool,
}

/// The relationships of one source part, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Relationships {
    /// The relationships in file order.
    pub items: Vec<Relationship>,
}

impl Relationships {
    /// Reads a relationships part, which has the name `name`. A DOCTYPE or malformed markup is
    /// `malformed_input`, and the message names the part.
    pub fn parse(name: &str, part: &Bytes, limits: &Limits) -> Result<Self, StetError> {
        let tree = parse_root(name, part, "Relationships", limits)?;
        let mut r = Relationships::default();
        for el in tree.root().child_elements() {
            if el.local_name() != "Relationship" {
                continue;
            }
            if let (Some(id), Some(t), Some(target)) =
                (attr(el, "Id"), attr(el, "Type"), attr(el, "Target"))
            {
                r.items.push(Relationship {
                    id,
                    rel_type: t,
                    target,
                    external: attr(el, "TargetMode")
                        .is_some_and(|m| m.eq_ignore_ascii_case("External")),
                });
            }
        }
        Ok(r)
    }

    /// The relationship with this id.
    pub fn get(&self, id: &str) -> Option<&Relationship> {
        self.items.iter().find(|r| r.id == id)
    }

    /// The relationships of one type, in file order.
    pub fn by_type<'a>(&'a self, rel_type: &'a str) -> impl Iterator<Item = &'a Relationship> {
        self.items.iter().filter(move |r| r.rel_type == rel_type)
    }

    /// The next free `rIdN`: one more than the highest numeric suffix, so it is deterministic.
    pub fn next_id(&self) -> String {
        let max = self
            .items
            .iter()
            .filter_map(|r| r.id.strip_prefix("rId")?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("rId{}", max.saturating_add(1))
    }
}

/// The name of the relationships part for a source part. An empty source means the package.
pub fn rels_part_name(source: &str) -> String {
    match source.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None if source.is_empty() => "_rels/.rels".to_owned(),
        None => format!("_rels/{source}.rels"),
    }
}

/// Resolves an internal relationship target against its source part, to a part name.
///
/// A target is a URI reference, so a `%XX` in it stands for one byte of the part name (UTF-8), and
/// `media/image%201.png` names the part `media/image 1.png`. Escaped dots count as dots. A
/// fragment is dropped. Returns `None` if the target climbs above the package root, or is not a
/// part name once decoded: a bad escape, bytes that are not UTF-8, or an escaped slash, backslash
/// or control character. `source` is a part name and is not decoded again.
pub fn resolve_target(source: &str, target: &str) -> Option<String> {
    let target = target.split('#').next().unwrap_or(target);
    let (mut parts, path): (Vec<String>, &str) = match target.strip_prefix('/') {
        Some(absolute) => (Vec::new(), absolute),
        None => {
            let dir = source.rsplit_once('/').map_or("", |(d, _)| d);
            (
                dir.split('/')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                target,
            )
        }
    };
    for segment in path.split('/') {
        match percent_decode(segment)?.as_str() {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s.to_owned()),
        }
    }
    Some(parts.join("/"))
}

/// Decodes the `%XX` escapes of one path segment.
fn percent_decode(segment: &str) -> Option<String> {
    if !segment.contains('%') {
        return Some(segment.to_owned());
    }
    let hex = |b: Option<&u8>| -> Option<u8> {
        char::from(*b?)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            out.push(hex(bytes.get(i + 1))? * 16 + hex(bytes.get(i + 2))?);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    let decoded = String::from_utf8(out).ok()?;
    if decoded
        .chars()
        .any(|c| c == '/' || c == '\\' || c.is_control())
    {
        return None;
    }
    Some(decoded)
}

/// Writes one segment of a part name as it goes in a URI path: letters, digits and the
/// characters a path segment may hold stay, and every other byte becomes `%XX`.
pub(super) fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for &b in segment.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The main document part's name, from the package relationships.
pub(super) fn office_document_target(rels: &Bytes, limits: &Limits) -> Option<String> {
    let r = Relationships::parse(&rels_part_name(""), rels, limits).ok()?;
    let rel = r.by_type(REL_OFFICE_DOCUMENT).next()?;
    if rel.external {
        return None;
    }
    resolve_target("", &rel.target)
}

/// A relationships part with a single relationship, for a source that has none yet.
/// `external` adds `TargetMode="External"`.
pub(super) fn new_rels_part(id: &str, rel_type: &str, target: &str, external: bool) -> Vec<u8> {
    fn esc(v: &str) -> String {
        v.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/></Relationships>",
        esc(id),
        esc(rel_type),
        esc(target),
        if external { " TargetMode=\"External\"" } else { "" }
    )
    .into_bytes()
}

/// Appends `<element a="v" ...>` as the last child of the root, in the root's own namespace and
/// prefix, and returns the part's new bytes. Only the root's end tag and the new element are
/// written; everything else is copied from the original.
fn append(
    name: &str,
    part: &Bytes,
    root: &str,
    element: &str,
    attrs: &[(&str, &str)],
    limits: &Limits,
) -> Result<Vec<u8>, StetError> {
    let mut tree = parse_root(name, part, root, limits)?;
    let root_id = tree.root().id();
    let ns = tree.root().namespace().to_owned();
    let mut ed = tree.edit();
    let el = ed.create_element(&ns, element)?;
    for (name, value) in attrs {
        ed.set_attr(el, "", name, value)?;
    }
    ed.append_child(root_id, el)?;
    Ok(tree.serialize())
}

/// `[Content_Types].xml` with an `Override` for `part` added.
pub(super) fn with_override(
    part: &Bytes,
    name: &str,
    content_type: &str,
    limits: &Limits,
) -> Result<Vec<u8>, StetError> {
    if name.is_empty() {
        return Err(invalid("a content type override needs a part name"));
    }
    append(
        CONTENT_TYPES_PART,
        part,
        "Types",
        "Override",
        &[
            ("PartName", &format!("/{name}")),
            ("ContentType", content_type),
        ],
        limits,
    )
}

/// A relationships part with one relationship added.
pub(super) fn with_relationship(
    name: &str,
    part: &Bytes,
    id: &str,
    rel_type: &str,
    target: &str,
    external: bool,
    limits: &Limits,
) -> Result<Vec<u8>, StetError> {
    let mut attrs = vec![("Id", id), ("Type", rel_type), ("Target", target)];
    if external {
        attrs.push(("TargetMode", "External"));
    }
    append(name, part, "Relationships", "Relationship", &attrs, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lim() -> Limits {
        Limits::default()
    }

    fn bytes(s: &str) -> Bytes {
        Bytes::from(s.as_bytes().to_vec())
    }

    const CT: &str = "\u{feff}<?xml version=\"1.0\"?>\r\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"XML\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"a&amp;b\"/></Types>\r\n";

    #[test]
    fn parses_content_types() {
        let ct = ContentTypes::parse(&bytes(CT), &lim()).unwrap();
        assert_eq!(ct.type_of("word/document.xml"), Some("a&b"));
        assert_eq!(ct.type_of("customXml/item1.xml"), Some("application/xml"));
        assert_eq!(ct.type_of("x.bin"), None);
        assert!(ct.has_override("WORD/document.xml"));
    }

    #[test]
    fn adds_an_override_and_keeps_everything_else() {
        let out = with_override(&bytes(CT), "word/comments.xml", "t", &lim()).unwrap();
        let s = String::from_utf8(out).unwrap();
        let want = CT.replace(
            "</Types>",
            "<Override PartName=\"/word/comments.xml\" ContentType=\"t\"/></Types>",
        );
        assert_eq!(s, want);
        assert!(s.starts_with('\u{feff}'));
    }

    #[test]
    fn prefixed_and_self_closing_roots_and_utf16() {
        let a = "<ct:Types xmlns:ct=\"http://schemas.openxmlformats.org/package/2006/content-types\"></ct:Types>";
        let out = with_override(&bytes(a), "p", "t", &lim()).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("<ct:Override"));
        let b = "<Relationships xmlns=\"x\"/>";
        let out =
            with_relationship("x.rels", &bytes(b), "rId1", "T", "a.xml", false, &lim()).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<Relationships xmlns=\"x\"><Relationship Id=\"rId1\" Type=\"T\" Target=\"a.xml\"/></Relationships>"
        );
        // A UTF-16 part is edited in UTF-16, with its byte order mark.
        let mut u16: Vec<u8> = vec![0xFF, 0xFE];
        for c in "<Relationships xmlns=\"x\"></Relationships>".encode_utf16() {
            u16.extend(c.to_le_bytes());
        }
        let out = with_relationship(
            "x.rels",
            &Bytes::from(u16),
            "rId1",
            "T",
            "a.xml",
            false,
            &lim(),
        )
        .unwrap();
        assert!(out.starts_with(&[0xFF, 0xFE]));
        let text: Vec<u16> = out[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert!(
            String::from_utf16(&text)
                .unwrap()
                .contains("<Relationship Id=\"rId1\"")
        );
    }

    #[test]
    fn relationships_and_targets() {
        let x = "<Relationships xmlns=\"x\"><Relationship Id=\"rId7\" Type=\"T\" Target=\"../a/b.xml\"/><Relationship Id=\"rId2\" Type=\"U\" Target=\"http://x?a=1&amp;b=2\" TargetMode=\"External\"/></Relationships>";
        let r = Relationships::parse("x.rels", &bytes(x), &lim()).unwrap();
        assert_eq!(r.next_id(), "rId8");
        assert!(r.get("rId2").unwrap().external);
        assert_eq!(r.get("rId2").unwrap().target, "http://x?a=1&b=2");
        assert_eq!(
            resolve_target("word/document.xml", "../a/b.xml").as_deref(),
            Some("a/b.xml")
        );
        assert_eq!(
            resolve_target("word/document.xml", "/docProps/core.xml").as_deref(),
            Some("docProps/core.xml")
        );
        assert_eq!(resolve_target("word/document.xml", "../../x"), None);
        // Targets are URI references: escapes decode, escaped dots are dots, and what cannot be a
        // part name once decoded is not one.
        assert_eq!(
            resolve_target("word/document.xml", "media/image%201.png#frag").as_deref(),
            Some("word/media/image 1.png")
        );
        assert_eq!(
            resolve_target("word/document.xml", "%2e%2e/caf%C3%A9.xml").as_deref(),
            Some("caf\u{e9}.xml")
        );
        assert_eq!(
            resolve_target("a b/c.xml", "d%25.xml").as_deref(),
            Some("a b/d%.xml")
        );
        for bad in [
            "a%2Fb",
            "a%5Cb",
            "a%00b",
            "a%zz",
            "a%2",
            "a%",
            "a%FF",
            "%2e%2e/%2e%2e/x",
        ] {
            assert_eq!(resolve_target("word/document.xml", bad), None, "{bad}");
        }
        assert_eq!(encode_segment("image 1.png"), "image%201.png");
        assert_eq!(encode_segment("caf\u{e9}%#?"), "caf%C3%A9%25%23%3F");
        assert_eq!(encode_segment("a-b_c.d~e"), "a-b_c.d~e");
        assert_eq!(
            rels_part_name("word/document.xml"),
            "word/_rels/document.xml.rels"
        );
        assert_eq!(rels_part_name(""), "_rels/.rels");
    }

    #[test]
    fn rejects_doctype_and_the_wrong_root() {
        let e = ContentTypes::parse(&bytes("<!DOCTYPE x [<!ENTITY a 'b'>]><Types/>"), &lim());
        assert!(e.is_err());
        assert!(ContentTypes::parse(&bytes("<Relationships xmlns=\"x\"/>"), &lim()).is_err());
        assert!(Relationships::parse("x.rels", &bytes("<Types/>"), &lim()).is_err());
    }
}
