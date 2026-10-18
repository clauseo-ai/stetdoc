//! A seeded generator of random edit batches, built from a document's public view output only.
//!
//! [`Facts::of`] reads what an agent could read: the `json/1` view of the body, the notes and the headers (refs, the Final text
//! that offsets count in, list labels, read-only flags, tables and their rows, revision ids, authors), and the appendix of the
//! body view (comment ids and their authors, note ids). [`generate`] makes a batch from those facts with a [`Rng`]: one to four
//! operations of the nineteen, valid payloads and edge ones (an empty range, an offset at either end, an emoji, a tab, a line
//! break, quotes, a long text, a merge with no neighbour, a table where a paragraph is wanted), tracked or not, and now and then
//! an operation that must be refused, which carries the codes it may be refused with. The same seed and the same facts always
//! give the same batch, so a failure reproduces from its seed.

pub use super::facts::{Facts, Para, Table};
use stetdoc_core::{ErrorCode, OPS};

/// A small deterministic random number generator (SplitMix64). The testkit takes no dependency for it.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// A generator from a seed.
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, or 0 when `n` is 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// True `percent` times in a hundred.
    pub fn chance(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    /// One of the items, or `None` when there are none.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        items.get(self.below(items.len()))
    }
}

/// A hash of bytes (FNV-1a), to give each document its own stream from the run's seed.
pub fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// One generated operation.
#[derive(Clone, Debug)]
pub struct GenOp {
    /// The operation's name.
    pub name: &'static str,
    /// The operation, as JSON.
    pub json: String,
    /// For an operation that must be refused, the codes it may be refused with.
    pub refused: Option<&'static [ErrorCode]>,
    /// For a whole-paragraph `replace` or an `insert` of plain text, what the Final text of the paragraphs must be.
    pub sticks: Option<(String, Vec<String>)>,
}

/// One generated batch.
#[derive(Clone, Debug)]
pub struct Generated {
    /// The operations, in order.
    pub ops: Vec<GenOp>,
    /// Whether the edit is tracked.
    pub tracked: bool,
}

impl Generated {
    /// The operations as the `ops` of a JSON batch, without the brackets.
    pub fn json(&self) -> String {
        self.ops
            .iter()
            .map(|o| o.json.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The index of the first operation that must be refused.
    pub fn first_refused(&self) -> Option<usize> {
        self.ops.iter().position(|o| o.refused.is_some())
    }

    /// Whether every operation is reversible by the table of operations, so that rejecting the author's revisions must give back
    /// the original (L5).
    pub fn reversible(&self) -> bool {
        self.ops
            .iter()
            .all(|o| OPS.iter().any(|s| s.name == o.name && s.reversible))
    }
}

impl std::fmt::Display for Generated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let how = if self.tracked { "tracked" } else { "untracked" };
        write!(f, "[{}] ({how})", self.json())
    }
}

/// The names the generator has a generator for. A test checks it against `capabilities().ops`.
pub const NAMES: &[&str] = &[
    "replace",
    "insert",
    "delete",
    "split",
    "merge",
    "format",
    "paragraph",
    "style",
    "list",
    "set",
    "row",
    "comment",
    "reply",
    "resolve",
    "edit-comment",
    "delete-comment",
    "delete-note",
    "accept",
    "reject",
];

/// A random batch from the facts: one operation half of the time, else two to four; tracked seven times in ten.
pub fn generate(rng: &mut Rng, facts: &Facts) -> Generated {
    let n = if rng.chance(50) { 1 } else { 2 + rng.below(3) };
    let ops = (0..n).map(|_| operation(rng, facts, None)).collect();
    Generated {
        ops,
        tracked: rng.chance(70),
    }
}

