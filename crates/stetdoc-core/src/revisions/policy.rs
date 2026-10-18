//! Whose revision is whose, and what counts as a claim (design `revisions.md`, 2.2: "own pending"
//! and "created by this transaction", and the copy rule).
//!
//! The rules that need the document, such as the owner of an item inside a wrapper, wait for
//! `wordml`. What is here needs only a stamp and ids.

use super::stamp::RevStamp;
use std::ops::Range;

/// The local names, in the `w` namespace, of the change-tracking elements. The copy rule strips these
/// from every copy of properties, a run or a row, so that a copy never carries someone else's claim:
/// the revisions of a paragraph mark, of a row (`w:trPr`) and of a list (`w:numPr`), and every
/// `*PrChange`, the cell revisions and the numbering change.
pub(crate) const CHANGE_TRACKING: &[&str] = &[
    "ins",
    "del",
    "moveFrom",
    "moveTo",
    "rPrChange",
    "pPrChange",
    "sectPrChange",
    "tblPrChange",
    "tblPrExChange",
    "tblGridChange",
    "trPrChange",
    "tcPrChange",
    "numberingChange",
    "cellIns",
    "cellDel",
    "cellMerge",
];

/// Whether an element of the `w` namespace with this local name is a change-tracking element.
pub(crate) fn is_change_tracking(local_name: &str) -> bool {
    CHANGE_TRACKING.contains(&local_name)
}

/// Whether a revision is **own pending**: its author is the stamp's, byte for byte. "The same author"
/// is that and nothing more, because it is all the file holds. Own pending text may be removed
/// outright, and an own pending property change may be folded into. A revision with no author is
/// nobody's.
pub(crate) fn is_own_pending(author: Option<&str>, stamp: &RevStamp) -> bool {
    author == Some(stamp.author())
}

/// Whether a revision was **created by this transaction**: its author is the stamp's, its id is in
/// the range that the transaction minted, and the id is not one that the transaction noted as a copy
/// (`Tx::copied_annotation_ids`). A split's tail and a carve's copy of a pending change take minted ids
/// too, but they are pieces of a revision that already existed, so only what passes this test may be
/// extended or edited in place.
pub(crate) fn is_created(
    author: Option<&str>,
    id: i64,
    stamp: &RevStamp,
    minted: &Range<u32>,
    copies: &[u32],
) -> bool {
    is_own_pending(author, stamp)
        && u32::try_from(id).is_ok_and(|id| minted.contains(&id) && !copies.contains(&id))
}
