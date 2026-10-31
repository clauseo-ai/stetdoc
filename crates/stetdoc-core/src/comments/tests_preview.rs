//! A package that binds `w15` to the namespace of Word 2013's preview gets no `w15` rows: desktop Word
//! refuses it once a `commentsExtended` part with rows is beside it (Comment035, Word 16.114). Every
//! other package is written as before, byte for byte.

use super::add::{add, reply};
use super::resolve::set_resolved;
use super::tests_support::*;
use crate::api::error::ErrorCode;
use crate::revisions::compiler::Compiler;
use crate::state::State;
use crate::threads::Threads;
use crate::threads::tests_support::*;
use sha2::Digest;

const PREVIEW: &str = "http://schemas.microsoft.com/office/word/2010/11/wordml";
const RELEASED: &str = "http://schemas.microsoft.com/office/word/2012/wordml";

fn body() -> String {
    format!(
        "<w:p><w:r><w:t>The Purchase Price is payable.</w:t></w:r></w:p><w:p>{}</w:p>",
        ranged(0, "Second.")
    )
}

/// A document with one comment, by Ann, whose main part binds `w15` to `uri`.
fn binding(uri: &str) -> Pkg {
    Pkg::new(&body()).declare("w15", uri).comments(&[comment(
        0,
        "Ann",
        &[Some("0000000A")],
        "first",
    )])
}

/// Adds a comment on "Purchase Price" and returns the parts the effect names.
fn add_one(state: &State, tx: &mut crate::state::Tx<'_>) -> Vec<String> {
    let block = para(state, 0);
    let mut c = compiler(tx, &stamp());
    let fx = add(&mut c, &stamp(), &block, 4..18, "Check the price.", None).unwrap();
    c.finish().unwrap();
    fx.parts.iter().map(|p| p.to_string()).collect()
}

#[test]
fn a_comment_on_a_preview_package_is_written_without_comments_extended() {
    let state = binding(PREVIEW).people(&["Ann"]).open();
    let people = text(&state.begin(), "word/people.xml");
    let mut tx = state.begin();
    let parts = add_one(&state, &mut tx);
    assert_eq!(parts, ["word/document.xml", "word/comments.xml"]);
    assert!(!tx.parts().contains("word/commentsExtended.xml"));
    assert_eq!(text(&tx, "word/people.xml"), people, "no person is added");
    let view = tx.parts();
    let t = Threads::of(&view).unwrap();
    let c = t.comment(1).unwrap();
    assert_eq!(c.author.as_deref(), Some("Clauseo"));
    assert!(!c.resolved);
    assert!(text(&tx, "word/comments.xml").contains("<w:t>Check the price.</w:t>"));
}

#[test]
fn a_preview_comments_extended_part_that_exists_is_kept_up_to_date_in_its_namespace() {
    // Word 2013's preview wrote the part itself (Comment042): it is beside the preview parts already, and a
    // comment with no row in it would be a `row_missing` finding. So the row is added, in the part's namespace.
    let state = binding(PREVIEW)
        .extended_preview(&[ex("0000000A", None, false)])
        .open();
    let mut tx = state.begin();
    let parts = add_one(&state, &mut tx);
    assert_eq!(
        parts,
        [
            "word/document.xml",
            "word/comments.xml",
            "word/commentsExtended.xml"
        ]
    );
    let ex = text(&tx, "word/commentsExtended.xml");
    assert_eq!(ex.matches("<w15:commentEx ").count(), 2, "{ex}");
    assert!(!ex.contains(RELEASED), "{ex}");
    let mut c = compiler(&mut tx, &stamp());
    set_resolved(&mut c, 0, true).unwrap();
    reply(&mut c, &stamp(), 0, "Agreed.", None).unwrap();
}

#[test]
fn a_comment_on_a_released_package_still_writes_comments_extended() {
    let state = binding(RELEASED).open();
    let mut tx = state.begin();
    let parts = add_one(&state, &mut tx);
    assert_eq!(
        parts,
        [
            "word/document.xml",
            "word/comments.xml",
            "word/commentsExtended.xml"
        ]
    );
    let ex = text(&tx, "word/commentsExtended.xml");
    assert_eq!(ex.matches("<w15:commentEx ").count(), 2, "{ex}");
    assert!(ex.contains(&format!("xmlns:w15=\"{RELEASED}\"")));
}

#[test]
fn a_reply_and_a_resolve_on_a_preview_package_are_refused_and_change_nothing() {
    let state = binding(PREVIEW).open();
    let mut tx = state.begin();
    {
        let mut c = compiler(&mut tx, &stamp());
        let e = reply(&mut c, &stamp(), 0, "Agreed.", None).unwrap_err();
        assert_eq!(e.code(), ErrorCode::Unsupported);
        assert!(e.message().contains("2010/11/wordml"), "{}", e.message());
        let e = set_resolved(&mut c, 0, true).unwrap_err();
        assert_eq!(e.code(), ErrorCode::Unsupported);
        // Reopening an open thread writes nothing, so it is the usual no-op.
        let fx = set_resolved(&mut c, 0, false).unwrap();
        assert_eq!(fx.warnings[0].code, "already_open");
        assert!(fx.parts.is_empty());
        c.finish().unwrap();
    }
    let (next, _) = tx.commit().unwrap();
    let saved = next.to_package().unwrap().write().unwrap();
    assert!(
        saved == binding(PREVIEW).bytes(),
        "the package is the input, byte for byte"
    );
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The digest of the saved package after `f` runs over a compiler on `pkg`.
fn saved(pkg: &Pkg, f: impl FnOnce(&State, &mut Compiler<'_, '_>)) -> String {
    let state = pkg.open();
    let mut tx = state.begin();
    {
        let mut c = compiler(&mut tx, &stamp());
        f(&state, &mut c);
        c.finish().unwrap();
    }
    let (next, _) = tx.commit().unwrap();
    let bytes = next.to_package().unwrap().write().unwrap();
    hex(&sha2::Sha256::digest(&bytes))
}

/// The bytes that ordinary packages were written as before the preview rule: a document with no comment
/// parts, one that binds `w15` to the released namespace, and Word's threaded shape after a reply and a
/// resolve. A change here changes what every user's file looks like.
#[test]
fn ordinary_packages_are_written_byte_for_byte_as_before() {
    let add_on = |state: &State, c: &mut Compiler<'_, '_>| {
        let block = para(state, 0);
        add(c, &stamp(), &block, 4..18, "Check the price.", None).unwrap();
    };
    let plain = Pkg::new("<w:p><w:r><w:t>The Purchase Price is payable.</w:t></w:r></w:p>");
    let thread = |_: &State, c: &mut Compiler<'_, '_>| {
        reply(c, &stamp(), 2, "Agreed.", None).unwrap();
        set_resolved(c, 2, true).unwrap();
        set_resolved(c, 0, false).unwrap();
    };
    let digests = [
        saved(&plain, add_on),
        saved(&binding(RELEASED), add_on),
        saved(&threaded(), thread),
    ];
    assert_eq!(
        digests,
        [
            "a5e5806bc6cd351a5834f703e3d6a37f2e5156aedb2888a27bf9aa620f98ab49",
            "246996ee463a16834a34f8f74a1791b7bbd5bcd20619811640c2e47fc396ab39",
            "cc2d4e9482364e456e81d604edb5bf94b18b18a7ab6f8c14b34cc4fdac63f915"
        ]
    );
}
