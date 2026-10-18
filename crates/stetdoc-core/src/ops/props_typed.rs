//! What the typed fields of `format` and `paragraph` write: the elements Word's own interface writes for each (design
//! `document-and-ops.md` 2.1).
//!
//! | Field | Element |
//! |---|---|
//! | `bold`, `italic`, `strike` | `w:b`, `w:i`, `w:strike`, with `w:val="0"` for false |
//! | `underline` | `w:u w:val` |
//! | `color`, `highlight` | `w:color w:val` (`RRGGBB` or `auto`), `w:highlight w:val` |
//! | `size` | `w:sz` and `w:szCs`, in half-points; a bare number is points |
//! | `font` | `w:rFonts` `w:ascii`, `w:hAnsi` and `w:cs` |
//! | `char_style` | `w:rStyle`, the id of a character style found by id or name |
//! | `align` | `w:jc`: `left`, `center`, `right`, and `both` for `justify` |
//! | `indent_left`, `indent_right`, `first_line`, `hanging` | `w:ind`, merged with the attributes the paragraph has; `first_line` and `hanging` exclude each other |
//! | `space_before`, `space_after`, `line_spacing` | `w:spacing`, merged the same way; a bare `line_spacing` is a multiple of single (`lineRule="auto"`), one with a unit is exact |
//! | `keep_next`, `keep_lines`, `page_break_before` | `w:keepNext`, `w:keepLines`, `w:pageBreakBefore` |
//!
//! `clear` takes the field names, or element names, and removes the elements.

use super::props::{element, invalid, style_id};
use super::units;
use crate::api::error::StetError;
use crate::api::op::{FormatOp, ParagraphOp};
use crate::revisions::types::{PropEdit, PropElement};
use crate::state::Tx;
use crate::wordml::{BlockKey, StyleKind};

/// An on-off property: no `w:val` when on, `w:val="0"` when off.
fn toggle(ns: &str, local: &str, on: bool) -> PropElement {
    if on {
        element(ns, local, &[])
    } else {
        element(ns, local, &[("val", "0".to_string())])
    }
}

/// The elements that `clear` names, by field name or by element name.
fn cleared(ns: &str, names: &[String], fields: &[(&str, &[&str])]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for name in names {
        let name = name.trim().trim_start_matches("w:");
        let locals: Vec<&str> = fields
            .iter()
            .find(|(field, _)| *field == name)
            .map_or_else(|| vec![name], |(_, locals)| locals.to_vec());
        for local in locals {
            let pair = (ns.to_string(), local.to_string());
            if !out.contains(&pair) {
                out.push(pair);
            }
        }
    }
    out
}

const RUN_FIELDS: &[(&str, &[&str])] = &[
    ("bold", &["b", "bCs"]),
    ("italic", &["i", "iCs"]),
    ("underline", &["u"]),
    ("strike", &["strike"]),
    ("color", &["color"]),
    ("highlight", &["highlight"]),
    ("size", &["sz", "szCs"]),
    ("font", &["rFonts"]),
    ("char_style", &["rStyle"]),
];

const PARAGRAPH_FIELDS: &[(&str, &[&str])] = &[
    ("align", &["jc"]),
    ("indent", &["ind"]),
    ("indent_left", &["ind"]),
    ("indent_right", &["ind"]),
    ("first_line", &["ind"]),
    ("hanging", &["ind"]),
    ("spacing", &["spacing"]),
    ("space_before", &["spacing"]),
    ("space_after", &["spacing"]),
    ("line_spacing", &["spacing"]),
    ("keep_next", &["keepNext"]),
    ("keep_lines", &["keepLines"]),
    ("page_break_before", &["pageBreakBefore"]),
];

/// The run properties of `format`.
pub(super) fn run_edit(tx: &Tx<'_>, ns: &str, op: &FormatOp) -> Result<PropEdit, StetError> {
    let mut set: Vec<PropElement> = Vec::new();
    if let Some(on) = op.bold {
        set.push(toggle(ns, "b", on));
    }
    if let Some(on) = op.italic {
        set.push(toggle(ns, "i", on));
    }
    if let Some(u) = &op.underline {
        set.push(element(ns, "u", &[("val", u.trim().to_string())]));
    }
    if let Some(on) = op.strike {
        set.push(toggle(ns, "strike", on));
    }
    if let Some(c) = &op.color {
        set.push(element(ns, "color", &[("val", units::color(c)?)]));
    }
    if let Some(h) = &op.highlight {
        set.push(element(ns, "highlight", &[("val", h.trim().to_string())]));
    }
    if let Some(size) = &op.size {
        let s = size.trim();
        let with_unit = if s.parse::<f64>().is_ok() {
            format!("{s}pt")
        } else {
            s.to_string()
        };
        let half = units::convert("sz", "val", &with_unit)?;
        if half.parse::<u32>().is_err() {
            return Err(invalid(format!(
                "`{size}` is not a font size. Write points, such as 11 or 12pt."
            )));
        }
        set.push(element(ns, "sz", &[("val", half.clone())]));
        set.push(element(ns, "szCs", &[("val", half)]));
    }
    if let Some(font) = &op.font {
        let f = font.trim().to_string();
        set.push(element(
            ns,
            "rFonts",
            &[("ascii", f.clone()), ("hAnsi", f.clone()), ("cs", f)],
        ));
    }
    if let Some(style) = &op.char_style {
        let id = style_id(tx, StyleKind::Character, style)?;
        set.push(element(ns, "rStyle", &[("val", id)]));
    }
    Ok(PropEdit {
        set,
        clear: cleared(ns, op.clear.as_deref().unwrap_or_default(), RUN_FIELDS),
    })
}

