//! One style, resolved: its `basedOn` chain laid over itself once, so a run costs a lookup.

use super::bag::{Entry, overlay};
use super::source::Source;
use super::theme::Theme;
use super::toggle::Flags;
use crate::wordml::{Props, StyleDef, StyleDefs, StyleKind, TableRegion};

/// What a style gives, with its `basedOn` chain applied (the nearest style wins).
#[derive(Clone, Debug, Default)]
pub(super) struct Resolved {
    /// The paragraph properties.
    pub para: Vec<Entry>,
    /// The run properties.
    pub run: Vec<Entry>,
    /// Which toggle properties, `webHidden` and `specVanish` the run properties mention.
    pub flags: Flags,
    /// The outline level (`outlineLvl`), 0 to 9.
    pub outline: Option<u8>,
    /// The font size in half-points.
    pub size: Option<i64>,
    /// The complex script font size in half-points (`szCs`).
    pub size_cs: Option<i64>,
    /// The run shading fill (`rPr/shd`).
    pub run_shd: Option<Box<str>>,
    /// The run highlight (`rPr/highlight`).
    pub highlight: Option<Box<str>>,
    /// The paragraph shading fill (`pPr/shd`).
    pub para_shd: Option<Box<str>>,
    /// Whether the text colour is white, if the run properties set a colour.
    pub white: Option<bool>,
    /// The cell shading fill, for a table style.
    pub fill: Option<Box<str>>,
}

/// A table style: its own properties and each conditional format, chain applied.
#[derive(Clone, Debug, Default)]
pub(super) struct TableResolved {
    /// The properties that apply to the whole table.
    pub whole: Resolved,
    /// The conditional formats that the style or a parent defines.
    pub regions: Vec<(TableRegion, Resolved)>,
    /// `w:tblStyleRowBandSize`.
    pub row_band: usize,
    /// `w:tblStyleColBandSize`.
    pub col_band: usize,
}

/// The properties of one level: paragraph, run and cell.
struct Part<'a> {
    para: &'a Props,
    run: &'a Props,
    cell: &'a Props,
    source: Source,
}

fn lay(part: &Part<'_>, out: &mut Resolved) {
    for p in part.para.iter() {
        overlay(&mut out.para, p, &part.source);
    }
    for p in part.run.iter() {
        overlay(&mut out.run, p, &part.source);
    }
    if let Some(fill) = part.cell.attr("shd", "fill") {
        out.fill = Some(fill.into());
    }
}

fn finish(mut r: Resolved, theme: &Theme) -> Resolved {
    r.flags = Flags::from_props(r.run.iter().map(|e| &e.prop));
    r.outline = r
        .para
        .iter()
        .find(|e| &*e.prop.name == "outlineLvl")
        .and_then(|e| e.prop.val())
        .and_then(|v| v.trim().parse::<u8>().ok());
    r.size = r
        .run
        .iter()
        .find(|e| &*e.prop.name == "sz")
        .and_then(|e| e.prop.val())
        .and_then(|v| v.trim().parse::<i64>().ok());
    let value = |bag: &[Entry], name: &str, attr: Option<&str>| -> Option<Box<str>> {
        let e = bag.iter().find(|e| &*e.prop.name == name)?;
        match attr {
            Some(a) => e.prop.attr(a).map(Into::into),
            None => e.prop.val().map(Into::into),
        }
    };
    r.size_cs = value(&r.run, "szCs", None).and_then(|v| v.trim().parse::<i64>().ok());
    r.run_shd = value(&r.run, "shd", Some("fill"));
    r.highlight = value(&r.run, "highlight", None);
    r.para_shd = value(&r.para, "shd", Some("fill"));
    r.white = r.run.iter().find(|e| &*e.prop.name == "color").map(|e| {
        is_white(
            theme,
            e.prop.attr("val"),
            e.prop.attr("themeColor"),
            e.prop.attr("themeTint"),
            e.prop.attr("themeShade"),
        )
    });
    r
}

