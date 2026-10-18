//! The two readers of the toggle rule agree: `hidden` is a bit set folded by `toggle.rs`, and `effective` is a
//! list of entries folded by `effective.rs`, and they must never differ on `vanish`. A random test sets
//! every toggle property at every level the rule reads, in every combination of mention and value, and checks
//! both against a plain statement of the rule (design 3.3): the document defaults give the first value, each of
//! the table style, the paragraph style and the character style that mentions the property exclusive-ors its
//! value in (inside one style the nearest `basedOn` level that mentions it speaks, and in a table style the
//! conditional format over the whole-table format), and direct formatting replaces the result.

use super::hidden::HiddenReason;
use super::site::{RunSite, Site};
use super::tests_support::*;

/// The twelve toggle properties.
const TOGGLES: [&str; 12] = [
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

/// A tiny deterministic generator, so the test needs no dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound.max(1)
    }
}

/// What one level says about each toggle: nothing, on or off.
type Level = [Option<bool>; 12];

fn random_level(rng: &mut Lcg) -> Level {
    let mut level = [None; 12];
    for slot in &mut level {
        *slot = match rng.next(5) {
            0 | 1 => None,
            2 | 3 => Some(true),
            _ => Some(false),
        };
    }
    level
}

/// The children of a `w:rPr` that say what the level says.
fn rpr(level: &Level) -> String {
    TOGGLES
        .iter()
        .zip(level)
        .filter_map(|(name, v)| {
            v.map(|on| {
                if on {
                    format!("<w:{name}/>")
                } else {
                    format!("<w:{name} w:val=\"0\"/>")
                }
            })
        })
        .collect()
}

/// The rule, written plainly: the first of two levels that says anything speaks.
fn nearest(near: Option<bool>, far: Option<bool>) -> Option<bool> {
    near.or(far)
}

#[test]
fn hidden_and_effective_agree_on_the_toggle_rule_and_both_match_it() {
    let mut rng = Lcg(2024);
    for round in 0..400 {
        let [defaults_l, whole, region, p1, p2, c1, c2, direct] =
            std::array::from_fn(|_| random_level(&mut rng));
        let table = format!(
            "<w:style w:type=\"table\" w:styleId=\"T\"><w:rPr>{}</w:rPr>\
             <w:tblStylePr w:type=\"firstRow\"><w:rPr>{}</w:rPr></w:tblStylePr></w:style>",
            rpr(&whole),
            rpr(&region)
        );
        let styles = [
            format!(
                "<w:docDefaults><w:rPrDefault><w:rPr>{}</w:rPr></w:rPrDefault></w:docDefaults>",
                rpr(&defaults_l)
            ),
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"/>".to_owned(),
            "<w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"/>"
                .to_owned(),
            table,
            style(
                "paragraph",
                "P1",
                None,
                &format!("<w:rPr>{}</w:rPr>", rpr(&p1)),
            ),
            style(
                "paragraph",
                "P2",
                Some("P1"),
                &format!("<w:rPr>{}</w:rPr>", rpr(&p2)),
            ),
            style(
                "character",
                "C1",
                None,
                &format!("<w:rPr>{}</w:rPr>", rpr(&c1)),
            ),
            style(
                "character",
                "C2",
                Some("C1"),
                &format!("<w:rPr>{}</w:rPr>", rpr(&c2)),
            ),
        ]
        .concat();
        let cascade = cascade(&styles);
        let direct_tree = tree(&format!("<w:r><w:rPr>{}</w:rPr></w:r>", rpr(&direct)));
        let cell = site(Some("T"), look(true, false, false, false), 0, 2, 0, 2);
        let at = Site {
            style: Some("P2"),
            table: Some(&cell),
            run: Some(RunSite {
                style: Some("C2"),
                direct: props(&direct_tree, "rPr"),
                complex_script: false,
            }),
            ..Site::default()
        };
        let effective = cascade.effective(&at);
        for (i, name) in TOGGLES.iter().enumerate() {
            let at_level = |l: &Level| l[i];
            let mut want = at_level(&defaults_l).unwrap_or(false);
            for kind in [
                nearest(at_level(&region), at_level(&whole)),
                nearest(at_level(&p2), at_level(&p1)),
                nearest(at_level(&c2), at_level(&c1)),
            ]
            .into_iter()
            .flatten()
            {
                want ^= kind;
            }
            if let Some(d) = at_level(&direct) {
                want = d;
            }
            assert_eq!(effective.on(name), want, "round {round}: effective {name}");
        }
        let vanish = TOGGLES.len() - 1;
        assert_eq!(TOGGLES[vanish], "vanish");
        assert_eq!(
            cascade.hidden(&at) == Some(HiddenReason::Vanish),
            effective.on("vanish"),
            "round {round}: hidden and effective disagree on vanish"
        );
    }
}
