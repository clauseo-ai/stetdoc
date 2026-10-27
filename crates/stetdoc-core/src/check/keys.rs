//! The keys of a unit: what is wrong, never where in the file.
//!
//! A schema violation has a `ViolationKey`. A Word rule, a package rule or a finding of the
//! comment audit has a code and a subject: an element, an id, a name. A [`KeySet`] holds both
//! kinds, and "no new violations" is `after.added_since(&before)` being empty. Counts play no
//! part in that comparison: splitting a run clones its properties, and counting would refuse the
//! clone of a problem the run already had. The exception is a counted key ([`Key::is_counted`]):
//! a child of a property container out of the order Word writes, which the compiler never copies
//! (`revisions::canonical`), and which the Open XML SDK counts as a schema error each time.

use crate::api::check::FindingCode;
use crate::schema::{ViolationKey, ViolationKind, ViolationSet};
use std::borrow::Cow;
use std::collections::BTreeMap;

/// The elements the compiler renames and keeps the content of, with the name they had before:
/// `w:t` becomes `w:delText` and `w:instrText` becomes `w:delInstrText` when a run is deleted, and
/// they are renamed back when the deletion is rejected. A problem the element had (python-docx
/// writes `w:space` on `w:t`, and 1,274 of 41,688 LAB documents have it) must keep its key
/// across the rename, or every tracked deletion in those documents would add a violation. Add a
/// pair here when `revisions` renames another element.
const RENAMED: [(&str, &str); 2] = [("delText", "t"), ("delInstrText", "instrText")];

/// The name an element has in a key: the one it had before it was renamed.
pub(crate) fn unrenamed(local: &str) -> &str {
    RENAMED
        .iter()
        .find(|(renamed, _)| *renamed == local)
        .map_or(local, |(_, original)| original)
}

/// A qualified name (`w:delText`) with its local part unrenamed (`w:t`).
fn unrenamed_qname(qname: &str) -> Cow<'_, str> {
    let (prefix, local) = qname.rsplit_once(':').unwrap_or(("", qname));
    let original = unrenamed(local);
    if original == local {
        Cow::Borrowed(qname)
    } else if prefix.is_empty() {
        Cow::Borrowed(original)
    } else {
        Cow::Owned(format!("{prefix}:{original}"))
    }
}

/// The declaration that stands in the key of a violation in a property container, whichever
/// declaration the container has where it stands (see [`Key::schema`]).
const PROPERTIES: &str = "properties";

/// Whether a qualified name is a property container that the compiler copies from one place to
/// another: `w:rPr` and `w:pPr`, and the containers a tracked change of properties keeps a copy of
/// in its record (`w:tblPr` in `w:tblPrChange`, `w:tblPrEx`, `w:trPr`, `w:tcPr`, `w:sectPr`). The
/// prefix of a key is the schema's, `w` in either dialect, and the math `m:rPr` is not one.
fn is_property_container(qname: &str) -> bool {
    matches!(
        qname,
        "w:rPr" | "w:pPr" | "w:tblPr" | "w:tblPrEx" | "w:trPr" | "w:tcPr" | "w:sectPr"
    )
}

/// What is wrong, independent of where in the file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Key {
    /// A departure from the schema.
    Schema(ViolationKey),
    /// Any other rule: its code, and the subject the rule names (an element, an id, a name).
    Rule {
        /// The code of the finding.
        code: FindingCode,
        /// What the rule is about, with no path and no offset in it.
        subject: String,
    },
}

impl Key {
    /// The key of a schema violation, with the names of the elements the compiler renames
    /// unrenamed, so that it is the same before and after the rename. The parent is the
    /// declaration `type/element`, and the subject the element or attribute.
    ///
    /// A property container (`w:rPr`, `w:pPr`) is keyed by its name and not by its declaration,
    /// so a violation in one is the same wherever in the unit the container stands: the mark's
    /// `w:rPr` (`CT_ParaRPr`), a run's (`CT_RPr`), a paragraph's `w:pPr` or the snapshot in a
    /// `w:pPrChange` (`CT_PPrExtended`), and a table's `w:tblPr` (`CT_TblPr`) or its snapshot in a
    /// `w:tblPrChange` (`CT_TblPrBase`). The compiler copies a mark's properties onto a new run
    /// and a container's into the snapshot of a change, and a copy of a defect the unit had is
    /// carried. A defect that is new (another property twice) still has a key of its own. An out-of-order violation in a property
    /// container is keyed by the container alone, without the child the validator names.
    pub(crate) fn schema(key: &ViolationKey) -> Key {
        let mut in_properties = false;
        let parent = match key.parent.rsplit_once('/') {
            Some((declared, element)) => {
                let element = unrenamed_qname(element);
                if is_property_container(&element) {
                    in_properties = true;
                    format!("{PROPERTIES}/{element}")
                } else {
                    format!("{declared}/{element}")
                }
            }
            None => key.parent.clone(),
        };
        // A property container out of the order its content model gives is one problem, whichever child the validator names:
        // a property placed into a container that is already out of order shifts which child that is, and the container was
        // out of order before the edit. A container that was in order and is not now still has a key its origin lacks.
        let subject = if in_properties && key.kind.keyed() == ViolationKind::OutOfOrder {
            String::new()
        } else {
            unrenamed_qname(&key.subject).into_owned()
        };
        Key::Schema(ViolationKey {
            kind: key.kind,
            parent,
            subject,
            extra: key.extra.clone(),
        })
    }

