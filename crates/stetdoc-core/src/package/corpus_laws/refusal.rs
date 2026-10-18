//! The law for a document that the index expects to be refused: it has to be refused with the code
//! and at the stage the index gives, by the parts it names, and not earlier, and the parts it does
//! not name have to be clean. See the header of `corpus_laws` for the stages.

use super::*;

pub(super) fn is_xml_part(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".xml") || lower.ends_with(".rels")
}

/// A document the index expects to be refused with `code`, at `stage` if the index says, by the
/// `parts` it names if it does. The refusal has to happen, and not earlier than the index says: a
/// document that opens, reads and parses as if it were fine fails here, and so does one that is
/// refused at `open` when the index puts the refusal at `read`.
pub(super) fn check_refusal(bytes: Vec<u8>, expect: &Expect) -> Result<(), String> {
    let (code, stage) = (expect.code.as_str(), expect.stage.as_deref());
    if let Some(other) = stage.filter(|s| !["open", "read", "xml", "document"].contains(s)) {
        return Err(format!("the index names an unknown stage {other:?}"));
    }
    let limits = Limits::default();
    let pkg = match Package::open(bytes.clone(), &limits) {
        Err(e) if matches!(stage, None | Some("open")) && e.code().as_str() == code => {
            return Ok(());
        }
        Err(e) if matches!(stage, None | Some("open")) => {
            return Err(format!("expected {code}, but open returned {e}"));
        }
        Err(e) => {
            let at = stage.unwrap_or("a later stage");
            return Err(format!(
                "expected {code} at {at}, but open refuses it already ({e}): the refusal comes earlier than the index says"
            ));
        }
        Ok(_) if stage == Some("open") => {
            return Err(format!("expected open to return {code}, but it opened"));
        }
        // Refused above this layer: the container laws hold as for any document.
        Ok(_) if stage == Some("document") => return check_identity(bytes),
        Ok(pkg) => pkg,
    };
    let (reads, parses) = (stage != Some("xml"), stage != Some("read"));
    let names: Vec<String> = if expect.parts.is_empty() {
        pkg.part_names().map(str::to_owned).collect()
    } else {
        expect.parts.clone()
    };
    // The refusals met with another code than the index gives, for the message if none is the
    // one it expects: a document whose part is refused for another reason is not one that
    // "opens, reads and parses as expected of a good one".
    let mut others: Vec<String> = Vec::new();
    for name in &names {
        match pkg.part(name) {
            Err(e) if e.code().as_str() == code && reads => {
                return unnamed_parts_are_clean(&pkg, expect, &limits);
            }
            Err(e) if e.code().as_str() == code => {
                return Err(format!(
                    "expected {code} at xml, but reading {name} is refused already ({e}): the refusal comes earlier than the index says"
                ));
            }
            Err(e) => others.push(format!(
                "reading {name} is refused with {} ({e})",
                e.code().as_str()
            )),
            Ok(Some(part)) if parses && is_xml_part(name) => {
                match SpanTree::parse(name, part, &limits) {
                    Err(e) if e.code().as_str() == code => {
                        return unnamed_parts_are_clean(&pkg, expect, &limits);
                    }
                    Err(e) => others.push(format!(
                        "parsing {name} is refused with {} ({e})",
                        e.code().as_str()
                    )),
                    Ok(_) => {}
                }
            }
            Ok(None) => {
                return Err(format!(
                    "the index names the part {name}, which is not in the package"
                ));
            }
            Ok(Some(_)) => {}
        }
    }
    let at = stage.map_or_else(|| "open, read or parse".to_owned(), str::to_owned);
    let of = if expect.parts.is_empty() {
        String::new()
    } else {
        format!(" of {}", expect.parts.join(", "))
    };
    if others.is_empty() {
        Err(format!(
            "expected {code} at {at}{of}, but the document opens, every part reads and every XML part parses as expected of a good one"
        ))
    } else {
        Err(format!(
            "expected {code} at {at}{of}, but it is not met: {}",
            others.join("; ")
        ))
    }
}

/// The law that `Expect::seen_by_reading` relies on: a refusal at `read` or `xml` is carried by
/// the parts the index names, and every other part reads and parses as the part of a good document
/// does. A layer that never reads a named part is told that the document must open; if another
/// part were damaged too, that layer would meet a refusal that the index does not mention. The law
/// holds nothing when the index names no parts, because then any part may carry the refusal.
fn unnamed_parts_are_clean(pkg: &Package, expect: &Expect, limits: &Limits) -> Result<(), String> {
    if expect.parts.is_empty() {
        return Ok(());
    }
    for name in pkg.part_names().map(str::to_owned).collect::<Vec<_>>() {
        if expect.parts.contains(&name) {
            continue;
        }
        let damaged = |what: &str, e: crate::api::error::StetError| {
            format!(
                "the index names {} as the parts that carry the refusal, but {name}, which it does not name, fails too: {what} is refused with {} ({e})",
                expect.parts.join(", "),
                e.code().as_str()
            )
        };
        match pkg.part(&name) {
            Err(e) => return Err(damaged("reading it", e)),
            Ok(Some(part)) if is_xml_part(&name) => {
                if let Err(e) = SpanTree::parse(&name, part, limits) {
                    return Err(damaged("parsing it", e));
                }
            }
            _ => {}
        }
    }
    Ok(())
}
