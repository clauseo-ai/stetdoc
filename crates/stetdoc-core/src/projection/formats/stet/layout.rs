//! Lines: one paragraph, one logical line, wrapped under 2,000 characters (design 5.3).
//!
//! A line that reaches 2,000 characters is cut by some tools, and an agent that sees a cut line and
//! rewrites it loses the rest without knowing. So the renderer breaks a long logical line, and writes
//! each break as a space-free continuation: the physical line ends in an odd number of backslashes, and
//! the reader removes the last backslash and the line feed. The reader joins continuations before it
//! looks at anything else, so a break may fall anywhere a break is allowed, inside a marker's text too.
//!
//! The renderer breaks at the last space at or before column 1,498, but not before column 1,000, and
//! otherwise at the last place a break is allowed at or before 1,498. A break is allowed between the
//! units of a text (a character or an escape) and between the pieces of a marker, never inside a
//! delimiter, a keyword, an id or an escape. A line of at most 1,498 characters is written whole.

use super::escape::escape_char;

/// The longest physical line before the continuation backslash, in characters.
pub(super) const WRAP_AT: usize = 1498;
/// The first column at which a break after a space is preferred to a break later in the window.
pub(super) const WRAP_FROM: usize = 1000;

/// A place where a break is allowed.
#[derive(Clone, Copy, Debug)]
struct Brk {
    byte: usize,
    chars: usize,
    after_space: bool,
    /// For a break inside a text slot: the slot and how many of its characters come before the break.
    slot: Option<(u32, u32)>,
}

/// A logical line being built, with the places it may be broken.
pub(super) struct LineBuf {
    text: String,
    chars: usize,
    record: bool,
    breaks: Vec<Brk>,
}

/// Where a break fell inside a text slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WrapAt {
    /// The slot that the continuation starts in.
    pub slot: u32,
    /// How many characters of the slot come before it.
    pub at: u32,
}

impl LineBuf {
    /// An empty line. With `record` it keeps the places a break may fall, which it need only do for a
    /// line that turns out to be long.
    pub(super) fn new(record: bool) -> LineBuf {
        LineBuf {
            text: String::new(),
            chars: 0,
            record,
            breaks: Vec::new(),
        }
    }

    /// The text so far.
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// The characters so far.
    pub(super) fn chars(&self) -> usize {
        self.chars
    }

    fn mark(&mut self, after_space: bool, slot: Option<(u32, u32)>) {
        if self.record {
            self.breaks.push(Brk {
                byte: self.text.len(),
                chars: self.chars,
                after_space,
                slot,
            });
        }
    }

    /// Writes a piece that is never broken inside: a delimiter, a keyword, an id. A break is allowed
    /// before it, which is the break after whatever came last.
    pub(super) fn atomic(&mut self, piece: &str) {
        self.mark(false, None);
        self.text.push_str(piece);
        self.chars += piece.chars().count();
    }

    /// Writes escaped text, one unit at a time, with a break allowed between units. With a slot the
    /// breaks say where in the slot they fall.
    pub(super) fn content(&mut self, s: &str, quoted: bool, slot: Option<u32>) {
        let mut within = 0u32;
        let mut unit = String::new();
        for c in s.chars() {
            unit.clear();
            escape_char(c, quoted, &mut unit);
            self.text.push_str(&unit);
            self.chars += unit.chars().count();
            within = within.saturating_add(1);
            if self.record {
                self.mark(c == ' ', slot.map(|slot| (slot, within)));
            }
        }
    }

    /// Writes a quoted string: the quotes are atomic and the inside is content.
    pub(super) fn quoted(&mut self, s: &str) {
        self.atomic("\"");
        self.content(s, true, None);
        self.atomic("\"");
    }

    /// Breaks the line into physical lines, and says where each break fell inside a text slot. The
    /// lines end in a backslash except the last.
    pub(super) fn wrap(&self) -> (Vec<String>, Vec<WrapAt>) {
        if !self.record || self.chars <= WRAP_AT {
            return (vec![self.text.clone()], Vec::new());
        }
        let mut lines = Vec::new();
        let mut wraps = Vec::new();
        let mut start_byte = 0usize;
        let mut start_chars = 0usize;
        loop {
            let rest = self.text.get(start_byte..).unwrap_or("");
            if self.chars - start_chars <= WRAP_AT {
                lines.push(rest.to_string());
                break;
            }
            let Some(brk) = self.choose(start_chars) else {
                lines.push(rest.to_string());
                break;
            };
            let mut line = self
                .text
                .get(start_byte..brk.byte)
                .unwrap_or("")
                .to_string();
            line.push('\\');
            lines.push(line);
            if let Some((slot, at)) = brk.slot {
                wraps.push(WrapAt { slot, at });
            }
            start_byte = brk.byte;
            start_chars = brk.chars;
        }
        (lines, wraps)
    }

    /// The break for a line that starts at character `start`.
    fn choose(&self, start: usize) -> Option<Brk> {
        let hi = start + WRAP_AT;
        let lo = start + WRAP_FROM;
        let end = self.breaks.partition_point(|b| b.chars <= hi);
        let window = self.breaks.get(..end)?;
        let from = window.partition_point(|b| b.chars <= start);
        let eligible = window.get(from..)?;
        let spaced = eligible
            .iter()
            .rev()
            .take_while(|b| b.chars >= lo)
            .find(|b| {
                b.after_space
                    && !self
                        .text
                        .get(b.byte..)
                        .is_some_and(|rest| rest.starts_with(' '))
            });
        spaced
            .or_else(|| eligible.last())
            .copied()
            .or_else(|| self.breaks.get(end).copied())
    }
}
