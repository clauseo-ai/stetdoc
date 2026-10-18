//! The words of a finding, and the report that collects them.

use crate::api::check::{CheckReport, Class, Finding, FindingCode};
use crate::api::error::StetError;
use crate::schema::{Violation, ViolationKind};
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::collections::BTreeMap;

/// What a schema violation says, in words that name the elements: the validator's own detail
/// names the declaration (`w:CT_Text/w:t`) and, for an attribute, not even that. The violation is
/// at an element, the child that is out of place for a problem of order, the element that has the
/// attribute for a problem of an attribute.
pub(crate) fn schema_message(tree: &SpanTree, v: &Violation<NodeId>) -> String {
    let node = tree.node_ref(v.node);
    let own = node.name().to_string();
    let parent = node
        .parent()
        .filter(NodeRef::is_element)
        .map_or_else(|| "the document".to_string(), |p| p.name().to_string());
    let subject = &v.key.subject;
    match v.kind {
        ViolationKind::UnknownAttribute => format!("{subject} is not an attribute of {own}"),
        ViolationKind::MissingAttribute => format!("{own} needs the attribute {subject}"),
        ViolationKind::InvalidAttributeValue => format!(
            "{subject} of {own} is {:?}, which is not a valid value",
            v.key.extra
        ),
        ViolationKind::UnexpectedChild => format!("{own} is not allowed in {parent}"),
        ViolationKind::UnknownElement => format!(
            "{own} is in a namespace that the schema does not know, and the part does not list it as ignorable"
        ),
        ViolationKind::OutOfOrder => format!("{own} is out of order in {parent}"),
        ViolationKind::TooMany => {
            format!("{own} is a second copy of an element that may occur once in {parent}")
        }
        ViolationKind::NonCanonicalOrder => {
            format!("{own} is not in the order Word writes in {parent}")
        }
        ViolationKind::MissingChild => match v.detail.rsplit_once("required ") {
            Some((_, name)) => format!("{own} ends before a required {name}"),
            None => v.detail.clone(),
        },
        ViolationKind::UnexpectedText => {
            format!("{own} holds text, and only elements are allowed in it")
        }
        ViolationKind::MustUnderstand | ViolationKind::WrongRoot => v.detail.clone(),
    }
}

/// The sentence for a finding of a rule about `subject`. The subject of a rule that reads a whole
/// part is `part#thing`, so the sentence can name both.
pub(crate) fn sentence(code: FindingCode, subject: &str) -> String {
    let (part, thing) = subject.split_once('#').unwrap_or(("", subject));
    let in_part = if part.is_empty() {
        String::new()
    } else {
        format!(" in {part}")
    };
    match code {
        FindingCode::TextInDeletion => format!(
            "w:{thing} inside a w:del: Word writes the text of a deletion as w:delText and w:delInstrText"
        ),
        FindingCode::DeltextOutsideDeletion => format!("w:{thing} with no w:del around it"),
        FindingCode::EdgeSpaceWithoutPreserve => {
            format!(
                "w:{thing} starts or ends with white space and has no xml:space=\"preserve\": Word shows the white space, LibreOffice and other readers drop it"
            )
        }
        FindingCode::RevisionIdDuplicate => format!("two tracked changes have the id {thing}"),
        FindingCode::BookmarkIdDuplicate => {
            format!("two bookmarks{in_part} have the id {thing}")
        }
        FindingCode::RevisionNestedInItself => {
            format!("a w:{thing} inside a w:{thing} in one paragraph")
        }
        FindingCode::BookmarkUnpaired => {
            format!("bookmark {thing}{in_part} has a start and no end, or an end and no start")
        }
        FindingCode::MoveRangeUnpaired => {
            format!("move range {thing}{in_part} has a start and no end, or an end and no start")
        }
        FindingCode::MoveNameUnpaired => format!(
            "move {thing}{in_part} has a source and no destination, or a destination and no source"
        ),
        FindingCode::FieldSplitByDeletion => "a field is partly inside a deletion".to_string(),
        FindingCode::CellWithoutFinalParagraph => {
            "a table cell does not end with a paragraph".to_string()
        }
        FindingCode::TableWithoutRow => "a table has no row".to_string(),
        FindingCode::RowWithoutCell => "a table row has no cell".to_string(),
        FindingCode::ContainerWithoutBlock => {
            format!("a w:{thing} holds no paragraph and no table")
        }
        FindingCode::EmptyRevision => format!("an empty w:{thing}"),
        FindingCode::EmptyProperties => format!("an empty w:{thing}"),
        FindingCode::PartWithoutContentType => format!("{thing} has no content type"),
        FindingCode::RelationshipTargetMissing => {
            format!("relationship {thing}{in_part} names no part")
        }
        FindingCode::RelationshipIdDuplicate => {
            format!("two relationships{in_part} have the id {thing}")
        }
        other => other.description().to_string(),
    }
}

