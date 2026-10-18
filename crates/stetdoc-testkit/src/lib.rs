//! Shared test support for stetdoc.
//!
//! **Owns.** Corpus loaders, the fixture builder (how to make a test document without
//! Word), batch generators built only from public `view` output, normalization, the
//! conformance suites (operations and projection formats), the law checks (L1 to L6)
//! and the oracle runners (LibreOffice, Word).
//!
//! **Invariants.**
//! - It reaches the core only through the public interface.
//! - Tests that need an external tool (Word, LibreOffice) are skipped where the tool
//!   is absent.
//! - No document is committed. Real ones are listed in `corpus/manifest/` and fetched by
//!   pinned hash; synthetic ones come from [`fixtures`].
//! - **The API that the core's own tests use is plain data.** `corpus`, `expect` and `fixtures`
//!   expose paths, bytes, strings and enums defined here, never a core type. The core's tests
//!   may depend on this crate, which compiles the core a second time, so a core type crossing
//!   the API would be two different types. Only `codes` looks at the core (to validate error
//!   code names), and a test checks that no other file in these modules names it.
//! - **Panics are allowed here.** The testkit is test-support code, and a panic in it is a
//!   test failure, so it may use `unwrap`, `expect` and `panic!` (see the attribute
//!   below). Library crates may not.

#![deny(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod bench;
mod codes;
pub mod corpus;
pub mod expect;
pub mod fixtures;
pub mod laws;
pub mod oracle;
pub mod pairs;
#[cfg(test)]
mod seeds;
