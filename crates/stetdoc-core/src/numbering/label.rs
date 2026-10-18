//! The text of a level: `lvlText` with its placeholders filled in.
//!
//! `%1` to `%9` name the counter of one-based level `k`, `%%` is a percent sign, and any other
//! `%` is a literal percent sign. See the design, section 4.4.

/// One piece of a level's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Piece {
    /// Text copied as it is.
    Literal(String),
    /// The counter of this level, zero-based.
    Level(u8),
}

/// Splits `text` into literal pieces and placeholders.
pub(super) fn parse(text: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut literal = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            literal.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                chars.next();
                literal.push('%');
            }
            Some(d @ '1'..='9') => {
                chars.next();
                if !literal.is_empty() {
                    pieces.push(Piece::Literal(std::mem::take(&mut literal)));
                }
                let level = d.to_digit(10).and_then(|v| u8::try_from(v - 1).ok());
                pieces.push(Piece::Level(level.unwrap_or(0)));
            }
            _ => literal.push('%'),
        }
    }
    if !literal.is_empty() {
        pieces.push(Piece::Literal(literal));
    }
    pieces
}

/// Fills in `text`. `value(j)` gives the formatted counter of zero-based level `j`, or an empty
/// string for a level that must render nothing. The result is trimmed at both ends.
pub(super) fn expand(text: &str, mut value: impl FnMut(u8) -> String) -> String {
    let mut out = String::new();
    for piece in parse(text) {
        match piece {
            Piece::Literal(l) => out.push_str(&l),
            Piece::Level(j) => out.push_str(&value(j)),
        }
    }
    out.trim().to_string()
}

/// True if `text` names every level from the first to the one before `level` (zero-based), so its
/// label already carries its ancestors, as `%1.%2.%3` does at level 2.
pub(super) fn contains_ancestors(text: &str, level: u8) -> bool {
    let pieces = parse(text);
    (0..level).all(|j| pieces.contains(&Piece::Level(j)))
}

/// The label without its trailing periods and spaces, as Word's cross-references show it.
pub(super) fn strip_trailing_periods(label: &str) -> &str {
    label.trim_end_matches(['.', ' ', '\t'])
}
