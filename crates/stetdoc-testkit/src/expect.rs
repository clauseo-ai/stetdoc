//! What stetdoc must do with a document, as plain data shared by the corpus and the fixtures.
//!
//! A document either opens and round-trips byte for byte ([`Expect::Ok`]), or is refused with
//! a stable error code at a [`Stage`] of the pipeline. The refusals are the exemption list of
//! law L1, so a refusal always has a stage. Each layer of the engine reaches only some stages:
//! a harness for the package layer cannot see a refusal that the XML layer makes, so it asks
//! [`Expect::seen_by`] what it should observe.
//!
//! A refusal at the `Read` or `Xml` stage is carried by particular parts: one part whose CRC is
//! wrong, one part that does not parse. A layer that never reads those parts does not meet the
//! refusal. For it the document must open, and law L1 must then hold. A lazy `Document::open`
//! reads only the parts it needs, so a document whose only damage is in `docProps/app.xml` or
//! in a theme opens there, and is refused only by a layer that reads every part the
//! relationships reach, such as the package layer's own laws. [`Expect::seen_by`] is for a
//! layer that reads every part the relationships reach, and [`Expect::seen_by_reading`] for one
//! that reads some.
//!
//! The refusals at `Open` and `Document` belong to no part, and every layer that reaches the
//! stage meets them. A `Document` refusal can still sit on a document whose parts are damaged
//! too, as `testascii.docx` and `zip.docx` show: neither has root relationships, and the XML of
//! both is also at fault. The row records the refusal that comes first, so its `part` stays
//! empty, and a layer below `Document` that reads one of those parts may meet the damage there.
//!
//! This module names no core type. The codes are checked against the core's `ErrorCode` wire
//! names inside this crate and leave as `&'static str`.

use crate::codes;
use std::fmt;

/// Where in the pipeline a refusal happens, in pipeline order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    /// The package cannot be opened: the ZIP structure is unusable.
    Open,
    /// The package opens and reading one part fails: bad CRC, broken deflate, encryption.
    Read,
    /// A part reads and parsing it as XML fails or hits a limit.
    Xml,
    /// The package is not a usable `.docx`: no content types, no root relationships or no main
    /// document part. This is found from the content types and the root relationships before
    /// any other part is read, so a package that lacks them and is also broken elsewhere is
    /// refused here, and the stages before it do not apply.
    Document,
}

impl Stage {
    /// Every stage, in pipeline order.
    pub const ALL: [Stage; 4] = [Stage::Open, Stage::Read, Stage::Xml, Stage::Document];

    /// The name used in `corpus/manifest` and in `INDEX.tsv`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Stage::Open => "open",
            Stage::Read => "read",
            Stage::Xml => "xml",
            Stage::Document => "document",
        }
    }

    /// The stage called `name`.
    pub fn parse(name: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|s| s.as_str() == name)
    }
}

/// What stetdoc must do with a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Expect {
    /// Open, read and round-trip byte for byte.
    Ok,
    /// Refuse with this stable error code at this stage.
    Refused {
        /// An `ErrorCode` wire name: `malformed_input`, `limit_exceeded` or `unsupported`.
        code: &'static str,
        /// Where the refusal happens.
        stage: Stage,
    },
}

impl Expect {
    /// Refused when the package is opened.
    pub const fn open(code: &'static str) -> Expect {
        Expect::Refused {
            code,
            stage: Stage::Open,
        }
    }

    /// Refused when a part is read.
    pub const fn read(code: &'static str) -> Expect {
        Expect::Refused {
            code,
            stage: Stage::Read,
        }
    }

    /// Refused when a part is parsed.
    pub const fn xml(code: &'static str) -> Expect {
        Expect::Refused {
            code,
            stage: Stage::Xml,
        }
    }

    /// Refused by the document layer.
    pub const fn document(code: &'static str) -> Expect {
        Expect::Refused {
            code,
            stage: Stage::Document,
        }
    }

    /// Reads the `expect` and `stage` columns of the manifest or of `INDEX.tsv`. The code must
    /// be `ok` (with stage `-`) or a refusal code that is an `ErrorCode` wire name (with a stage).
    pub fn from_columns(expect: &str, stage: &str) -> Result<Expect, String> {
        if expect == "ok" {
            return match stage {
                "-" => Ok(Expect::Ok),
                other => Err(format!("an ok document has stage `{other}`, not `-`")),
            };
        }
        let code = codes::refusal(expect).ok_or_else(|| {
            format!(
                "`{expect}` is not ok and not a refusal code ({})",
                codes::REFUSALS.join(", ")
            )
        })?;
        let stage = Stage::parse(stage).ok_or_else(|| format!("`{stage}` is not a stage"))?;
        Ok(Expect::Refused { code, stage })
    }

