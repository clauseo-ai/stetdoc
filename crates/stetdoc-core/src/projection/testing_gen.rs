//! A seeded generator of projections for the laws of the module: every kind of block and inline, every
//! class of character that a format must escape, and strings long enough to be wrapped.
//!
//! The same seed gives the same model on every platform. A generated model keeps the invariants of
//! `model.rs`, and a paragraph with no ref carries no tags, because a format cannot write a tag on a
//! line that has no ref.

use super::model::*;
use crate::refs::{BlockRef, RefKind};
use std::sync::Arc;

/// A xorshift generator: the same seed gives the same sequence on every platform.
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Rng {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for _ in 0..4 {
            rng.next();
        }
        rng
    }

    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(super) fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    pub(super) fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    pub(super) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// One of a list of words.
    pub(super) fn word(&mut self, items: &[&'static str]) -> &'static str {
        items[self.below(items.len())]
    }
}

/// Pieces that strings are made of. They cover every class that a format escapes or must not confuse:
/// braces, backslash, quotes, the markers' own punctuation, a tab, a line break, a carriage return,
/// controls, format characters, a no-break space, private use, a tag character, astral characters.
pub(super) const PIECES: &[&str] = &[
    "the Seller",
    " shall pay ",
    "Price",
    " ",
    "  ",
    ". ",
    ", ",
    "(a)",
    "4.2",
    "\"",
    "'",
    "\u{201c}",
    "\u{201d}",
    "{",
    "}",
    "\\",
    "\\n",
    "{++",
    "++}",
    "{--",
    "--}",
    "--",
    "++",
    "[x]",
    "|",
    "#",
    "\t",
    "\n",
    "\r",
    "\u{0}",
    "\u{1b}",
    "\u{7f}",
    "\u{85}",
    "\u{a0}",
    "\u{ad}",
    "\u{200b}",
    "\u{200e}",
    "\u{2028}",
    "\u{3000}",
    "\u{feff}",
    "\u{f0b7}",
    "\u{e0041}",
    "\u{1f600}",
    "\u{fffe}",
    "\u{a7}",
    "\u{b6}",
    "\u{20b9}",
    "\u{928}\u{92e}",
    "p12",
    "t3 ",
    "r4",
    "c3",
    "fn1",
    "p",
    " p7 ",
    "-",
    ">",
];

/// What a generated model's names are drawn from.
pub(super) const STYLES: &[&str] = &[
    "Heading 1",
    "Title",
    "List Paragraph",
    "Body \"Text\"",
    "Quote {x}",
];
const URLS: &[&str] = &[
    "https://example.com/a?b=1&c=\"2\"",
    "mailto:a@b.c",
    "https://x.test/{y}\\z",
];
const KEYWORDS: &[&str] = &["REF", "PAGEREF", "TOC", "SEQ", "HYPERLINK"];

/// A generator of projections.
pub(super) struct Gen {
    pub(super) rng: Rng,
    pub(super) next_ref: u32,
    pub(super) next_id: i64,
    pub(super) authors: usize,
}

impl Gen {
    pub(super) fn new(seed: u64) -> Gen {
        Gen {
            rng: Rng::new(seed),
            next_ref: 1,
            next_id: 1,
            authors: 2,
        }
    }

    /// A string of one to `max` pieces.
    pub(super) fn string(&mut self, max: usize) -> String {
        let n = 1 + self.rng.below(max);
        let mut out = String::new();
        for _ in 0..n {
            out.push_str(self.rng.word(PIECES));
        }
        out
    }

    /// A string that is long, with spaces, or long with none, so that a wrap has to break a word.
    pub(super) fn long_string(&mut self) -> String {
        let n = 1200 + self.rng.below(2600);
        let mut out = String::new();
        if self.rng.chance(30) {
            while out.chars().count() < n {
                out.push_str("abcdefghij");
            }
            return out;
        }
        while out.chars().count() < n {
            out.push_str(self.rng.word(PIECES));
            if self.rng.chance(60) {
                out.push_str(" word");
            }
        }
        out
    }

    pub(super) fn words(&mut self) -> String {
        self.string(3)
    }

    pub(super) fn opt_words(&mut self) -> Option<Box<str>> {
        self.rng.chance(60).then(|| self.words().into())
    }

    pub(super) fn rev_tag(&mut self) -> RevTag {
        let id = self.next_id;
        self.next_id += 1;
        let author = u32::try_from(self.rng.below(self.authors)).unwrap();
        RevTag {
            id,
            author,
            date: self
                .rng
                .chance(50)
                .then(|| Arc::from("2026-09-30T10:00:00Z")),
            peer: None,
        }
    }

    pub(super) fn new_ref(&mut self, kind: RefKind) -> Option<BlockRef> {
        let n = self.next_ref;
        self.next_ref += 1;
        BlockRef::new(kind, n)
    }

    fn atom(&mut self) -> Atom {
        match self.rng.below(7) {
            0 => Atom::PageBreak,
            1 => Atom::ColumnBreak,
            2 => Atom::NoteRef {
                kind: *self.rng.pick(&[NoteKind::Footnote, NoteKind::Endnote]),
                ordinal: 1 + self.rng.below(9) as u32,
            },
            3 => Atom::NoteMark {
                kind: *self.rng.pick(&[NoteKind::Footnote, NoteKind::Endnote]),
            },
            4 => Atom::Object(ObjectInfo {
                kind: *self.rng.pick(&[
                    ObjectKind::Image,
                    ObjectKind::Shape,
                    ObjectKind::Chart,
                    ObjectKind::Object,
                    ObjectKind::TextBox,
                ]),
                alt: self.opt_words(),
                text_box: self.opt_words(),
            }),
            5 => Atom::Math(self.words().into()),
            _ => Atom::Auto(
                *self
                    .rng
                    .pick(&[AutoKind::Page, AutoKind::Date, AutoKind::Value]),
            ),
        }
    }

    fn field(&mut self) -> Field {
        let text: Box<str> = self.words().into();
        let target = self
            .rng
            .chance(50)
            .then(|| BlockRef::new(RefKind::Paragraph, 1 + self.rng.below(50) as u32))
            .flatten();
        Field {
            keyword: self.rng.word(KEYWORDS).into(),
            width: count(&text),
            text,
            target,
            opens: self.rng.chance(80),
            closes: self.rng.chance(80),
            dirty: self.rng.chance(10),
        }
    }

    /// Inlines at a nesting depth. A deletion holds only text and atoms.
    pub(super) fn inlines(&mut self, depth: usize) -> Vec<Inline> {
        let n = self.rng.below(6);
        let mut out = Vec::new();
        for _ in 0..n {
            let inline = self.inline(depth);
            out.push(inline);
        }
        out
    }

    fn flat(&mut self) -> Vec<Inline> {
        let mut out = vec![Inline::Text(self.words())];
        if self.rng.chance(20) {
            out.push(Inline::Atom(self.atom()));
        }
        out
    }

    fn inline(&mut self, depth: usize) -> Inline {
        let deep = depth < 3;
        match self.rng.below(100) {
            0..=39 => Inline::Text(self.string(4)),
            40..=47 => Inline::Atom(self.atom()),
            48..=55 if deep => {
                let tag = self.rev_tag();
                Inline::Rev {
                    kind: RevKind::Ins,
                    tag,
                    inlines: self.inlines(depth + 1),
                }
            }
            56..=61 => {
                let tag = self.rev_tag();
                Inline::Rev {
                    kind: RevKind::Del,
                    tag,
                    inlines: self.flat(),
                }
            }
            62..=64 => {
                let mut tag = self.rev_tag();
                tag.peer = Some(self.next_id + 100);
                Inline::Rev {
                    kind: *self.rng.pick(&[RevKind::MoveTo, RevKind::MoveFrom]),
                    tag,
                    inlines: if deep {
                        self.inlines(depth + 1)
                    } else {
                        self.flat()
                    },
                }
            }
            65..=69 if deep => Inline::Link {
                target: LinkTarget::External(self.rng.word(URLS).into()),
                inlines: self.inlines(depth + 1),
            },
            70..=74 => {
                let state = *self.rng.pick(&[
                    ControlState::Editable,
                    ControlState::Locked,
                    ControlState::DataBound,
                    ControlState::Placeholder,
                ]);
                let kind = *self.rng.pick(&super::testing::CONTROL_KINDS);
                Inline::Control {
                    info: ControlInfo {
                        kind,
                        alias: self.opt_words(),
                        tag: self.opt_words(),
                    },
                    state,
                    inlines: if state == ControlState::Editable && deep {
                        self.inlines(depth + 1)
                    } else {
                        self.flat()
                    },
                }
            }
            75..=82 => Inline::Field(self.field()),
            83..=86 => Inline::Hidden {
                reason: HiddenReason::Vanish,
                inlines: self.flat(),
            },
            87..=90 => Inline::LooksHidden {
                signal: *self
                    .rng
                    .pick(&[LooksHiddenSignal::White, LooksHiddenSignal::Tiny]),
                inlines: self.flat(),
            },
            91..=95 => {
                let id = 1 + self.rng.below(9) as i64;
                Inline::Mark(
                    *self
                        .rng
                        .pick(&[Mark::CommentStart(id), Mark::CommentEnd(id)]),
                )
            }
            _ => Inline::Text(self.string(2)),
        }
    }
}
