//! Exact splits and merges (design 6.4, step 6).
//!
//! An agent that wants one paragraph to be two writes a second line, and one that wants two to be one
//! joins the text and removes a line. The redline a lawyer expects for these is Word's own, a paragraph
//! mark inserted or deleted with the text unchanged, and not a deletion of text followed by the same text
//! inserted. So the alignment looks for **exact** splits and merges, and compiles them to `split` and
//! `merge`. Exact is a matter of equality and never of resemblance: any difference, a word, a tag or a
//! marker, and the lines are an ordinary edit, which is what the agent wrote.
//!
//! The pieces are compared as drafts. A split is one old paragraph `P` and `k >= 2` lines whose slots and
//! seams, put together, are `P`'s, with the whitespace that the old text had at each cut left out of the
//! lines: `P`'s slot is `X + W + Y`, where `X` ends the piece before the cut, `W` is the maximal run of
//! spaces and tabs after it and `Y` begins the next piece. The cut is **after** `W`, so the first piece keeps
//! the whitespace and the new paragraph starts with the next word. A merge is the reverse: the line is the
//! parts' slots and seams in order, with the whitespace between two parts that the first one hides at the
//! end of its line, or, for a merge of two, one run of whitespace that is then inserted at the join.

use crate::projection::draft::DraftBlock;
use crate::projection::keys::Key;
use crate::projection::model::count;

/// One cut of a split.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Cut {
    /// The text slot of the old paragraph that holds the cut.
    pub slot: usize,
    /// How many characters of that slot come before the cut: `X` and `W`.
    pub within: u32,
    /// The offset of the cut in the paragraph's `Final` text.
    pub at: u32,
    /// The piece that starts after the cut.
    pub piece: usize,
}

/// What a comparison of lines with a paragraph found.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Split {
    /// The lines are not the paragraph cut.
    No,
    /// The lines are the paragraph, cut here.
    Yes(Vec<Cut>),
    /// They are, but a cut falls next to a seam that takes no room, so the side it is on cannot be told.
    Ambiguous {
        /// The piece after the cut.
        piece: usize,
        /// The seam.
        seam: Key,
    },
}

/// The length in bytes of the run of spaces and tabs at the start of a text.
fn leading_whitespace(s: &str) -> usize {
    s.bytes().take_while(|b| *b == b' ' || *b == b'\t').count()
}

fn is_whitespace(s: &str) -> bool {
    s.bytes().all(|b| b == b' ' || b == b'\t')
}

/// Compares `pieces` with the old paragraph `p`.
pub(super) fn match_split(p: &DraftBlock, pieces: &[&DraftBlock]) -> Split {
    if pieces.len() < 2 {
        return Split::No;
    }
    let starts = p.slot_starts();
    let seam_count = p.seams.len();
    let mut cuts = Vec::new();
    let mut ambiguous: Option<(usize, Key)> = None;
    let (mut x, mut o) = (0usize, 0usize);
    for (i, piece) in pieces.iter().enumerate() {
        for (j, u) in piece.texts.iter().enumerate() {
            let Some(t) = p.texts.get(x) else {
                return Split::No;
            };
            let mut start = o;
            if i > 0 && j == 0 {
                start += leading_whitespace(t.get(o..).unwrap_or(""));
                let within = count(t.get(..start).unwrap_or(""));
                let at = starts.get(x).copied().unwrap_or(0).saturating_add(within);
                if ambiguous.is_none()
                    && let Some(seam) = ambiguous_seam(p, x, within == 0, start == t.len())
                {
                    ambiguous = Some((i, seam));
                }
                cuts.push(Cut {
                    slot: x,
                    within,
                    at,
                    piece: i,
                });
            }
            let rest = t.get(start..).unwrap_or("");
            let last_slot = j + 1 == piece.texts.len();
            if !last_slot {
                if rest != u || piece.seams.get(j).map(|s| &s.key) != p.seams.get(x).map(|s| &s.key)
                {
                    return Split::No;
                }
                x += 1;
                o = 0;
            } else if i + 1 == pieces.len() {
                if rest != u || x != seam_count {
                    return Split::No;
                }
            } else {
                if !rest.starts_with(u.as_str()) {
                    return Split::No;
                }
                o = start + u.len();
            }
        }
    }
    match ambiguous {
        Some((piece, seam)) => Split::Ambiguous { piece, seam },
        None => Split::Yes(cuts),
    }
}

/// The seam that a cut at the start (`at_start`) or the end (`at_end`) of slot `x` is next to, if it
/// takes no room in the `Final` text.
fn ambiguous_seam(p: &DraftBlock, x: usize, at_start: bool, at_end: bool) -> Option<Key> {
    if at_start
        && let Some(before) = x.checked_sub(1).and_then(|i| p.seams.get(i))
        && before.is_zero_width()
    {
        return Some(before.key.clone());
    }
    if at_end
        && let Some(after) = p.seams.get(x)
        && after.is_zero_width()
    {
        return Some(after.key.clone());
    }
    None
}