    /// Whether an edit adds this problem when a unit has it more times than its origin, and not only
    /// when the origin lacks it: a child of a property container that is not in the order Word
    /// writes (`schema_non_canonical_order`). Word reads any order, but the Open XML SDK validator
    /// counts every such container as a schema error, so a copy of one is a new error to it. The
    /// compiler writes every container it creates in order (`revisions::canonical`), so a unit
    /// never has more than its origin had.
    pub(crate) fn is_counted(&self) -> bool {
        matches!(self, Key::Schema(key) if key.kind == ViolationKind::NonCanonicalOrder)
    }

    /// The key of a rule about `subject`.
    pub(crate) fn rule(code: FindingCode, subject: impl Into<String>) -> Key {
        Key::Rule {
            code,
            subject: subject.into(),
        }
    }

    /// The code of a finding with this key.
    pub(crate) fn code(&self) -> FindingCode {
        match self {
            Key::Schema(key) => schema_code(key.kind),
            Key::Rule { code, .. } => *code,
        }
    }
}

/// The code of a schema violation. A surplus element and a misplaced one are one code, as they
/// are one key.
pub(crate) fn schema_code(kind: ViolationKind) -> FindingCode {
    match kind.keyed() {
        ViolationKind::WrongRoot => FindingCode::SchemaWrongRoot,
        ViolationKind::UnexpectedChild => FindingCode::SchemaUnexpectedChild,
        ViolationKind::UnknownElement => FindingCode::SchemaUnknownElement,
        ViolationKind::OutOfOrder | ViolationKind::TooMany => FindingCode::SchemaOutOfOrder,
        ViolationKind::NonCanonicalOrder => FindingCode::SchemaNonCanonicalOrder,
        ViolationKind::MissingChild => FindingCode::SchemaMissingChild,
        ViolationKind::MustUnderstand => FindingCode::SchemaMustUnderstand,
        ViolationKind::UnexpectedText => FindingCode::SchemaUnexpectedText,
        ViolationKind::UnknownAttribute => FindingCode::SchemaUnknownAttribute,
        ViolationKind::MissingAttribute => FindingCode::SchemaMissingAttribute,
        ViolationKind::InvalidAttributeValue => FindingCode::SchemaInvalidAttributeValue,
    }
}

/// A set of keys, in key order so that anything made from it is the same every time, each with
/// how many times it occurs. Membership is the test of "no new violations" for every key but a
/// counted one ([`Key::is_counted`]), whose count is the test.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeySet(BTreeMap<Key, usize>);

impl KeySet {
    /// The keys of a schema report, with their counts.
    pub(crate) fn of_schema(set: &ViolationSet) -> KeySet {
        let mut keys = KeySet::default();
        for (key, n) in set.iter() {
            keys.add(Key::schema(key), n as usize);
        }
        keys
    }

    /// Adds one occurrence of a key. True if it was not there.
    pub(crate) fn insert(&mut self, key: Key) -> bool {
        let n = self.0.entry(key).or_insert(0);
        *n += 1;
        *n == 1
    }

    /// Adds `n` occurrences of a key; none when `n` is 0.
    pub(crate) fn add(&mut self, key: Key, n: usize) {
        if n > 0 {
            *self.0.entry(key).or_insert(0) += n;
        }
    }

    /// Whether the key is in the set.
    pub(crate) fn contains(&self, key: &Key) -> bool {
        self.0.contains_key(key)
    }

    /// How many times the key occurs.
    pub(crate) fn count(&self, key: &Key) -> usize {
        self.0.get(key).copied().unwrap_or(0)
    }

    /// How many keys there are.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no keys.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The keys, in order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Key> {
        self.0.keys()
    }

    /// The keys that are here and not in `before`: the test of "no new violations" for a key
    /// that is not counted.
    pub(crate) fn added_since(&self, before: &KeySet) -> Vec<Key> {
        self.0
            .keys()
            .filter(|k| !before.contains(k))
            .cloned()
            .collect()
    }

    /// The keys of both, counts added. The origin of a block that several blocks were merged into
    /// holds what each of them held.
    pub(crate) fn union(&self, other: &KeySet) -> KeySet {
        let mut keys = self.clone();
        keys.extend(other);
        keys
    }

    /// Adds the keys of `other`, counts added.
    pub(crate) fn extend(&mut self, other: &KeySet) {
        for (key, n) in &other.0 {
            self.add(key.clone(), *n);
        }
    }
}

impl FromIterator<Key> for KeySet {
    fn from_iter<I: IntoIterator<Item = Key>>(iter: I) -> KeySet {
        let mut keys = KeySet::default();
        for key in iter {
            keys.insert(key);
        }
        keys
    }
}
