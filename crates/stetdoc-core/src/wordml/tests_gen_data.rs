//! The words the generator of `tests_gen.rs` draws from, and its random numbers.

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
}

/// Words that no mode's text may contain, whatever is generated.
pub(super) const HIDDEN: [&str; 5] = ["ZZCODEZZ", "UNTAKEN", "HIDDEN", "SKIPPED", "CODETEXT"];

/// Text as written in XML and as it reads once decoded.
pub(super) const PIECES: [(&str, &str); 19] = [
    ("abc", "abc"),
    ("Hello ", "Hello "),
    (" lead", " lead"),
    ("trail  ", "trail  "),
    (" ", " "),
    ("&amp;", "&"),
    ("&lt;b&gt;", "<b>"),
    ("&#x41;&#66;", "AB"),
    ("\u{1F600}", "\u{1F600}"),
    ("&#128512;", "\u{1F600}"),
    ("caf\u{e9}", "caf\u{e9}"),
    ("\u{10000}x", "\u{10000}x"),
    ("a\tb", "a\tb"),
    ("a\nb", "a\nb"),
    ("a\r\nb", "a\nb"),
    ("&#13;", "\r"),
    ("\u{2028}", "\u{2028}"),
    ("&#xA0;", "\u{a0}"),
    ("\u{e000}", "\u{e000}"),
];

pub(super) const CDATA_PIECES: [(&str, &str); 3] = [
    ("<![CDATA[a<b&c]]>", "a<b&c"),
    ("<![CDATA[ \u{1F600} ]]>", " \u{1F600} "),
    ("<![CDATA[x]]>", "x"),
];

/// Instructions, split at random places when written. Every one holds the word that must stay hidden.
pub(super) const INSTRS: [&str; 9] = [
    " REF ZZCODEZZ \\h ",
    " PAGE ZZCODEZZ ",
    "MACROBUTTON ZZCODEZZ Click here",
    " IF ZZCODEZZ = \"x\" \"y\" ",
    " HYPERLINK \"http://x/ZZCODEZZ\" ",
    " EQ \\o(ZZCODEZZ) ",
    " seq ZZCODEZZ ",
    " \"quote\" ZZCODEZZ ",
    " ",
];

/// The keywords for which Word shows something with no stored result (4.4).
pub(super) const SHOWN: [&str; 8] = [
    "EQ",
    "MACROBUTTON",
    "LISTNUM",
    "SYMBOL",
    "PAGE",
    "GOTOBUTTON",
    "QUOTE",
    "BARCODE",
];
