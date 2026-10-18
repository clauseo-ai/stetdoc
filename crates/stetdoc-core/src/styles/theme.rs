//! The theme: fonts and colours that a style names instead of spelling out.
//!
//! `Theme::read` takes the theme part and the settings part (both optional). The colour scheme gives
//! the twelve colours, `w:clrSchemeMapping` in settings maps `background1`, `text1` and the like to
//! them, and `w:themeFontLang` says which per-script font stands in for an empty East Asian or complex
//! script typeface. A tint or shade scales the colour's luminance in HSL space. See the design, 3.5.

use crate::wordml::{Ns, SpanTree};
use std::collections::HashMap;

/// The lowest value of a channel of a colour that counts as white: `F8F8F8` is the first near-white.
const NEAR_WHITE: u8 = 0xF8;

/// A colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// Reads `RRGGBB`. `auto` and anything else is `None`.
    pub fn parse(hex: &str) -> Option<Rgb> {
        let h = hex.trim();
        if h.len() != 6 || !h.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        Some(Rgb {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
        })
    }

    /// `RRGGBB` in upper case.
    pub fn hex(self) -> String {
        format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// True for white, and for the near-whites that a reader cannot tell from it: every channel at or above
    /// `0xF8`. Text of such a colour hides as well as white text does, and a fill of such a colour is not a
    /// background that makes the text visible.
    pub fn is_near_white(self) -> bool {
        self.r >= NEAR_WHITE && self.g >= NEAR_WHITE && self.b >= NEAR_WHITE
    }

    fn to_hsl(self) -> (f64, f64, f64) {
        let (r, g, b) = (
            f64::from(self.r) / 255.0,
            f64::from(self.g) / 255.0,
            f64::from(self.b) / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        if (max - min).abs() < f64::EPSILON {
            return (0.0, 0.0, l);
        }
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if (max - r).abs() < f64::EPSILON {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if (max - g).abs() < f64::EPSILON {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h / 6.0, s, l)
    }

    fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
        let channel = |t: f64| {
            let t = t.rem_euclid(1.0);
            let q = if l < 0.5 {
                l * (1.0 + s)
            } else {
                l + s - l * s
            };
            let p = 2.0 * l - q;
            let v = if t < 1.0 / 6.0 {
                p + (q - p) * 6.0 * t
            } else if t < 0.5 {
                q
            } else if t < 2.0 / 3.0 {
                p + (q - p) * (2.0 / 3.0 - t) * 6.0
            } else {
                p
            };
            (v * 255.0 + 1e-9).floor().clamp(0.0, 255.0) as u8
        };
        if s == 0.0 {
            let v = (l * 255.0 + 1e-9).floor().clamp(0.0, 255.0) as u8;
            return Rgb { r: v, g: v, b: v };
        }
        Rgb {
            r: channel(h + 1.0 / 3.0),
            g: channel(h),
            b: channel(h - 1.0 / 3.0),
        }
    }

    /// The colour with its luminance scaled by `shade / 255` and moved towards white by `tint / 255`.
    ///
    /// Word does this in an integer HLS space, and its exact rounding is not published. This is the
    /// closest formula found against the values Word cached in the corpus: hue to 1/255 and saturation
    /// to 1/256, the arithmetic in floating point, and the result truncated. It matches 75 of 95 distinct
    /// tinted or shaded colours exactly, 94 within one in each channel and all within two.
    pub fn tinted(self, tint: Option<u8>, shade: Option<u8>) -> Rgb {
        if tint.is_none() && shade.is_none() {
            return self;
        }
        let (h, s, mut l) = self.to_hsl();
        let h = (h * 255.0).round() / 255.0;
        let s = (s * 256.0).round() / 256.0;
        if let Some(sh) = shade {
            l *= f64::from(sh) / 255.0;
        }
        if let Some(t) = tint {
            let t = f64::from(t) / 255.0;
            l = l * t + (1.0 - t);
        }
        Rgb::from_hsl(h, s, l)
    }
}

/// The typefaces of one font scheme (major or minor).
#[derive(Clone, Debug, Default)]
struct FontSet {
    latin: String,
    east_asian: String,
    complex: String,
    /// The per-script fonts: (script, typeface), such as (`Jpan`, `Yu Gothic`).
    scripts: Vec<(String, String)>,
}

/// The theme of a document. Empty (resolves nothing) when the document has no theme part.
#[derive(Clone, Debug, Default)]
pub struct Theme {
    colors: HashMap<String, Rgb>,
    major: FontSet,
    minor: FontSet,
    map: HashMap<String, String>,
    east_asian_lang: String,
    bidi_lang: String,
}

const DRAWINGML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// The script a language tag stands for in a font scheme's per-script list.
fn script_of(lang: &str) -> Option<&'static str> {
    let l = lang.to_ascii_lowercase();
    let p = l.split('-').next().unwrap_or("");
    Some(match (p, l.as_str()) {
        ("ja", _) => "Jpan",
        ("ko", _) => "Hang",
        (_, "zh-tw" | "zh-hk" | "zh-mo") => "Hant",
        ("zh", _) => "Hans",
        ("ar", _) => "Arab",
        ("he", _) => "Hebr",
        ("th", _) => "Thai",
        ("hi" | "mr" | "ne" | "sa", _) => "Deva",
        ("ru" | "uk" | "bg" | "sr", _) => "Cyrl",
        _ => return None,
    })
}

