//! Hidden text, text that only looks hidden, the paragraph mark, and the outline level.
//!
//! `hidden` has Word's meaning only: `vanish` hides text, and `webHidden` hides it in a web view.
//! `looks_hidden` is a separate signal for text that Word shows and a reader cannot see (white text,
//! text of 2 pt or less). `specVanish` is not about text: on a paragraph mark it means the paragraph
//! never ends there. See the design, section 3.6. Nothing here allocates.

use super::cascade::Cascade;
use super::resolve::{Resolved, is_white};
use super::site::Site;
use super::theme::Rgb;
use super::toggle::{Flags, SPEC_VANISH, VANISH, WEB_HIDDEN, combine};

/// Why text is hidden, in Word's own sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HiddenReason {
    /// `vanish`: hidden from display.
    Vanish,
    /// `webHidden`: hidden in a web page view only, and shown in print.
    WebHidden,
}

/// Why text looks hidden to a reader though Word shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LooksHidden {
    /// The effective size is 2 pt or less.
    TinyText,
    /// The text is white and nothing behind it is a colour.
    WhiteText,
}

/// What a paragraph mark does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkEffect {
    /// An ordinary mark.
    Normal,
    /// The mark is hidden (`vanish`).
    Hidden,
    /// `specVanish`: the paragraph never ends here, so it joins the next one.
    JoinsNext,
}

/// The value of one property after the levels that can set it: the last level that mentions it wins.
fn last<'a, T: ?Sized>(levels: &[Option<&'a T>]) -> Option<&'a T> {
    levels.iter().rev().find_map(|l| *l)
}

impl Cascade {
    /// The flags after the default, the table, the paragraph style, the character style (for a run)
    /// and the direct formatting.
    fn flags(&self, site: &Site<'_>) -> u16 {
        let mut kinds = [Flags::default(); 3];
        let mut n = 0;
        let mut put = |f: Flags| {
            if let Some(slot) = kinds.get_mut(n) {
                *slot = f;
                n += 1;
            }
        };
        if let Some(ts) = site.table
            && let Some((table, regions)) = self.table_view(ts)
        {
            let mut f = Flags::default();
            for level in Cascade::table_levels(table, regions) {
                f.overlay(level.flags);
            }
            put(f);
        }
        if let Some(p) = self.paragraph_style(site.style) {
            put(p.flags);
        }
        let direct = match &site.run {
            Some(run) => {
                if let Some(c) = self.character_style(run) {
                    put(c.flags);
                }
                run.direct.map(Flags::from_ref).unwrap_or_default()
            }
            None => site.mark.map(Flags::from_ref).unwrap_or_default(),
        };
        combine(self.defaults.flags, kinds.get(..n).unwrap_or(&[]), direct)
    }

    /// Whether the run is hidden, in Word's sense. With no run in the site, whether the paragraph
    /// mark is hidden.
    pub fn hidden(&self, site: &Site<'_>) -> Option<HiddenReason> {
        let bits = self.flags(site);
        if bits & VANISH != 0 {
            Some(HiddenReason::Vanish)
        } else if bits & WEB_HIDDEN != 0 && site.run.is_some() {
            Some(HiddenReason::WebHidden)
        } else {
            None
        }
    }

    /// What the paragraph mark does: hidden, joined to the next paragraph, or nothing.
    pub fn mark_effect(&self, site: &Site<'_>) -> MarkEffect {
        let mark_site = Site {
            run: None,
            ..site.clone()
        };
        let bits = self.flags(&mark_site);
        if bits & SPEC_VANISH != 0 {
            MarkEffect::JoinsNext
        } else if bits & VANISH != 0 {
            MarkEffect::Hidden
        } else {
            MarkEffect::Normal
        }
    }

