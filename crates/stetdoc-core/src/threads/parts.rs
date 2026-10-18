//! Which part is which: the comments part and the four parts that extend it, found through the main
//! part's relationships, and the dependency list of everything that is read from them.

use crate::api::error::StetError;
use crate::schema::PartKind;
use crate::state::{Dep, PartId, PartsView};
use crate::wordml::RelTarget;

const TRANSITIONAL_BASE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_BASE: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";

/// True if `rel_type` is the relationship type of `kind`, in the Transitional spelling or in the Strict
/// one. The extension parts have one spelling, which is also the only one a Strict file uses.
/// (`wordml` keeps the same rule for its own lists and does not export it.)
pub(crate) fn is_kind(kind: PartKind, rel_type: &str) -> bool {
    let wanted = kind.relationship_type();
    if rel_type == wanted {
        return true;
    }
    match (
        rel_type.strip_prefix(STRICT_BASE),
        wanted.strip_prefix(TRANSITIONAL_BASE),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// The parts of a document that hold comments and what extends them, each if the main part has a
/// relationship to it and the part exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommentParts {
    /// `word/comments.xml`.
    pub(crate) comments: Option<PartId>,
    /// `word/commentsExtended.xml`: threads and done flags.
    pub(crate) extended: Option<PartId>,
    /// `word/commentsIds.xml`: durable ids.
    pub(crate) ids: Option<PartId>,
    /// `word/commentsExtensible.xml`: dates in UTC.
    pub(crate) extensible: Option<PartId>,
    /// `word/people.xml`: the authors.
    pub(crate) people: Option<PartId>,
}

impl CommentParts {
    /// Finds the parts from the relationships of `main`.
    pub(crate) fn find(view: &PartsView<'_>, main: PartId) -> Result<CommentParts, StetError> {
        let rels = view.rels(main)?;
        let of = |kind: PartKind| {
            rels.iter()
                .filter(|r| is_kind(kind, &r.rel_type))
                .find_map(|r| match &r.target {
                    RelTarget::Part(name) => view.id(name),
                    _ => None,
                })
        };
        Ok(CommentParts {
            comments: of(PartKind::Comments),
            extended: of(PartKind::CommentsExtended),
            ids: of(PartKind::CommentsIds),
            extensible: of(PartKind::CommentsExtensible),
            people: of(PartKind::People),
        })
    }

    /// The parts that exist, in a fixed order.
    pub(crate) fn all(&self) -> impl Iterator<Item = PartId> {
        [
            self.comments,
            self.extended,
            self.ids,
            self.extensible,
            self.people,
        ]
        .into_iter()
        .flatten()
    }

    /// What a value read from these parts depends on: the set of parts, since a part that is absent
    /// now may be added, and each part that exists.
    pub(crate) fn deps(&self) -> Vec<Dep> {
        std::iter::once(Dep::Package)
            .chain(self.all().map(Dep::Part))
            .collect()
    }
}

/// The package paths that a part of each kind usually has, for `audit` to find a part that exists
/// but that no relationship names.
pub(crate) fn default_paths() -> [(PartKind, &'static str); 5] {
    [
        PartKind::Comments,
        PartKind::CommentsExtended,
        PartKind::CommentsIds,
        PartKind::CommentsExtensible,
        PartKind::People,
    ]
    .map(|k| (k, k.default_path()))
}
