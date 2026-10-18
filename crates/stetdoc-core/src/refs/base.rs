//! The namespace id, the lineage and the base token.
//!
//! A **namespace** is the identity of the bytes a document was opened from. A **lineage** is a hash
//! chain over the committed states of one namespace, so two sessions opened from the same bytes tell
//! themselves apart as soon as their edits differ. A **base token** is the namespace, a state version
//! and the lineage at that version, written as one opaque string. Views, receipts and readable files
//! carry the token of the state they describe, and an operation batch names the token it was
//! written against.

use super::digest::{Hasher, hex, unhex};
use crate::api::error::{ErrorCode, StetError};
use std::fmt;

/// The version of the rules that number blocks and mint refs. It is hashed into every namespace,
/// so a release that numbers blocks differently can never share refs with one that numbers them
/// otherwise. It changes only in a major release.
pub(crate) const REF_SCHEME: u32 = 1;

const NAMESPACE_DOMAIN: &str = "stetdoc/ref-namespace/1";
const DERIVED_DOMAIN: &str = "stetdoc/ref-namespace/derived/1";
const LINEAGE_DOMAIN: &str = "stetdoc/lineage/1";

/// The most parents and the longest label and extra a derived namespace records.
pub(crate) const MAX_PARENTS: usize = 4;
pub(crate) const MAX_LABEL: usize = 32;
pub(crate) const MAX_EXTRA: usize = 256;

/// The id of a ref namespace: 8 bytes, written as 16 lower-case hexadecimal digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct NamespaceId([u8; 8]);

fn truncate(digest: [u8; 32]) -> [u8; 8] {
    let mut out = [0u8; 8];
    for (slot, b) in out.iter_mut().zip(digest) {
        *slot = b;
    }
    out
}

impl NamespaceId {
    /// The namespace of the bytes a document was opened from, under a block-scan version. The same
    /// bytes always give the same id, on every run and every platform.
    pub(crate) fn of_package(opened: &[u8], scan_version: u32) -> NamespaceId {
        let mut h = Hasher::new(NAMESPACE_DOMAIN);
        h.number(u64::from(REF_SCHEME))
            .number(u64::from(scan_version))
            .field(opened);
        NamespaceId(truncate(h.finish()))
    }

    /// The namespace of a state that is not the result of opening bytes, such as a redline built by
    /// `compare`, from the namespaces of what it was built from and some extra bytes.
    pub(crate) fn derived(label: &str, parents: &[NamespaceId], extra: &[u8]) -> NamespaceId {
        let mut h = Hasher::new(DERIVED_DOMAIN);
        h.number(u64::from(REF_SCHEME))
            .field(label.as_bytes())
            .number(parents.len() as u64);
        for p in parents {
            h.field(&p.0);
        }
        h.field(extra);
        NamespaceId(truncate(h.finish()))
    }

    /// Reads 16 lower-case hexadecimal digits.
    pub(crate) fn parse(text: &str) -> Option<NamespaceId> {
        unhex::<8>(text).map(NamespaceId)
    }

    /// The 8 bytes.
    pub(crate) fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex(&self.0))
    }
}

/// How a namespace came to be, which `resume` needs in order to check it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum NamespaceOrigin {
    /// From the bytes of a package. It is checked against the original package bytes.
    Opened,
    /// From other namespaces, by [`NamespaceId::derived`]. It is checked by recomputing it.
    Derived {
        /// What kind of state it is, for example `compare`.
        label: String,
        /// The namespaces it was built from.
        parents: Vec<NamespaceId>,
        /// Anything else that went into it, such as a stamp.
        extra: Vec<u8>,
    },
}

impl NamespaceOrigin {
    /// The namespace this origin gives, or `None` for an opened namespace when the original bytes
    /// are not at hand.
    pub(crate) fn recompute(
        &self,
        original: Option<&[u8]>,
        scan_version: u32,
    ) -> Option<NamespaceId> {
        match self {
            NamespaceOrigin::Opened => original.map(|b| NamespaceId::of_package(b, scan_version)),
            NamespaceOrigin::Derived {
                label,
                parents,
                extra,
            } => Some(NamespaceId::derived(label, parents, extra)),
        }
    }

    /// True if the label, parents and extra are within the limits a ref map records.
    pub(crate) fn within_limits(&self) -> bool {
        match self {
            NamespaceOrigin::Opened => true,
            NamespaceOrigin::Derived {
                label,
                parents,
                extra,
            } => {
                !label.is_empty()
                    && label.len() <= MAX_LABEL
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                    && parents.len() <= MAX_PARENTS
                    && extra.len() <= MAX_EXTRA
            }
        }
    }
}

/// A point in the hash chain over the committed states of one namespace: 8 bytes, written as 16
/// lower-case hexadecimal digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct Lineage([u8; 8]);

