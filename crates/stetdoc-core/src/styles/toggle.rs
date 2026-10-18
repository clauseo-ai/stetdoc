//! Toggle properties, and the small flag set that answers "is this hidden" without merging anything.
//!
//! ISO 29500 calls twelve run properties toggle properties. Across the table style, the paragraph
//! style and the character style their values combine by exclusive-or, and direct formatting is
//! absolute. Inside one style's `basedOn` chain the nearest definition wins. See the design, 3.3.

use crate::wordml::{Ns, Prop, PropsRef};

/// The twelve toggle properties, in bit order.
pub(super) const TOGGLES: [&str; 12] = [
    "b",
    "bCs",
    "i",
    "iCs",
    "caps",
    "smallCaps",
    "strike",
    "emboss",
    "imprint",
    "outline",
    "shadow",
    "vanish",
];

/// The bit of `vanish`.
pub(super) const VANISH: u16 = 1 << 11;
/// The bit of `webHidden`, which is not a toggle: the last level that mentions it wins.
pub(super) const WEB_HIDDEN: u16 = 1 << 12;
/// The bit of `specVanish`, which is not a toggle either.
pub(super) const SPEC_VANISH: u16 = 1 << 13;
/// The bits that combine by exclusive-or.
pub(super) const TOGGLE_MASK: u16 = (1 << 12) - 1;

/// True if `name` is a toggle property.
pub(super) fn is_toggle(name: &str) -> bool {
    TOGGLES.contains(&name)
}

/// The flag bit of a Word-namespace property name, if it has one.
fn bit(name: &str) -> Option<u16> {
    match name {
        "webHidden" => Some(WEB_HIDDEN),
        "specVanish" => Some(SPEC_VANISH),
        _ => TOGGLES.iter().position(|t| *t == name).map(|i| 1u16 << i),
    }
}

/// Which of the toggle properties, `webHidden` and `specVanish` a level mentions, and their values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Flags {
    /// The properties the level mentions.
    pub set: u16,
    /// Their values (meaningful only where `set` is).
    pub on: u16,
}

impl Flags {
    /// The flags of a list of resolved properties.
    pub fn from_props<'a>(props: impl Iterator<Item = &'a Prop>) -> Flags {
        let mut f = Flags::default();
        for p in props {
            if p.ns != Ns::Word {
                continue;
            }
            if let Some(b) = bit(&p.name) {
                f.mention(b, p.on_off());
            }
        }
        f
    }

    /// The flags of a run's direct `w:rPr`, read in place with no allocation.
    pub fn from_ref(props: PropsRef<'_>) -> Flags {
        let mut f = Flags::default();
        for n in props.iter() {
            if !Ns::Word.matches_uri(n.namespace()) {
                continue;
            }
            if let Some(b) = bit(n.local_name()) {
                f.mention(b, props.on_off(n.local_name()).unwrap_or(true));
            }
        }
        f
    }

    /// Records that a level says `bit` is `on`.
    pub fn mention(&mut self, bit: u16, on: bool) {
        self.set |= bit;
        if on {
            self.on |= bit;
        } else {
            self.on &= !bit;
        }
    }

    /// Lays `higher` over `self` with plain replacement, as a `basedOn` chain does.
    pub fn overlay(&mut self, higher: Flags) {
        self.set |= higher.set;
        self.on = (self.on & !higher.set) | (higher.on & higher.set);
    }
}

/// The value of every flag after the document defaults, the three style kinds and direct formatting:
/// toggles are exclusive-ored from the defaults through the kinds, `webHidden` and `specVanish` are
/// replaced by the last level that mentions them, and direct formatting replaces all of them.
pub(super) fn combine(default: Flags, kinds: &[Flags], direct: Flags) -> u16 {
    let mut on = default.on & default.set;
    for k in kinds {
        on ^= k.on & k.set & TOGGLE_MASK;
        let plain = k.set & !TOGGLE_MASK;
        on = (on & !plain) | (k.on & plain);
    }
    (on & !direct.set) | (direct.on & direct.set)
}
