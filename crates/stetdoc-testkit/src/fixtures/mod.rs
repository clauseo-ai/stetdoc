//! The synthetic edge, Word and hostile fixtures, built in memory.
//!
//! Real documents come from the corpus (`cargo xtask corpus fetch`). Details that no real
//! document exercises, and inputs that must be refused, are built here, byte by byte, so
//! tests generate them and the repository commits no binary.
//!
//! Each [`Fixture`] says what it exercises and what stetdoc must do with it: open it
//! (`Expect::Ok`, and then L1 holds) or refuse it with a stable error code at a stage. Edge
//! and Word fixtures are all valid and must open. A hostile fixture says why it is hostile,
//! in the vocabulary of `corpus/reasons.tsv`, and is either refused or must be survived.
//!
//! The expectations are judged at the level of the whole engine: a package with no
//! `[Content_Types].xml` opens as a ZIP archive but is not a `.docx`, so it is refused at the
//! document stage. Each layer checks only the stages it reaches ([`Expect::seen_by`]).
//!
//! A refusal at the read or XML stage also names the parts that carry it ([`Fixture::parts`]),
//! and every such fixture names them itself, with no default. A layer that never reads those
//! parts does not meet the refusal, and [`Fixture::seen_by_reading`] says what it must observe.

pub mod contract;
mod deflate;
mod edge_layout;
mod edge_markup;
mod edge_xml;
mod edge_zip;
mod hostile;
mod hostile_container;
mod hostile_syntax;
mod hostile_xml;
pub mod parts;
pub mod word;
pub mod zip;

pub use crate::expect::{Expect, Stage};

/// Which family a fixture belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Valid and unusual: stetdoc must handle it, and L1 holds.
    Edge,
    /// A minimal Word document for one feature, for the schema and text model tests. Valid.
    Word,
    /// Built to fail: stetdoc must refuse it cleanly, or survive it and never panic.
    Hostile,
}

/// One synthetic document.
#[derive(Clone, Copy, Debug)]
pub struct Fixture {
    /// A stable name, unique across every kind.
    pub name: &'static str,
    /// Edge, Word or hostile.
    pub kind: Kind,
    /// The detail it exercises, in a sentence.
    pub exercises: &'static str,
    /// What stetdoc must do with it.
    pub expect: Expect,
    /// Why a hostile fixture is hostile: entries of `corpus/reasons.tsv` separated by `;`.
    /// Empty for edge and Word fixtures.
    pub reason: &'static str,
    /// The parts that carry a refusal at the `Read` or `Xml` stage. Every such fixture names them
    /// itself, and a test enforces it. A layer that never reads them does not meet the refusal.
    /// Empty for every other fixture. The law that goes with it: **every part that is not named
    /// reads and parses cleanly**. [`Fixture::seen_by_reading`] relies on it, because it tells a
    /// layer that reads none of the named parts that the document opens, and the container laws
    /// of the core check it for every fixture (`unnamed_parts_are_clean`). A fixture that damages
    /// a second part names that part too.
    pub parts: &'static [&'static str],
    build: fn() -> Vec<u8>,
}

/// The main part, where most read and XML refusals sit.
pub(crate) const DOCUMENT_XML: &[&str] = &["word/document.xml"];

impl Fixture {
    /// Names the parts that carry a refusal at the `Read` or `Xml` stage.
    #[must_use]
    pub(crate) fn in_parts(mut self, parts: &'static [&'static str]) -> Fixture {
        self.parts = parts;
        self
    }

    /// What a layer must observe if it reads only the parts `reads` accepts and reaches the
    /// stages up to `last`: see [`Expect::seen_by_reading`].
    pub fn seen_by_reading(&self, last: Stage, reads: impl Fn(&str) -> bool) -> Expect {
        self.expect.seen_by_reading(last, self.parts, reads)
    }

    /// The bytes of the document. Building is deterministic and nothing is cached, so a
    /// large fixture costs time and memory only when asked for.
    pub fn bytes(&self) -> Vec<u8> {
        (self.build)()
    }
}

pub(crate) fn edge_fixture(
    name: &'static str,
    exercises: &'static str,
    expect: Expect,
    build: fn() -> Vec<u8>,
) -> Fixture {
    Fixture {
        name,
        kind: Kind::Edge,
        exercises,
        expect,
        reason: "",
        parts: &[],
        build,
    }
}

pub(crate) fn word_fixture(
    name: &'static str,
    exercises: &'static str,
    build: fn() -> Vec<u8>,
) -> Fixture {
    Fixture {
        name,
        kind: Kind::Word,
        exercises,
        expect: Expect::Ok,
        reason: "",
        parts: &[],
        build,
    }
}

pub(crate) fn hostile_fixture(
    name: &'static str,
    exercises: &'static str,
    expect: Expect,
    reason: &'static str,
    build: fn() -> Vec<u8>,
) -> Fixture {
    Fixture {
        name,
        kind: Kind::Hostile,
        exercises,
        expect,
        reason,
        parts: &[],
        build,
    }
}

/// The edge fixtures: valid packages with unusual ZIP, XML or package structure.
pub fn edge() -> Vec<Fixture> {
    let mut all = edge_zip::list();
    all.extend(edge_xml::list());
    all.extend(edge_layout::list());
    all.extend(edge_markup::list());
    all
}

/// The Word fixtures: minimal valid documents that each carry one feature, such as one of
/// every element, child elements out of order, extension attributes or a smart tag.
pub fn word() -> Vec<Fixture> {
    word::list()
}

/// The hostile fixtures: inputs that must be refused or survived.
pub fn hostile() -> Vec<Fixture> {
    let mut all = hostile::list();
    all.extend(hostile_xml::list());
    all.extend(hostile_container::list());
    all.extend(hostile_syntax::list());
    all
}

/// Every fixture: edge, then Word, then hostile.
pub fn all() -> Vec<Fixture> {
    let mut all = edge();
    all.extend(word());
    all.extend(hostile());
    all
}

#[cfg(test)]
mod expectations;
#[cfg(test)]
pub(crate) mod fuzz_seeds;
// Most of what the reader offers is for the tests of the builder; the benchmarks use `read`
// and `extract`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) mod inspect;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod markup_tests;
#[cfg(test)]
mod tests;