impl Lineage {
    /// The lineage of version 0, which is the namespace id itself.
    pub(crate) fn root(namespace: &NamespaceId) -> Lineage {
        Lineage(namespace.0)
    }

    /// The lineage of the next version: a hash of this one, the new version number, and the digest
    /// of what the commit changed.
    pub(crate) fn next(&self, version: u32, changes: &[u8; 32]) -> Lineage {
        let mut h = Hasher::new(LINEAGE_DOMAIN);
        h.field(&self.0).number(u64::from(version)).field(changes);
        Lineage(truncate(h.finish()))
    }

    /// Reads 16 lower-case hexadecimal digits.
    pub(crate) fn parse(text: &str) -> Option<Lineage> {
        unhex::<8>(text).map(Lineage)
    }
}

impl fmt::Display for Lineage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&hex(&self.0))
    }
}

/// The namespace, state version and lineage of one state of a document, as one opaque string such as
/// `3f9a0c11d2b4e577:7:51ac09f2e8b34d11`. Callers copy a token from a view or a receipt and never
/// build one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct BaseToken {
    namespace: NamespaceId,
    version: u32,
    lineage: Lineage,
}

impl BaseToken {
    /// The token of a state with this namespace, version and lineage.
    pub(crate) fn new(namespace: NamespaceId, version: u32, lineage: Lineage) -> BaseToken {
        BaseToken {
            namespace,
            version,
            lineage,
        }
    }

    /// The token of a freshly opened document: version 0, with the namespace as its lineage.
    pub(crate) fn initial(namespace: NamespaceId) -> BaseToken {
        BaseToken::new(namespace, 0, Lineage::root(&namespace))
    }

    /// The namespace.
    pub(crate) fn namespace(&self) -> NamespaceId {
        self.namespace
    }

    /// The state version.
    pub(crate) fn version(&self) -> u32 {
        self.version
    }

    /// The lineage at that version.
    pub(crate) fn lineage(&self) -> Lineage {
        self.lineage
    }

    /// Reads a token exactly as [`fmt::Display`] writes it. Anything else is `invalid_payload`.
    pub(crate) fn parse(text: &str) -> Result<BaseToken, StetError> {
        let parsed = (|| {
            let mut parts = text.split(':');
            let namespace = NamespaceId::parse(parts.next()?)?;
            let version = parts.next()?;
            let lineage = Lineage::parse(parts.next()?)?;
            let canonical = !version.is_empty()
                && version.len() <= 10
                && version.bytes().all(|b| b.is_ascii_digit())
                && (version == "0" || !version.starts_with('0'));
            let version = canonical.then(|| version.parse::<u32>().ok()).flatten()?;
            parts
                .next()
                .is_none()
                .then_some(BaseToken::new(namespace, version, lineage))
        })();
        parsed.ok_or_else(|| {
            StetError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "{} is not a base token. Copy the token from the view or the receipt, such as \
                     3f9a0c11d2b4e577:7:51ac09f2e8b34d11.",
                    super::token::shown(text)
                ),
            )
            .with_next("Name the base token exactly as the view or the receipt shows it.")
        })
    }
}

impl fmt::Display for BaseToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.namespace, self.version, self.lineage)
    }
}

