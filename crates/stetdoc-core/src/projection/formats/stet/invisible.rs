//! The characters that must not appear raw in a file (design 5.4).
//!
//! A line-based text file, and a person reading one, cannot see these for what they are: the controls,
//! the format characters (zero width, bidirectional marks, tag characters), private use (a symbol
//! font's bullet), every space-like character but U+0020, the line and paragraph separators and the
//! noncharacters. A text that holds one is written with `\u{..}`, so that an agent copies a visible
//! token and never retypes a no-break space as a space, and so that a hidden instruction smuggled in
//! tag characters is plain to the agent and to a person.

/// The ranges, inclusive, sorted. One table, no dependency.
const RANGES: &[(u32, u32)] = &[
    (0x0000, 0x001f),   // C0 controls, tab and line feed among them
    (0x007f, 0x009f),   // DEL and the C1 controls
    (0x00a0, 0x00a0),   // no-break space
    (0x00ad, 0x00ad),   // soft hyphen
    (0x0600, 0x0605),   // Arabic number signs
    (0x061c, 0x061c),   // Arabic letter mark
    (0x06dd, 0x06dd),   // Arabic end of ayah
    (0x070f, 0x070f),   // Syriac abbreviation mark
    (0x0890, 0x0891),   // Arabic pound and piastre marks above
    (0x08e2, 0x08e2),   // Arabic disputed end of ayah
    (0x1680, 0x1680),   // Ogham space mark
    (0x180e, 0x180e),   // Mongolian vowel separator
    (0x2000, 0x200f),   // en and em spaces, zero width space, joiners, directional marks
    (0x2028, 0x202f),   // line and paragraph separators, embeddings, narrow no-break space
    (0x205f, 0x206f),   // medium mathematical space, word joiner, invisible operators, isolates
    (0x3000, 0x3000),   // ideographic space
    (0xd800, 0xdfff),   // surrogates (never in a Rust string; here for completeness)
    (0xe000, 0xf8ff),   // private use
    (0xfdd0, 0xfdef),   // noncharacters
    (0xfeff, 0xfeff),   // byte order mark, zero width no-break space
    (0xfff9, 0xfffb),   // interlinear annotation characters
    (0x110bd, 0x110bd), // Kaithi number sign
    (0x110cd, 0x110cd), // Kaithi number sign above
    (0x13430, 0x1343f), // Egyptian hieroglyph format controls
    (0x1bca0, 0x1bca3), // shorthand format controls
    (0x1d173, 0x1d17a), // musical symbol format controls
    (0xe0000, 0xe007f), // tag characters
];

/// True if the character must be written as an escape.
pub(super) fn must_escape(c: char) -> bool {
    let code = c as u32;
    if code & 0xfffe == 0xfffe {
        return true; // U+FFFE and U+FFFF of every plane
    }
    if (0xf0000..=0x10ffff).contains(&code) {
        return true; // planes 15 and 16: private use
    }
    RANGES
        .binary_search_by(|&(lo, hi)| {
            if code < lo {
                std::cmp::Ordering::Greater
            } else if code > hi {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}
