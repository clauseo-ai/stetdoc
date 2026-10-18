//! Human units for the values of `set` and the typed property operations (design `document-and-ops.md` 2.7).
//!
//! `12pt`, `0.5in`, `1.5cm`, `10mm` and `20twips` become the attribute's own unit: twips for indents, spacing and page sizes,
//! half-points for font sizes, eighths of a point for borders. Which unit an attribute has is a table here, until
//! `schema::measure_of` can say it. The table holds only attributes whose unit is fixed: not the line of
//! `w:spacing` (240ths of a line when `lineRule` is `auto`), not a width whose `w:type` decides its unit (`tblW`, `tcW`, the cell
//! margins), not the `*Chars` indents (hundredths of a character), and no percentage. A value with no unit is passed through as written,
//! and so is a value for an attribute the table does not know, so `w:val` enumerations still work, and `check` validates the
//! result under the no-new-violations rule. A value with a unit that does not parse is `invalid_payload`.

use crate::api::error::{ErrorCode, StetError};

/// The unit an attribute is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unit {
    /// Twentieths of a point.
    Twips,
    /// Halves of a point.
    HalfPoints,
    /// Eighths of a point.
    EighthPoints,
}

/// The unit of an attribute of a property element, by their local names.
pub(super) fn unit_of(element: &str, attribute: &str) -> Option<Unit> {
    match (element, attribute) {
        ("ind", "left" | "right" | "start" | "end" | "firstLine" | "hanging") => Some(Unit::Twips),
        ("spacing", "before" | "after" | "val") => Some(Unit::Twips),
        ("pgSz", "w" | "h") => Some(Unit::Twips),
        ("pgMar", "top" | "right" | "bottom" | "left" | "header" | "footer" | "gutter") => {
            Some(Unit::Twips)
        }
        ("cols", "space") | ("col", "w" | "space") | ("tab", "pos") => Some(Unit::Twips),
        ("trHeight", "val") | ("gridCol", "w") => Some(Unit::Twips),
        ("sz" | "szCs" | "kern" | "position", "val") => Some(Unit::HalfPoints),
        (
            "top" | "left" | "bottom" | "right" | "start" | "end" | "between" | "bar" | "insideH"
            | "insideV" | "bdr",
            "sz",
        ) => Some(Unit::EighthPoints),
        _ => None,
    }
}

/// A length with a unit, in points.
fn points(value: &str) -> Option<Result<f64, StetError>> {
    let v = value.trim();
    let (number, factor) = [
        ("twips", 1.0 / 20.0),
        ("pt", 1.0),
        ("in", 72.0),
        ("cm", 72.0 / 2.54),
        ("mm", 72.0 / 25.4),
    ]
    .iter()
    .find_map(|(suffix, factor)| v.strip_suffix(suffix).map(|n| (n.trim(), *factor)))?;
    Some(
        number
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| n * factor)
            .ok_or_else(|| {
                StetError::new(
                    ErrorCode::InvalidPayload,
                    format!("`{value}` is not a length. Write a number and a unit, such as 12pt, 0.5in or 1.5cm."),
                )
            }),
    )
}

/// A length with a unit, in twips: `None` when the value has no unit.
pub(super) fn twips(value: &str) -> Result<Option<i64>, StetError> {
    match points(value) {
        Some(pt) => Ok(Some((pt? * 20.0).round() as i64)),
        None => Ok(None),
    }
}

/// The value as the attribute writes it: converted when it has a unit and the attribute has one, as written otherwise.
pub(super) fn convert(element: &str, attribute: &str, value: &str) -> Result<String, StetError> {
    let Some(unit) = unit_of(element, attribute) else {
        return Ok(value.to_string());
    };
    let Some(pt) = points(value) else {
        return Ok(value.to_string());
    };
    let pt = pt?;
    let n = match unit {
        Unit::Twips => pt * 20.0,
        Unit::HalfPoints => pt * 2.0,
        Unit::EighthPoints => pt * 8.0,
    };
    Ok(format!("{}", n.round() as i64))
}

/// A colour as `w:color` writes it: `RRGGBB` in capitals, with or without `#`, or `auto`.
pub(super) fn color(value: &str) -> Result<String, StetError> {
    let v = value.trim().trim_start_matches('#');
    if v.eq_ignore_ascii_case("auto") {
        return Ok("auto".to_string());
    }
    if v.len() == 6 && v.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(v.to_ascii_uppercase());
    }
    Err(StetError::new(
        ErrorCode::InvalidPayload,
        format!("`{value}` is not a colour. Write RRGGBB, such as 1F4E79, or auto."),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_length_becomes_the_unit_of_its_attribute_and_anything_else_is_passed_through() {
        let table = [
            ("ind", "left", "0.5in", "720"),
            ("ind", "firstLine", "1.27cm", "720"),
            ("spacing", "after", "6pt", "120"),
            ("spacing", "before", "10mm", "567"),
            ("sz", "val", "12pt", "24"),
            ("top", "sz", "0.5pt", "4"),
            ("pgSz", "w", "8.5in", "12240"),
            ("ind", "left", "360twips", "360"),
            ("ind", "left", "720", "720"),
            ("jc", "val", "center", "center"),
            ("lang", "val", "12pt", "12pt"),
            // A unit that another attribute decides is not converted.
            ("spacing", "line", "14pt", "14pt"),
            ("tblW", "w", "2in", "2in"),
            ("ind", "leftChars", "1in", "1in"),
        ];
        for (element, attribute, value, expected) in table {
            assert_eq!(
                convert(element, attribute, value).unwrap(),
                expected,
                "{element} {attribute} {value}"
            );
        }
        let e = convert("ind", "left", "half an in").unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidPayload);
    }

    #[test]
    fn a_colour_is_six_hex_digits_or_auto() {
        assert_eq!(color("#1f4e79").unwrap(), "1F4E79");
        assert_eq!(color("Auto").unwrap(), "auto");
        assert_eq!(color("red").unwrap_err().code(), ErrorCode::InvalidPayload);
    }
}