/// What a batch says about the document it was written for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Claim<'a> {
    /// The base token the batch names. Resolution checks it against the document's history.
    Base(&'a BaseToken),
    /// Nothing is named. Only the read path accepts it, for `inspect` and for the operations that
    /// `Edit::files` derives, which have already compared the files' token with the document's.
    Implied,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_bytes_give_the_same_namespace() {
        let a = NamespaceId::of_package(b"package bytes", 1);
        assert_eq!(a, NamespaceId::of_package(b"package bytes", 1));
        assert_eq!(a.to_string().len(), 16);
        assert_eq!(NamespaceId::parse(&a.to_string()), Some(a));
        assert_eq!(hex(a.as_bytes()), a.to_string());
    }

    #[test]
    fn a_claim_names_a_token_or_nothing() {
        let t = BaseToken::initial(NamespaceId::of_package(b"a", 1));
        assert_ne!(Claim::Base(&t), Claim::Implied);
        assert_eq!(Claim::Base(&t), Claim::Base(&t));
    }

    #[test]
    fn different_bytes_or_scan_versions_give_different_namespaces() {
        let a = NamespaceId::of_package(b"package bytes", 1);
        assert_ne!(a, NamespaceId::of_package(b"package byteS", 1));
        assert_ne!(a, NamespaceId::of_package(b"package bytes", 2));
        assert_ne!(a, NamespaceId::of_package(b"package bytes ", 1));
        assert_ne!(
            NamespaceId::of_package(b"", 1),
            NamespaceId::of_package(b"\0", 1)
        );
    }

    #[test]
    fn a_derived_namespace_depends_on_every_input_and_their_order() {
        let a = NamespaceId::of_package(b"a", 1);
        let b = NamespaceId::of_package(b"b", 1);
        let d = NamespaceId::derived("compare", &[a, b], b"stamp");
        assert_eq!(d, NamespaceId::derived("compare", &[a, b], b"stamp"));
        assert_ne!(d, NamespaceId::derived("compare", &[b, a], b"stamp"));
        assert_ne!(d, NamespaceId::derived("compare", &[a, b], b"stamp2"));
        assert_ne!(d, NamespaceId::derived("redline", &[a, b], b"stamp"));
        assert_ne!(d, NamespaceId::derived("compare", &[a], b"stamp"));
        assert_ne!(d, a);
    }

    #[test]
    fn an_origin_recomputes_its_namespace() {
        let a = NamespaceId::of_package(b"a", 1);
        let opened = NamespaceOrigin::Opened;
        assert_eq!(opened.recompute(Some(b"a"), 1), Some(a));
        assert_eq!(opened.recompute(None, 1), None);
        let derived = NamespaceOrigin::Derived {
            label: "compare".into(),
            parents: vec![a],
            extra: b"x".to_vec(),
        };
        assert_eq!(
            derived.recompute(None, 1),
            Some(NamespaceId::derived("compare", &[a], b"x"))
        );
        assert!(derived.within_limits());
        let bad = NamespaceOrigin::Derived {
            label: "Compare".into(),
            parents: vec![],
            extra: vec![],
        };
        assert!(!bad.within_limits(), "upper case");
        let long = NamespaceOrigin::Derived {
            label: "x".repeat(MAX_LABEL + 1),
            parents: vec![],
            extra: vec![],
        };
        assert!(!long.within_limits());
        let many = NamespaceOrigin::Derived {
            label: "x".into(),
            parents: vec![a; MAX_PARENTS + 1],
            extra: vec![],
        };
        assert!(!many.within_limits());
    }

    #[test]
    fn the_lineage_of_version_zero_is_the_namespace() {
        let ns = NamespaceId::of_package(b"a", 1);
        assert_eq!(Lineage::root(&ns).to_string(), ns.to_string());
        assert_eq!(BaseToken::initial(ns).version(), 0);
    }

    #[test]
    fn two_histories_get_two_lineages_and_equal_histories_equal_ones() {
        let ns = NamespaceId::of_package(b"a", 1);
        let l0 = Lineage::root(&ns);
        let one = [1u8; 32];
        let other = [2u8; 32];
        let a1 = l0.next(1, &one);
        assert_eq!(a1, l0.next(1, &one));
        assert_ne!(a1, l0.next(1, &other));
        assert_ne!(a1, l0.next(2, &one));
        let a2 = a1.next(2, &one);
        let b2 = l0.next(1, &other).next(2, &one);
        assert_ne!(a2, b2, "a fork stays a fork");
        assert_ne!(a2, a1);
    }

    #[test]
    fn a_token_is_namespace_version_lineage() {
        let ns = NamespaceId::of_package(b"a", 1);
        let l1 = Lineage::root(&ns).next(1, &[7; 32]);
        let t = BaseToken::new(ns, 7, l1);
        let text = t.to_string();
        assert_eq!(text, format!("{ns}:7:{l1}"));
        assert_eq!(BaseToken::parse(&text).unwrap(), t);
        assert_eq!((t.namespace(), t.version(), t.lineage()), (ns, 7, l1));
        let zero = BaseToken::initial(ns);
        assert_eq!(BaseToken::parse(&zero.to_string()).unwrap(), zero);
    }

    #[test]
    fn every_other_spelling_of_a_token_is_invalid_payload() {
        let ns = NamespaceId::of_package(b"a", 1);
        let l = Lineage::root(&ns);
        let good = BaseToken::new(ns, 3, l).to_string();
        let upper = good.to_uppercase();
        let bad = [
            String::new(),
            "x".into(),
            upper,
            format!("{ns}:3"),
            format!("{ns}:3:{l}:"),
            format!("{ns}:3:{l}:0"),
            format!("{ns}:03:{l}"),
            format!("{ns}:+3:{l}"),
            format!("{ns}:-3:{l}"),
            format!("{ns}::{l}"),
            format!("{ns}:4294967296:{l}"),
            format!("{ns}:99999999999:{l}"),
            format!("{ns} :3:{l}"),
            format!("{}:3:{l}", &ns.to_string()[..15]),
            format!("{ns}:3:{}", &l.to_string()[1..]),
        ];
        for text in bad {
            let e = BaseToken::parse(&text).unwrap_err();
            assert_eq!(e.code(), ErrorCode::InvalidPayload, "{text:?}");
        }
        assert!(BaseToken::parse(&format!("{ns}:4294967295:{l}")).is_ok());
        assert!(BaseToken::parse(&format!("{ns}:0:{l}")).is_ok());
    }
}
