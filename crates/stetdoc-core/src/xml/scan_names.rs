//! Namespace and attribute checks for the scanner: duplicate attributes, declarations, and the
//! resolution of element and attribute prefixes against the bindings in scope.

use super::errors::limit;
use super::scan::Parser;
use super::text::unescape_attr;
use super::tree::{Attr, Span, XML_NS, XMLNS_NS};
use crate::api::error::StetError;

/// The length of a prefix, which the tree stores in 16 bits. The all-ones value marks an
/// attribute whose prefix is not chosen yet, so it is not a valid length.
fn prefix_len(c: usize) -> Result<u16, StetError> {
    u16::try_from(c)
        .ok()
        .filter(|&c| c != Attr::LATE)
        .ok_or_else(|| limit("an XML name has a prefix of 65535 bytes or more"))
}

/// A handful of recent answers to "what does this prefix mean", valid while no binding changes.
#[derive(Default)]
pub(super) struct Recent<'a> {
    epoch: u64,
    entries: [(&'a [u8], Option<u16>); 4],
    used: usize,
    next: usize,
}

impl<'a> Parser<'a> {
    /// What `prefix` means here, or `None` if it is not bound. The empty prefix is the default
    /// namespace.
    pub(super) fn lookup(&mut self, prefix: &'a [u8]) -> Option<u16> {
        let r = &mut self.recent;
        if r.epoch != self.scope.epoch() {
            r.epoch = self.scope.epoch();
            r.used = 0;
            r.next = 0;
        }
        if let Some((_, answer)) = r.entries.iter().take(r.used).find(|(k, _)| *k == prefix) {
            return *answer;
        }
        let answer = self.scope.get(prefix);
        if let Some(slot) = r.entries.get_mut(r.next) {
            *slot = (prefix, answer);
        }
        r.next = (r.next + 1) % r.entries.len();
        r.used = (r.used + 1).min(r.entries.len());
        answer
    }
}

impl<'a> Parser<'a> {
    pub(super) fn check_duplicate_attrs(
        &self,
        first: usize,
        count: usize,
        at: usize,
    ) -> Result<(), StetError> {
        let list = self.attrs.slice(first, count).unwrap_or(&[]);
        let nm = |a: &Attr| {
            self.src
                .get(a.name.start as usize..a.name.end as usize)
                .unwrap_or(&[])
        };
        let dup = if list.len() <= 12 {
            list.iter()
                .enumerate()
                .any(|(i, a)| list.iter().skip(i + 1).any(|b| nm(a) == nm(b)))
        } else {
            let mut names: Vec<&[u8]> = list.iter().map(nm).collect();
            names.sort_unstable();
            names.windows(2).any(|w| w.first() == w.get(1))
        };
        if dup {
            return Err(self.bad(at, "duplicate attribute on one element"));
        }
        Ok(())
    }

    /// Refuses two attributes with the same expanded name (namespace and local name) under
    /// different prefixes bound to the same namespace, which Namespaces in XML 1.0 makes a
    /// well-formedness error ("Attributes Unique"). Equal qualified names were refused before
    /// resolution, and an unprefixed attribute is in no namespace while a prefixed one always is
    /// in one, so only prefixed attributes can collide. Declarations are in the `xmlns` namespace
    /// and their local names are the declared prefixes, which are distinct already.
    pub(super) fn check_duplicate_expanded(
        &self,
        first: usize,
        count: usize,
        at: usize,
    ) -> Result<(), StetError> {
        let list = self.attrs.slice(first, count).unwrap_or(&[]);
        let key = |a: &Attr| {
            let n = self
                .src
                .get(a.name.start as usize..a.name.end as usize)
                .unwrap_or(&[]);
            (a.ns, n.get(a.prefix_len as usize + 1..).unwrap_or(&[]))
        };
        let can_collide = |a: &&Attr| a.prefix_len > 0 && a.ns != self.xmlns_idx;
        // The usual element has a few attributes: compare pairs without allocating.
        let dup = if list.len() <= 12 {
            list.iter().filter(can_collide).enumerate().any(|(i, a)| {
                let k = key(a);
                list.iter()
                    .filter(can_collide)
                    .skip(i + 1)
                    .any(|b| key(b) == k)
            })
        } else {
            let mut keys: Vec<(u16, &[u8])> = list.iter().filter(can_collide).map(key).collect();
            keys.sort_unstable();
            keys.windows(2).any(|w| w.first() == w.get(1))
        };
        if dup {
            return Err(self.bad(
                at,
                "two attributes on one element have the same namespace and local name",
            ));
        }
        Ok(())
    }

    /// Binds the prefixes this start tag declares, and returns how many it declares. They stay
    /// bound until the element closes.
    pub(super) fn declare_namespaces(
        &mut self,
        first: usize,
        count: usize,
        at: usize,
    ) -> Result<usize, StetError> {
        let src: &'a [u8] = self.src;
        let mut declared = 0usize;
        for i in first..first + count {
            let Some(a) = self.attrs.get(i).copied() else {
                continue;
            };
            let n = src
                .get(a.name.start as usize..a.name.end as usize)
                .unwrap_or(&[]);
            let prefix: &'a [u8] = if n == b"xmlns" {
                b""
            } else if let Some(rest) = n.strip_prefix(b"xmlns:") {
                if rest.is_empty() || rest == b"xmlns" {
                    return Err(self.bad(at, "invalid namespace prefix declaration"));
                }
                rest
            } else {
                continue;
            };
            let raw = std::str::from_utf8(
                src.get(a.value.start as usize..a.value.end as usize)
                    .unwrap_or(&[]),
            )
            .unwrap_or("");
            let uri = unescape_attr(raw);
            if !prefix.is_empty() && uri.is_empty() {
                return Err(self.bad(
                    at,
                    "a prefixed namespace cannot be bound to the empty string",
                ));
            }
            let id = if uri.is_empty() {
                0
            } else {
                self.ns.intern(&uri)?
            };
            self.scope.bind(prefix, id);
            declared += 1;
            let in_scope = self.scope.mark();
            if in_scope > self.limits.max_xml_namespaces_in_scope as usize {
                return Err(limit(format!(
                    "more than {} namespace declarations are in scope at once (byte {at}{})",
                    self.limits.max_xml_namespaces_in_scope, self.note
                )));
            }
            self.ns_peak = self.ns_peak.max(in_scope);
        }
        Ok(declared)
    }

    fn xmlns_ns(&mut self) -> Result<u16, StetError> {
        if self.xmlns_idx == 0 {
            self.xmlns_idx = self.ns.intern(XMLNS_NS)?;
        }
        Ok(self.xmlns_idx)
    }

    fn xml_ns(&mut self) -> Result<u16, StetError> {
        if self.xml_idx == 0 {
            self.xml_idx = self.ns.intern(XML_NS)?;
        }
        Ok(self.xml_idx)
    }

    pub(super) fn resolve_element(
        &mut self,
        name: Span,
        at: usize,
    ) -> Result<(u16, u16), StetError> {
        let n = self
            .src
            .get(name.start as usize..name.end as usize)
            .unwrap_or(&[]);
        match memchr::memchr(b':', n) {
            None => Ok((0, self.lookup(b"").unwrap_or(0))),
            Some(c) => {
                let prefix = n.get(..c).unwrap_or(&[]);
                let ns = self.resolve_prefix(prefix, at)?;
                Ok((prefix_len(c)?, ns))
            }
        }
    }

    pub(super) fn resolve_attr(&mut self, name: Span, at: usize) -> Result<(u16, u16), StetError> {
        let n = self
            .src
            .get(name.start as usize..name.end as usize)
            .unwrap_or(&[]);
        if n == b"xmlns" {
            return self.xmlns_ns().map(|ns| (0, ns));
        }
        match memchr::memchr(b':', n) {
            None => Ok((0, 0)),
            Some(c) => {
                let prefix = n.get(..c).unwrap_or(&[]);
                if prefix == b"xmlns" {
                    let ns = self.xmlns_ns()?;
                    return Ok((prefix_len(c)?, ns));
                }
                let ns = self.resolve_prefix(prefix, at)?;
                Ok((prefix_len(c)?, ns))
            }
        }
    }

    fn resolve_prefix(&mut self, prefix: &'a [u8], at: usize) -> Result<u16, StetError> {
        if let Some(ns) = self.lookup(prefix) {
            return Ok(ns);
        }
        if prefix == b"xml" {
            return self.xml_ns();
        }
        Err(self.bad(
            at,
            &format!(
                "namespace prefix {:?} is not declared",
                String::from_utf8_lossy(prefix)
            ),
        ))
    }
}
