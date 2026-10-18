//! The laws, each a function over `stetdoc_core`'s public interface.
//!
//! `ARCHITECTURE.md` rule 5 puts the laws at the interface, and `docs/design/document-and-ops.md` section 9.1 moves them here
//! from the core's own harnesses, which stay as the unit-level checks of `package`, `xml` and `state`. A law returns `Err`
//! with a sentence that names what broke, so that a test over the corpus can list every document that fails.
//!
//! **Built so far.** L1 and L4 over a document ([`identity`]), H7 over a working copy ([`complement`]), H12 over a write plan against
//! an in-memory directory ([`writes`], [`memdir`]), with and without hand edits now that `export` writes the readable files, the
//! operation conformance suite ([`conformance`]), the edit laws over any document ([`edits`]), H14 ([`receipts`]), the laws of an
//! edit of files, T1, T6 and T8 through `Document` ([`files`]), the totality of the read surface ([`surface`]), and
//! N5 and N7 of a comparison ([`compare`]). The random edit batches are [`generate`] (a seeded generator over the view) and
//! [`random`] (L2 to L5 on each batch, L2 by package entry in [`locality`]), with [`random_files`] for random edits of the readable files; the fixed batches of
//! [`edits`] stay as a smoke set. The sample of outputs for LibreOffice to read, and stetdoc's text of each, is [`libreoffice`]. The laws that
//! need `check` (L6, H13 and the refusals by operation index) arrive with step 4, and those of comments and properties with
//! steps 6 and 7.

pub mod blocks;
pub mod compare;
pub mod complement;
pub mod conformance;
pub mod edits;
pub mod facts;
pub mod files;
pub mod generate;
mod generate_ops;
mod hoist;
pub mod identity;
pub mod libreoffice;
pub mod locality;
pub mod memdir;
pub mod random;
pub mod random_files;
pub mod receipts;
pub mod surface;
pub mod sweep;
pub mod writes;