    /// The `expect` column: `ok` or the error code.
    pub const fn code(self) -> &'static str {
        match self {
            Expect::Ok => "ok",
            Expect::Refused { code, .. } => code,
        }
    }

    /// The `stage` column: `-` or the stage name.
    pub const fn stage_name(self) -> &'static str {
        match self {
            Expect::Ok => "-",
            Expect::Refused { stage, .. } => stage.as_str(),
        }
    }

    /// Whether the document must open.
    pub const fn is_ok(self) -> bool {
        matches!(self, Expect::Ok)
    }

    /// What a harness whose layer reaches stages up to and including `last` must observe: the
    /// refusal if it happens at or before `last`, and success otherwise, because the layer
    /// never gets as far as the refusal.
    pub fn seen_by(self, last: Stage) -> Expect {
        match self {
            Expect::Refused { stage, .. } if stage > last => Expect::Ok,
            other => other,
        }
    }
}

impl Expect {
    /// Like [`Expect::seen_by`], for a layer that reads only the parts `reads` accepts.
    ///
    /// `parts` are the parts that carry the refusal (`Entry::parts`, `Fixture::parts`). A refusal
    /// at the `Read` or `Xml` stage is met only by a layer that reads at least one of them. A
    /// layer that reads none of them sees `Ok`, and law L1 must hold for the document. That is
    /// true only if every part that is not named reads and parses cleanly, a law that the
    /// container laws of the core check.
    pub fn seen_by_reading(
        self,
        last: Stage,
        parts: &[&str],
        reads: impl Fn(&str) -> bool,
    ) -> Expect {
        match self {
            Expect::Refused {
                stage: Stage::Read | Stage::Xml,
                ..
            } if !parts.iter().any(|p| reads(p)) => Expect::Ok,
            other => other.seen_by(last),
        }
    }
}

impl fmt::Display for Expect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expect::Ok => f.write_str("ok"),
            Expect::Refused { code, stage } => write!(f, "{code} at {}", stage.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_in_pipeline_order_and_round_trip_through_their_names() {
        assert!(
            Stage::Open < Stage::Read && Stage::Read < Stage::Xml && Stage::Xml < Stage::Document
        );
        for s in Stage::ALL {
            assert_eq!(Stage::parse(s.as_str()), Some(s));
        }
        assert_eq!(Stage::parse("package"), None);
    }

    #[test]
    fn columns_parse_to_an_expectation_and_back() {
        assert_eq!(Expect::from_columns("ok", "-"), Ok(Expect::Ok));
        let e = Expect::from_columns("limit_exceeded", "read").unwrap();
        assert_eq!(e, Expect::read("limit_exceeded"));
        assert_eq!((e.code(), e.stage_name()), ("limit_exceeded", "read"));
        assert_eq!(e.to_string(), "limit_exceeded at read");
        assert_eq!((Expect::Ok.code(), Expect::Ok.stage_name()), ("ok", "-"));
    }

    #[test]
    fn bad_columns_are_rejected_with_a_reason() {
        assert!(
            Expect::from_columns("ok", "open")
                .unwrap_err()
                .contains("stage `open`")
        );
        assert!(
            Expect::from_columns("malformed_input", "-")
                .unwrap_err()
                .contains("not a stage")
        );
        assert!(
            Expect::from_columns("stale_ref", "open")
                .unwrap_err()
                .contains("not ok and not a refusal")
        );
        assert!(Expect::from_columns("nonsense", "open").is_err());
    }

    #[test]
    fn a_layer_that_never_reads_the_part_sees_ok() {
        let read = Expect::read("limit_exceeded");
        let reads_core = |p: &str| p == "word/document.xml";
        let reads_all = |_: &str| true;
        let app = ["docProps/app.xml"];
        assert_eq!(
            read.seen_by_reading(Stage::Xml, &app, reads_core),
            Expect::Ok
        );
        assert_eq!(read.seen_by_reading(Stage::Xml, &app, reads_all), read);
        let both = ["docProps/app.xml", "word/document.xml"];
        assert_eq!(read.seen_by_reading(Stage::Xml, &both, reads_core), read);
        let xml = Expect::xml("malformed_input");
        assert_eq!(
            xml.seen_by_reading(Stage::Read, &both, reads_all),
            Expect::Ok
        );
        assert_eq!(
            xml.seen_by_reading(Stage::Xml, &app, reads_core),
            Expect::Ok
        );
        // Open and document refusals belong to no part.
        let open = Expect::open("malformed_input");
        assert_eq!(open.seen_by_reading(Stage::Open, &[], reads_core), open);
        let doc = Expect::document("malformed_input");
        assert_eq!(doc.seen_by_reading(Stage::Document, &[], reads_core), doc);
        assert_eq!(
            Expect::Ok.seen_by_reading(Stage::Xml, &[], reads_core),
            Expect::Ok
        );
    }

    #[test]
    fn a_layer_sees_only_the_refusals_it_reaches() {
        let xml = Expect::xml("malformed_input");
        assert_eq!(xml.seen_by(Stage::Read), Expect::Ok);
        assert_eq!(xml.seen_by(Stage::Xml), xml);
        assert_eq!(xml.seen_by(Stage::Document), xml);
        let open = Expect::open("unsupported");
        assert_eq!(open.seen_by(Stage::Open), open);
        assert_eq!(Expect::Ok.seen_by(Stage::Open), Expect::Ok);
        assert_eq!(
            Expect::document("malformed_input").seen_by(Stage::Xml),
            Expect::Ok
        );
    }
}