    /// Text that Word shows but a reader cannot see. Ask it for runs that have text.
    pub fn looks_hidden(&self, site: &Site<'_>) -> Option<LooksHidden> {
        let run = site.run.as_ref()?;
        let direct = run.direct;
        let defaults = Some(&self.defaults);
        let para = self.paragraph_style(site.style);
        let chr = self.character_style(run);
        // The table style sits between the defaults and the paragraph style. Its levels are folded
        // into the last values below.
        let mut size_t: Option<i64> = None;
        let mut white_t: Option<bool> = None;
        let mut run_shd_t: Option<&str> = None;
        let mut highlight_t: Option<&str> = None;
        let mut para_shd_t: Option<&str> = None;
        let mut cell_fill: Option<&str> = site.cell_fill;
        if let Some(ts) = site.table
            && let Some((table, regions)) = self.table_view(ts)
        {
            let mut fill = None;
            for level in Cascade::table_levels(table, regions) {
                size_t = if run.complex_script {
                    level.size_cs
                } else {
                    level.size
                }
                .or(size_t);
                white_t = level.white.or(white_t);
                run_shd_t = level.run_shd.as_deref().or(run_shd_t);
                highlight_t = level.highlight.as_deref().or(highlight_t);
                para_shd_t = level.para_shd.as_deref().or(para_shd_t);
                fill = level.fill.as_deref().or(fill);
            }
            // The cell's own shading is above the table style's.
            cell_fill = site.cell_fill.or(fill);
        }
        let level_size = |r: &Resolved| {
            if run.complex_script {
                r.size_cs
            } else {
                r.size
            }
        };
        let mut size = chr
            .and_then(level_size)
            .or_else(|| para.and_then(level_size))
            .or(size_t)
            .or_else(|| defaults.and_then(level_size));
        let mut white = chr
            .and_then(|r| r.white)
            .or_else(|| para.and_then(|r| r.white))
            .or(white_t)
            .or_else(|| defaults.and_then(|r| r.white));
        if let Some(d) = direct {
            let name = if run.complex_script { "szCs" } else { "sz" };
            if let Some(s) = d.val(name).and_then(|v| v.trim().parse::<i64>().ok()) {
                size = Some(s);
            }
            if d.get("color").is_some() {
                white = Some(is_white(
                    &self.theme,
                    d.attr("color", "val").as_deref(),
                    d.attr("color", "themeColor").as_deref(),
                    d.attr("color", "themeTint").as_deref(),
                    d.attr("color", "themeShade").as_deref(),
                ));
            }
        }
        if size.is_some_and(|s| s <= 4) {
            return Some(LooksHidden::TinyText);
        }
        if white != Some(true) {
            return None;
        }
        // What is behind the text: the last level that sets each kind of background wins.
        let direct_shd = direct.and_then(|r| r.attr("shd", "fill"));
        let direct_highlight = direct.and_then(|r| r.val("highlight"));
        let direct_para = site.ppr.and_then(|p| p.attr("shd", "fill"));
        let run_shd = last(&[
            defaults.and_then(|r| r.run_shd.as_deref()),
            run_shd_t,
            para.and_then(|r| r.run_shd.as_deref()),
            chr.and_then(|r| r.run_shd.as_deref()),
            direct_shd.as_deref(),
        ]);
        let highlight = last(&[
            defaults.and_then(|r| r.highlight.as_deref()),
            highlight_t,
            para.and_then(|r| r.highlight.as_deref()),
            chr.and_then(|r| r.highlight.as_deref()),
            direct_highlight.as_deref(),
        ]);
        let para_shd = last(&[
            defaults.and_then(|r| r.para_shd.as_deref()),
            para_shd_t,
            para.and_then(|r| r.para_shd.as_deref()),
            direct_para.as_deref(),
        ]);
        let behind = [run_shd, highlight, para_shd, cell_fill, site.background];
        if behind.iter().any(|c| coloured(*c)) {
            return None;
        }
        Some(LooksHidden::WhiteText)
    }

    /// The outline level of a paragraph, 0 to 8 (a body text paragraph is `None`): the direct
    /// `outlineLvl` over the paragraph style's. No merge and no allocation.
    pub fn outline_level(&self, site: &Site<'_>) -> Option<u8> {
        let direct = site
            .ppr
            .and_then(|p| p.val("outlineLvl"))
            .and_then(|v| v.trim().parse::<u8>().ok());
        let level = direct.or_else(|| {
            self.paragraph_style(site.style)
                .and_then(|p| p.outline)
                .or(self.defaults.outline)
        });
        level.filter(|l| *l <= 8)
    }
}

/// True if a fill is a colour other than white: not `auto`, `none`, white or a near-white.
fn coloured(c: Option<&str>) -> bool {
    c.is_some_and(|c| {
        let c = c.trim();
        !(c.eq_ignore_ascii_case("auto")
            || c.eq_ignore_ascii_case("none")
            || c.eq_ignore_ascii_case("white")
            || Rgb::parse(c).is_some_and(Rgb::is_near_white))
    })
}
