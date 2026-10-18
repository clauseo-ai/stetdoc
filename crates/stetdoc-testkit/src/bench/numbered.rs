//! The numbered body of the gates of `numbering` (`inputs::NUMBERED`): 20,000 paragraphs, two in five
//! of them numbered, with the styles and the numbering definitions that go with them. It is built
//! the same every time, in memory.

use super::spec::Input;
use crate::fixtures::parts::Doc;
use crate::fixtures::word::Package;

/// How many styles `numbered` defines: `Normal` and 56 chains of ten, each based on the one before.
const STYLE_CHAINS: usize = 56;
const STYLE_DEPTH: usize = 10;

/// The chains that carry a list in their head style, so that a paragraph in one is numbered by its
/// style and not by a `numPr` of its own.
const LISTED_CHAINS: usize = 10;

/// `styles.xml` of `numbered`: 561 styles in chains of at most ten.
fn numbered_styles() -> String {
    let w = crate::fixtures::parts::NS_W;
    let mut out = format!(
        "{}<w:styles xmlns:w=\"{w}\"><w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>",
        crate::fixtures::parts::DECL
    );
    for chain in 0..STYLE_CHAINS {
        for depth in 0..STYLE_DEPTH {
            let id = format!("S{chain}x{depth}");
            let based_on = if depth == 0 {
                "Normal".to_owned()
            } else {
                format!("S{chain}x{}", depth - 1)
            };
            let list = if chain < LISTED_CHAINS && depth == 0 {
                format!(
                    "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"1\"/></w:numPr>",
                    chain % 3
                )
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{id}\"/><w:basedOn w:val=\"{based_on}\"/><w:qFormat/><w:pPr>{list}<w:spacing w:before=\"{}\" w:after=\"120\"/><w:ind w:left=\"{}\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"{}\"/></w:rPr></w:style>",
                60 * depth,
                360 * (depth + 1),
                20 + depth
            ));
        }
    }
    out.push_str("</w:styles>");
    out
}

/// `numbering.xml` of `numbered`: a multilevel decimal list (`1.`, `1.1.`, ...) and a lettered one.
fn numbered_numbering() -> String {
    let w = crate::fixtures::parts::NS_W;
    let levels = |fmt: &[&str], text: &dyn Fn(usize) -> String| {
        (0..9)
            .map(|l| {
                format!(
                    "<w:lvl w:ilvl=\"{l}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{}\"/><w:lvlText w:val=\"{}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
                    fmt[l % fmt.len()],
                    text(l),
                    360 * (l + 1)
                )
            })
            .collect::<String>()
    };
    let dotted = |l: usize| (1..=l + 1).map(|k| format!("%{k}.")).collect::<String>();
    let lettered = |l: usize| format!("(%{})", l + 1);
    format!(
        "{}<w:numbering xmlns:w=\"{w}\"><w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"multilevel\"/>{}</w:abstractNum><w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"multilevel\"/>{}</w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>",
        crate::fixtures::parts::DECL,
        levels(&["decimal"], &dotted),
        levels(&["lowerLetter", "lowerRoman", "decimal"], &lettered)
    )
}

/// The body of `numbered`: `paragraphs` paragraphs, two in five numbered, half of those by a
/// `numPr` of their own (three levels, two lists) and half by their style, and the rest in styles
/// that carry no list. Every paragraph has one run.
fn numbered_body(paragraphs: usize) -> String {
    let mut body = String::with_capacity(paragraphs * 260);
    for i in 0..paragraphs {
        let (style, list) = match i % 5 {
            0 => (
                "Normal".to_owned(),
                format!(
                    "<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{}\"/></w:numPr>",
                    (i / 5) % 3,
                    1 + (i / 500) % 2
                ),
            ),
            1 => (
                format!("S{}x{}", (i / 5) % LISTED_CHAINS, (i / 50) % STYLE_DEPTH),
                String::new(),
            ),
            _ => (
                format!(
                    "S{}x{}",
                    LISTED_CHAINS + (i / 5) % (STYLE_CHAINS - LISTED_CHAINS),
                    (i / 230) % STYLE_DEPTH
                ),
                String::new(),
            ),
        };
        body.push_str(&format!(
            "<w:p><w:pPr><w:pStyle w:val=\"{style}\"/>{list}</w:pPr><w:r><w:t>Paragraph {i}: the party shall perform its obligations under clause {i} of this agreement.</w:t></w:r></w:p>"
        ));
    }
    body
}

/// The numbered body: 20,000 paragraphs and the styles and numbering that go with them, which is
/// what the gate of `numbering` is written for (design 8.5 of styles and numbering).
pub(super) fn numbered(name: &'static str) -> Input {
    let document = Doc::new().body(&numbered_body(20_000)).xml();
    let docx = Package::new(document.clone())
        .part(
            "word/styles.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
            "styles",
            numbered_styles(),
        )
        .part(
            "word/numbering.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
            "numbering",
            numbered_numbering(),
        )
        .build();
    let xml_bytes = super::inputs::xml_size(&docx);
    Input {
        name,
        docx,
        main_part: document.into_bytes(),
        xml_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::super::inputs::NUMBERED;
    use super::*;
    use crate::fixtures::inspect;

    fn count(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    #[test]
    fn the_numbered_input_has_the_paragraphs_styles_and_lists_the_gate_names() {
        let body = numbered_body(20_000);
        assert_eq!(count(&body, "<w:p>"), 20_000);
        // Two in five are numbered: by a `numPr` of their own, or by a style of a chain that
        // carries a list.
        assert_eq!(count(&body, "<w:numPr>"), 4_000);
        let listed: usize = (0..LISTED_CHAINS)
            .map(|c| count(&body, &format!("w:val=\"S{c}x")))
            .sum();
        assert_eq!(listed, 4_000);
        let styles = numbered_styles();
        assert_eq!(
            count(&styles, "<w:style "),
            561,
            "Normal and 56 chains of ten"
        );
        assert!(
            styles.contains(
                "w:styleId=\"S55x9\"><w:name w:val=\"S55x9\"/><w:basedOn w:val=\"S55x8\"/>"
            )
        );
        assert_eq!(count(&styles, "<w:numPr>"), LISTED_CHAINS);
        let numbering = numbered_numbering();
        assert_eq!(count(&numbering, "<w:abstractNum "), 2);
        assert_eq!(count(&numbering, "<w:num "), 2);
        assert_eq!(count(&numbering, "<w:lvl "), 18);
        assert!(numbering.contains("<w:lvlText w:val=\"%1.%2.%3.\"/>"));
    }

    #[test]
    fn the_numbered_input_is_a_package_that_relates_the_styles_and_the_numbering_to_the_document() {
        let input = numbered(NUMBERED);
        let archive = inspect::read(&input.docx).unwrap();
        let names: Vec<String> = archive
            .records
            .iter()
            .map(|r| String::from_utf8_lossy(&r.name).into_owned())
            .collect();
        for part in [
            "word/document.xml",
            "word/styles.xml",
            "word/numbering.xml",
            "word/_rels/document.xml.rels",
        ] {
            assert!(
                names.iter().any(|n| n == part),
                "{part} is not in {names:?}"
            );
        }
        assert!(
            input.main_part.len() > 3_000_000,
            "{}",
            input.main_part.len()
        );
        assert!(input.xml_bytes as usize > input.main_part.len());
    }
}