/// What a comparison of parts with a line found.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Merge {
    /// The line is not the parts joined.
    No,
    /// The line is the parts joined. For a merge of two with whitespace between them that the first part
    /// does not hide, the whitespace is to be inserted at `offset` in the first part's `Final` text.
    Yes {
        /// What to insert at the join, and where, if anything.
        insert: Option<(u32, String)>,
    },
    /// The line is the parts joined, and the whitespace between them goes at the start of the second part, because
    /// the first ends in a seam that takes no room (`{/ctl}`, `{/c2}`, `{/link}`) and the second starts with text:
    /// at the end of the first, which side of the seam the space is on could not be told, and at the start of the
    /// second text it can.
    YesInSecond {
        /// The whitespace.
        text: String,
    },
    /// It is, but the whitespace would go next to a seam that takes no room on both sides.
    Ambiguous(Key),
}

/// The width of a paragraph in its `Final` text, hidden trailing spaces included.
pub(super) fn final_width(p: &DraftBlock) -> u32 {
    let starts = p.slot_starts();
    let last = p.texts.last().map_or(0, |t| count(t));
    starts
        .last()
        .copied()
        .unwrap_or(0)
        .saturating_add(last)
        .saturating_add(p.facts.trailing)
}

/// Compares `line` with the old paragraphs `parts` joined.
pub(super) fn match_merge(parts: &[&DraftBlock], line: &DraftBlock) -> Merge {
    let Some(first) = parts.first() else {
        return Merge::No;
    };
    if parts.len() < 2 {
        return Merge::No;
    }
    let mut joins: Vec<String> = parts
        .iter()
        .take(parts.len() - 1)
        .map(|part| " ".repeat(usize::try_from(part.facts.trailing).unwrap_or(0)))
        .collect();
    let mut insert = None;
    let mut ambiguous = None;
    let mut in_second = false;
    if parts.len() == 2 && first.facts.trailing == 0 {
        // The first part hides no whitespace: whatever whitespace the line has between the parts is the
        // join, and it is inserted.
        let junction = first.seams.len();
        let (Some(a), Some(b), Some(slot)) = (
            first.texts.last(),
            parts.get(1).and_then(|p| p.texts.first()),
            line.texts.get(junction),
        ) else {
            return Merge::No;
        };
        let middle = slot
            .strip_prefix(a.as_str())
            .and_then(|r| r.strip_suffix(b.as_str()))
            .filter(|m| slot.len() >= a.len() + b.len() && is_whitespace(m));
        let Some(middle) = middle else {
            return Merge::No;
        };
        if !middle.is_empty() {
            if final_width(first) == 0 {
                return Merge::No;
            }
            if a.is_empty()
                && let Some(seam) = first.seams.last()
                && seam.is_zero_width()
            {
                if b.is_empty() {
                    ambiguous = Some(seam.key.clone());
                } else {
                    in_second = true;
                }
            }
            insert = Some((final_width(first), middle.to_string()));
        }
        joins = vec![middle.to_string()];
    }
    if joined(parts, &joins) != (line.texts.clone(), keys(line)) {
        return Merge::No;
    }
    match (ambiguous, insert) {
        (Some(seam), _) => Merge::Ambiguous(seam),
        (None, Some((_, text))) if in_second => Merge::YesInSecond { text },
        (None, insert) => Merge::Yes { insert },
    }
}

fn keys(block: &DraftBlock) -> Vec<Key> {
    block.seams.iter().map(|s| s.key.clone()).collect()
}

/// The slots and the seam keys of the parts put together, with `joins[i]` between part `i` and the next,
/// and the trailing spaces of the whole dropped, as a draft drops them.
fn joined(parts: &[&DraftBlock], joins: &[String]) -> (Vec<String>, Vec<Key>) {
    let mut texts = Vec::new();
    let mut seams = Vec::new();
    let mut current = String::new();
    for (i, part) in parts.iter().enumerate() {
        for (j, slot) in part.texts.iter().enumerate() {
            current.push_str(slot);
            if let Some(seam) = part.seams.get(j) {
                texts.push(std::mem::take(&mut current));
                seams.push(seam.key.clone());
            }
        }
        if let Some(join) = joins.get(i)
            && i + 1 < parts.len()
        {
            current.push_str(join);
        }
    }
    let kept = current.trim_end_matches(' ').len();
    current.truncate(kept);
    texts.push(current);
    (texts, seams)
}