impl Theme {
    /// Reads the theme part and the settings part. Either may be missing.
    pub fn read(theme: Option<&SpanTree>, settings: Option<&SpanTree>) -> Theme {
        let mut t = Theme::default();
        if let Some(tree) = theme {
            t.read_theme(tree);
        }
        if let Some(tree) = settings {
            t.read_settings(tree);
        }
        t
    }

    fn read_theme(&mut self, tree: &SpanTree) {
        let Some(elements) = tree
            .root()
            .child_elements()
            .find(|c| c.local_name() == "themeElements")
        else {
            return;
        };
        for scheme in elements.child_elements() {
            match scheme.local_name() {
                "clrScheme" => {
                    for c in scheme.child_elements() {
                        let hex = c.child_elements().find_map(|v| match v.local_name() {
                            "srgbClr" => v.attr("", "val"),
                            "sysClr" => v.attr("", "lastClr"),
                            _ => None,
                        });
                        if let Some(rgb) = hex.and_then(|h| Rgb::parse(&h)) {
                            self.colors.insert(c.local_name().to_string(), rgb);
                        }
                    }
                }
                "fontScheme" => {
                    for set in scheme.child_elements() {
                        let target = match set.local_name() {
                            "majorFont" => &mut self.major,
                            "minorFont" => &mut self.minor,
                            _ => continue,
                        };
                        for f in set.child_elements() {
                            let face = f
                                .attr("", "typeface")
                                .map(|v| v.to_string())
                                .unwrap_or_default();
                            match f.local_name() {
                                "latin" => target.latin = face,
                                "ea" => target.east_asian = face,
                                "cs" => target.complex = face,
                                "font" => {
                                    if let Some(script) = f.attr("", "script") {
                                        target.scripts.push((script.to_string(), face));
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let _ = DRAWINGML;
    }

    fn read_settings(&mut self, tree: &SpanTree) {
        for c in tree.root().child_elements() {
            if !Ns::Word.matches_uri(c.namespace()) {
                continue;
            }
            let attrs = |c: &crate::xml::NodeRef<'_>| {
                c.attrs()
                    .filter(|a| Ns::Word.matches_uri(a.namespace()))
                    .map(|a| (a.local_name().to_string(), a.value().to_string()))
                    .collect::<Vec<_>>()
            };
            match c.local_name() {
                "clrSchemeMapping" => {
                    for (k, v) in attrs(&c) {
                        self.map.insert(k, v);
                    }
                }
                "themeFontLang" => {
                    for (k, v) in attrs(&c) {
                        match k.as_str() {
                            "eastAsia" => self.east_asian_lang = v,
                            "bidi" => self.bidi_lang = v,
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The scheme colour a `w:themeColor` value names, with a tint or shade applied.
    pub fn color(&self, theme_color: &str, tint: Option<u8>, shade: Option<u8>) -> Option<Rgb> {
        let name = match theme_color {
            "background1" | "bg1" => self.mapped("bg1", "light1"),
            "text1" | "tx1" | "t1" => self.mapped("t1", "dark1"),
            "background2" | "bg2" => self.mapped("bg2", "light2"),
            "text2" | "tx2" | "t2" => self.mapped("t2", "dark2"),
            "hyperlink" => "hlink".to_string(),
            "followedHyperlink" => "folHlink".to_string(),
            other => other.to_string(),
        };
        let key = match name.as_str() {
            "light1" => "lt1",
            "dark1" => "dk1",
            "light2" => "lt2",
            "dark2" => "dk2",
            other => other,
        };
        self.colors.get(key).map(|c| c.tinted(tint, shade))
    }

    fn mapped(&self, key: &str, default: &str) -> String {
        self.map
            .get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    /// The typeface a theme font name such as `minorHAnsi` or `majorEastAsia` stands for. An empty
    /// East Asian or complex script typeface takes the per-script font of the theme font language.
    pub fn font(&self, theme_name: &str) -> Option<String> {
        let (set, rest) = if let Some(r) = theme_name.strip_prefix("major") {
            (&self.major, r)
        } else if let Some(r) = theme_name.strip_prefix("minor") {
            (&self.minor, r)
        } else {
            return None;
        };
        let (face, lang) = match rest {
            "HAnsi" | "Ascii" => (&set.latin, ""),
            "EastAsia" => (&set.east_asian, self.east_asian_lang.as_str()),
            "Bidi" => (&set.complex, self.bidi_lang.as_str()),
            _ => return None,
        };
        if !face.is_empty() {
            return Some(face.clone());
        }
        let script = script_of(lang)?;
        set.scripts
            .iter()
            .find(|(s, _)| s == script)
            .map(|(_, f)| f.clone())
            .filter(|f| !f.is_empty())
    }
}