/// True if a colour written as `val` and the theme attributes is white, or so near white that a reader
/// cannot tell (every channel at or above `0xF8`, see `Rgb::is_near_white`).
pub(super) fn is_white(
    theme: &Theme,
    val: Option<&str>,
    theme_color: Option<&str>,
    tint: Option<&str>,
    shade: Option<&str>,
) -> bool {
    let hex = |s: Option<&str>| s.and_then(|v| u8::from_str_radix(v.trim(), 16).ok());
    if let Some(tc) = theme_color
        && let Some(rgb) = theme.color(tc, hex(tint), hex(shade))
    {
        return rgb.is_near_white();
    }
    val.and_then(super::theme::Rgb::parse)
        .is_some_and(|c| c.is_near_white())
}

/// The chain of `id`, base style first.
fn chain<'a>(defs: &'a StyleDefs, id: &str) -> Vec<&'a StyleDef> {
    let mut v: Vec<&StyleDef> = defs.chain(id).collect();
    v.reverse();
    v
}

/// Resolves a paragraph or character style.
pub(super) fn resolve_style(defs: &StyleDefs, theme: &Theme, id: &str) -> Resolved {
    let mut out = Resolved::default();
    for s in chain(defs, id) {
        let source = match s.kind {
            StyleKind::Character => Source::CharacterStyle { id: s.id.clone() },
            _ => Source::ParagraphStyle { id: s.id.clone() },
        };
        lay(
            &Part {
                para: &s.paragraph,
                run: &s.run,
                cell: &s.cell,
                source,
            },
            &mut out,
        );
    }
    finish(out, theme)
}

/// Resolves a table style and the conditional formats along its chain.
pub(super) fn resolve_table(defs: &StyleDefs, theme: &Theme, id: &str) -> TableResolved {
    let styles = chain(defs, id);
    let mut whole = Resolved::default();
    let mut regions: Vec<(TableRegion, Resolved)> = Vec::new();
    let (mut row_band, mut col_band) = (1usize, 1usize);
    for s in &styles {
        lay(
            &Part {
                para: &s.paragraph,
                run: &s.run,
                cell: &s.cell,
                source: Source::TableStyle {
                    id: s.id.clone(),
                    region: None,
                },
            },
            &mut whole,
        );
        let band = |name: &str| {
            s.table
                .val(name)
                .and_then(|v| v.trim().parse::<usize>().ok())
                .filter(|v| *v > 0)
        };
        row_band = band("tblStyleRowBandSize").unwrap_or(row_band);
        col_band = band("tblStyleColBandSize").unwrap_or(col_band);
        for (region, props) in &s.conditional {
            let at = match regions.iter().position(|(r, _)| r == region) {
                Some(i) => i,
                None => {
                    regions.push((*region, Resolved::default()));
                    regions.len() - 1
                }
            };
            if let Some((_, r)) = regions.get_mut(at) {
                lay(
                    &Part {
                        para: &props.paragraph,
                        run: &props.run,
                        cell: &props.cell,
                        source: Source::TableStyle {
                            id: s.id.clone(),
                            region: Some(*region),
                        },
                    },
                    r,
                );
            }
        }
    }
    TableResolved {
        whole: finish(whole, theme),
        regions: regions
            .into_iter()
            .map(|(r, v)| (r, finish(v, theme)))
            .collect(),
        row_band,
        col_band,
    }
}

/// Resolves the document defaults (`w:docDefaults`).
pub(super) fn resolve_defaults(defs: &StyleDefs, theme: &Theme) -> Resolved {
    let empty = Props::new();
    let mut out = Resolved::default();
    lay(
        &Part {
            para: &defs.doc_defaults.paragraph,
            run: &defs.doc_defaults.run,
            cell: &empty,
            source: Source::Default,
        },
        &mut out,
    );
    finish(out, theme)
}