/// The attributes the paragraph's own `w:pPr/w:<local>` has, by local name.
fn own_attrs(tx: &Tx<'_>, key: &BlockKey, local: &str) -> Vec<(String, String)> {
    let view = tx.parts();
    let Some(tree) = view.id(key.part()).and_then(|id| view.tree(id).ok()) else {
        return Vec::new();
    };
    tree.node_ref(key.node())
        .child_elements()
        .find(|c| c.local_name() == "pPr")
        .and_then(|ppr| ppr.child_elements().find(|c| c.local_name() == local))
        .map(|el| {
            el.attrs()
                .filter(|a| a.namespace() == el.namespace())
                .map(|a| (a.local_name().to_string(), a.value().into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// Sets `name` among the attributes, or removes it with `None`.
fn put(attrs: &mut Vec<(String, String)>, name: &str, value: Option<String>) {
    attrs.retain(|(n, _)| n != name);
    if let Some(v) = value {
        attrs.push((name.to_string(), v));
    }
}

/// A length for an attribute, converted from human units.
fn length(element: &str, attribute: &str, value: &str) -> Result<String, StetError> {
    let v = units::convert(element, attribute, value)?;
    if v.parse::<i64>().is_err() {
        return Err(invalid(format!(
            "`{value}` is not a length. Write a number and a unit, such as 0.5in, 6pt or 1.5cm."
        )));
    }
    Ok(v)
}

/// The paragraph properties of `paragraph`.
pub(super) fn paragraph_edit(
    tx: &Tx<'_>,
    key: &BlockKey,
    ns: &str,
    op: &ParagraphOp,
) -> Result<PropEdit, StetError> {
    let mut set: Vec<PropElement> = Vec::new();
    if let Some(align) = &op.align {
        let val = match align.trim() {
            "left" | "start" => "left",
            "center" | "centre" => "center",
            "right" | "end" => "right",
            "justify" | "both" => "both",
            other => {
                return Err(invalid(format!(
                    "`align` is left, center, right or justify, not `{other}`."
                )));
            }
        };
        set.push(element(ns, "jc", &[("val", val.to_string())]));
    }
    let any_ind = op.indent_left.is_some()
        || op.indent_right.is_some()
        || op.first_line.is_some()
        || op.hanging.is_some();
    if any_ind {
        let mut attrs = own_attrs(tx, key, "ind");
        // Word 2010 and later may write `start` and `end` for `left` and `right`: keep the paragraph's own spelling.
        let left = if attrs.iter().any(|(n, _)| n == "start") {
            "start"
        } else {
            "left"
        };
        let right = if attrs.iter().any(|(n, _)| n == "end") {
            "end"
        } else {
            "right"
        };
        if let Some(v) = &op.indent_left {
            put(&mut attrs, left, Some(length("ind", "left", v)?));
        }
        if let Some(v) = &op.indent_right {
            put(&mut attrs, right, Some(length("ind", "right", v)?));
        }
        if let Some(v) = &op.first_line {
            put(
                &mut attrs,
                "firstLine",
                Some(length("ind", "firstLine", v)?),
            );
            put(&mut attrs, "hanging", None);
        }
        if let Some(v) = &op.hanging {
            put(&mut attrs, "hanging", Some(length("ind", "hanging", v)?));
            put(&mut attrs, "firstLine", None);
        }
        let pairs: Vec<(&str, String)> =
            attrs.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
        set.push(element(ns, "ind", &pairs));
    }
    if op.space_before.is_some() || op.space_after.is_some() || op.line_spacing.is_some() {
        let mut attrs = own_attrs(tx, key, "spacing");
        if let Some(v) = &op.space_before {
            put(&mut attrs, "before", Some(length("spacing", "before", v)?));
            put(&mut attrs, "beforeAutospacing", None);
        }
        if let Some(v) = &op.space_after {
            put(&mut attrs, "after", Some(length("spacing", "after", v)?));
            put(&mut attrs, "afterAutospacing", None);
        }
        if let Some(v) = &op.line_spacing {
            let t = v.trim();
            match t.parse::<f64>() {
                Ok(multiple) if multiple.is_finite() && multiple > 0.0 => {
                    put(
                        &mut attrs,
                        "line",
                        Some(format!("{}", (multiple * 240.0).round() as i64)),
                    );
                    put(&mut attrs, "lineRule", Some("auto".to_string()));
                }
                Ok(_) => {
                    return Err(invalid(format!(
                        "`line_spacing` is a multiple of single above 0, such as 1.5, or a length such as 14pt, not `{v}`."
                    )));
                }
                Err(_) => {
                    // A length is exact line spacing, in twips; a bare number is a multiple of single, above.
                    let line = units::twips(t)?.ok_or_else(|| {
                        invalid(format!(
                            "`line_spacing` is a multiple of single, such as 1.5, or a length such as 14pt, not `{v}`."
                        ))
                    })?;
                    put(&mut attrs, "line", Some(line.to_string()));
                    put(&mut attrs, "lineRule", Some("exact".to_string()));
                }
            }
        }
        let pairs: Vec<(&str, String)> =
            attrs.iter().map(|(n, v)| (n.as_str(), v.clone())).collect();
        set.push(element(ns, "spacing", &pairs));
    }
    if let Some(on) = op.keep_next {
        set.push(toggle(ns, "keepNext", on));
    }
    if let Some(on) = op.keep_lines {
        set.push(toggle(ns, "keepLines", on));
    }
    if let Some(on) = op.page_break_before {
        set.push(toggle(ns, "pageBreakBefore", on));
    }
    Ok(PropEdit {
        set,
        clear: cleared(
            ns,
            op.clear.as_deref().unwrap_or_default(),
            PARAGRAPH_FIELDS,
        ),
    })
}