/// One random operation, of the given name or of any.
pub fn operation(rng: &mut Rng, facts: &Facts, name: Option<&'static str>) -> GenOp {
    let name = name.unwrap_or_else(|| *rng.pick(NAMES).unwrap_or(&"replace"));
    let mut g = Gen {
        rng,
        facts,
        fields: Vec::new(),
        refused: None,
        sticks: None,
    };
    g.fields.push(format!(r#""op":{}"#, q(name)));
    super::generate_ops::fill(&mut g, name);
    // Now and then a field no operation has, which must be refused.
    if g.refused.is_none() && g.rng.chance(2) {
        g.fields.push(r#""colour":"red""#.to_string());
        g.refused = Some(PAYLOAD);
    }
    GenOp {
        name,
        json: format!("{{{}}}", g.fields.join(",")),
        refused: g.refused,
        sticks: g.sticks.take(),
    }
}

/// The codes of a malformed payload.
pub const PAYLOAD: &[ErrorCode] = &[ErrorCode::InvalidPayload];
/// The codes of a target that does not exist.
pub const MISSING: &[ErrorCode] = &[
    ErrorCode::NotFound,
    ErrorCode::StaleRef,
    ErrorCode::InvalidPayload,
];

/// The state of one operation being made.
pub struct Gen<'a> {
    /// The random stream.
    pub rng: &'a mut Rng,
    /// What the view says.
    pub facts: &'a Facts,
    /// The JSON fields so far, `"key":value`.
    pub fields: Vec<String>,
    /// Set when the operation must be refused.
    pub refused: Option<&'static [ErrorCode]>,
    /// What a plain edit must leave, for L3.
    pub sticks: Option<(String, Vec<String>)>,
}

impl Gen<'_> {
    /// Adds a field, unless the operation has one with that key already.
    pub fn field(&mut self, key: &str, value: impl Into<String>) {
        self.try_field(key, value);
    }

    /// Adds a field, unless the operation has one with that key already. Returns whether it was added.
    pub fn try_field(&mut self, key: &str, value: impl Into<String>) -> bool {
        let key = q(key);
        if self
            .fields
            .iter()
            .any(|f| f.starts_with(&format!("{key}:")))
        {
            return false;
        }
        self.fields.push(format!("{key}:{}", value.into()));
        true
    }

    /// Marks the operation as one that must be refused with one of `codes`.
    pub fn refuse(&mut self, codes: &'static [ErrorCode]) {
        self.refused.get_or_insert(codes);
    }

    /// A paragraph to edit: mostly a body paragraph with text, sometimes any.
    pub fn para(&mut self) -> Option<Para> {
        let facts = self.facts;
        let texty: Vec<&Para> = facts
            .paras
            .iter()
            .filter(|p| p.story == "body" && !p.text.is_empty())
            .collect();
        if !texty.is_empty() && self.rng.chance(80) {
            return self.rng.pick(&texty).map(|p| (*p).clone());
        }
        self.rng.pick(&facts.paras).cloned()
    }

    /// A ref to a paragraph, or one that does not exist (which must be refused).
    pub fn para_ref(&mut self) -> Option<Para> {
        if self.rng.chance(3) || self.facts.paras.is_empty() {
            self.field("ref", q("p999999"));
            self.refuse(MISSING);
            return None;
        }
        let p = self.para()?;
        self.field("ref", q(&p.r));
        Some(p)
    }

    /// A range inside the paragraph: none, a `find`, or offsets, with the edges and a few that must be refused.
    pub fn within(&mut self, p: &Para) -> bool {
        let len = p.text.len();
        match self.rng.below(10) {
            0..=3 => false,
            4..=6 => match word(self.rng, &p.text) {
                Some(w) => {
                    self.field("find", q(&w));
                    if self.rng.chance(15) {
                        let n = 1 + self.rng.below(3);
                        self.field("occurrence", n.to_string());
                    }
                    if self.rng.chance(3) {
                        self.field("start", "0");
                        self.refuse(PAYLOAD);
                    }
                    true
                }
                None => false,
            },
            7 if self.rng.chance(20) => {
                self.field("find", q(""));
                self.refuse(PAYLOAD);
                true
            }
            _ => {
                let (mut a, mut b) = (self.rng.below(len + 1), self.rng.below(len + 1));
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                match self.rng.below(8) {
                    0 => a = 0,
                    1 => b = len,
                    2 => b = a,
                    3 if self.rng.chance(30) => {
                        b = len + 5;
                        self.refuse(MISSING);
                    }
                    _ => {}
                }
                self.field("start", a.to_string());
                self.field("end", b.to_string());
                true
            }
        }
    }
}

/// A JSON string.
pub fn q(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

/// A word of three letters or more from the text, if it has one.
pub fn word(rng: &mut Rng, text: &[char]) -> Option<String> {
    let s: String = text.iter().collect();
    let words: Vec<&str> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .collect();
    rng.pick(&words).map(|w| (*w).to_string())
}

/// A text to write: plain words mostly, and the edges (quotes, an emoji, accents, a tab, a line break, spaces, a long text).
pub fn text(rng: &mut Rng) -> String {
    const WORDS: &[&str] = &[
        "the",
        "Supplier",
        "shall",
        "indemnify",
        "Customer",
        "within",
        "thirty",
        "days",
        "notice",
        "Agreement",
        "terminate",
        "liability",
        "clause",
        "stetdoc",
    ];
    let n = 1 + rng.below(8);
    let mut words: Vec<String> = (0..n)
        .map(|_| (*rng.pick(WORDS).unwrap_or(&"word")).to_string())
        .collect();
    match rng.below(14) {
        0 => words.push("\"Losses\"".to_string()),
        1 => words.push("\u{201c}Losses\u{201d} and the Supplier\u{2019}s".to_string()),
        2 => words.push("\u{1f600}".to_string()),
        3 => words.push("\u{dc}n\u{ef}c\u{f6}d\u{e9}".to_string()),
        4 => words.push("a\ttab".to_string()),
        5 => words.push("a\nbreak".to_string()),
        6 => words.push(" ".to_string()),
        7 => {
            words = (0..300)
                .map(|i| WORDS[i % WORDS.len()].to_string())
                .collect()
        }
        8 => words.push("{braces} and \\ backslash".to_string()),
        _ => {}
    }
    words.join(" ")
}

/// The character of a note's own number (`w:footnoteRef`, `w:endnoteRef`) in the Final text.
const NOTE_MARK: char = '\u{3}';

/// The Final text that a whole-paragraph `replace` of `old` with `with` leaves: `with`, and every note's own number of `old`
/// that `with` lacks put back, before `with` when it opened `old` and after it otherwise. A text edit never deletes a note's
/// own number (design `revisions.md`, 3.6.4: a note goes only with its reference), as `replace_text` does it.
pub fn kept_note_marks(old: &str, with: &str) -> String {
    let marks = old.chars().filter(|c| *c == NOTE_MARK).count();
    let have = with.chars().filter(|c| *c == NOTE_MARK).count();
    if marks <= have {
        return with.to_string();
    }
    let lead = old
        .chars()
        .take_while(|c| *c == NOTE_MARK)
        .count()
        .min(marks - have);
    let trail = marks - have - lead;
    let mut out: String = std::iter::repeat_n(NOTE_MARK, lead).collect();
    out.push_str(with);
    out.extend(std::iter::repeat_n(NOTE_MARK, trail));
    out
}

/// Whether a text is plain enough that the Final text must be exactly it (no quote that the quote policy may curl, no line
/// break or tab, no space at either end or twice).
pub fn plain(text: &str) -> bool {
    !text.contains(['"', '\'', '\u{201c}', '\u{201d}', '\u{2019}', '\n', '\t'])
        && text.trim() == text
        && !text.contains("  ")
        && !text.is_empty()
}