/// Collects findings into a [`CheckReport`]. Counts are kept for every finding, and the findings
/// are listed up to the limit of the report, violations and notes separately, so that a document
/// with a million notes cannot crowd the violations out.
pub(crate) struct Builder {
    violations: Vec<Finding>,
    notes: Vec<Finding>,
    counts: BTreeMap<FindingCode, usize>,
    parts: usize,
    unchecked: usize,
}

impl Builder {
    /// An empty report.
    pub(crate) fn new() -> Builder {
        Builder {
            violations: Vec::new(),
            notes: Vec::new(),
            counts: BTreeMap::new(),
            parts: 0,
            unchecked: 0,
        }
    }

    /// A part was examined.
    pub(crate) fn examined(&mut self) {
        self.parts += 1;
    }

    /// `n` parts were left alone because the check has no schema for them.
    pub(crate) fn skipped(&mut self, n: usize) {
        self.unchecked += n;
    }

    /// A part could not be read.
    pub(crate) fn unreadable(&mut self, part: &str, why: &StetError) {
        self.unchecked += 1;
        self.add(Finding {
            code: FindingCode::PartUnreadable,
            class: Class::Note,
            part: part.to_string(),
            block: None,
            message: format!(
                "the part could not be read, so it was not checked: {}",
                why.message()
            ),
        });
    }

    /// Whether another finding of this class would be listed. A caller that has to work to
    /// build a finding asks first.
    pub(crate) fn has_room(&self, class: Class) -> bool {
        match class {
            Class::Violation => self.violations.len() < CheckReport::MAX_FINDINGS,
            Class::Note => self.notes.len() < CheckReport::MAX_FINDINGS,
        }
    }

    /// Counts `n` findings of a code, without listing them.
    pub(crate) fn count(&mut self, code: FindingCode, n: usize) {
        if n > 0 {
            *self.counts.entry(code).or_default() += n;
        }
    }

    /// Lists a finding, if there is room, without counting it.
    pub(crate) fn list(&mut self, finding: Finding) {
        if self.has_room(finding.class) {
            match finding.class {
                Class::Violation => self.violations.push(finding),
                Class::Note => self.notes.push(finding),
            }
        }
    }

    /// Counts a finding and lists it.
    pub(crate) fn add(&mut self, finding: Finding) {
        self.count(finding.code, 1);
        self.list(finding);
    }

    /// The report.
    pub(crate) fn finish(self) -> CheckReport {
        let total = |class: Class| {
            self.counts
                .iter()
                .filter(|(code, _)| code.class() == class)
                .map(|(_, n)| *n)
                .sum::<usize>()
        };
        let (violations, notes) = (total(Class::Violation), total(Class::Note));
        let mut findings = self.violations;
        let room = CheckReport::MAX_FINDINGS.saturating_sub(findings.len());
        findings.extend(self.notes.into_iter().take(room));
        CheckReport {
            ok: violations == 0,
            violations,
            notes,
            findings,
            by_code: self.counts.into_iter().collect(),
            parts: self.parts,
            unchecked: self.unchecked,
        }
    }
}
