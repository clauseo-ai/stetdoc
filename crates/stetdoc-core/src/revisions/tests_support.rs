//! Builders and readers for the tests of the compiler: a state from a body, a transaction and a
//! compiler over it, and the text and XML of what it did.

use super::compiler::Compiler;
use super::outcome::Outcome;
use super::stamp::{RevStamp, Track};
use super::verify::key_through_renames;
use crate::api::limits::Limits;
use crate::schema::{PartKind, validate_part};
use crate::state::{State, Tx};
use crate::wordml::BlockKey;
use crate::wordml::{BlockKind, Mode};
use stetdoc_testkit::fixtures::{parts, zip::Zip};

pub(super) const MAIN: &str = "word/document.xml";

/// Extra namespace declarations for the root element: `m` (math) and `w14`.
pub(super) const EXTRA_NS: &str = " xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"w14\"";

/// A state whose body is `body` (a section break follows it).
pub(super) fn state(body: &str) -> State {
    let doc = parts::Doc::new().body(body).root_attrs(EXTRA_NS).bytes();
    State::open(
        Zip::new(parts::entries_doc(&doc)).build(),
        &Limits::default(),
    )
    .unwrap_or_else(|e| panic!("the test body does not open: {e}"))
}

/// The stamp of the author under test.
pub(super) fn stamp() -> RevStamp {
    RevStamp::new("Clauseo", "2026-10-05T14:03:27Z").unwrap()
}

/// A stamp of another author.
pub(super) fn stamp_of(author: &str) -> RevStamp {
    RevStamp::new(author, "2026-10-06T09:00:00Z").unwrap()
}

/// A tracked compiler over a transaction.
pub(super) fn tracked<'t, 's>(tx: &'t mut Tx<'s>) -> Compiler<'t, 's> {
    Compiler::new(tx, Track::Tracked(stamp())).unwrap()
}

/// A tracked compiler of another author.
pub(super) fn tracked_by<'t, 's>(tx: &'t mut Tx<'s>, author: &str) -> Compiler<'t, 's> {
    Compiler::new(tx, Track::Tracked(stamp_of(author))).unwrap()
}

/// An untracked compiler over a transaction.
pub(super) fn untracked<'t, 's>(tx: &'t mut Tx<'s>) -> Compiler<'t, 's> {
    Compiler::new(tx, Track::Untracked).unwrap()
}

/// `<w:p>` with one run of text.
pub(super) fn para(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

/// A revision's attributes as the tests' author writes them.
pub(super) const AT: &str = "w:author=\"Clauseo\" w:date=\"2026-10-05T14:03:27Z\"";
/// The attributes of another author's revision, id 900 and up.
pub(super) fn eric(id: u32) -> String {
    format!("w:id=\"{id}\" w:author=\"Eric\" w:date=\"2020-01-02T03:04:05Z\"")
}

/// The keys of the blocks of the body, in order.
pub(super) fn keys(tx: &Tx<'_>) -> Vec<BlockKey> {
    let view = tx.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    story.blocks().iter().map(|e| story.key(e)).collect()
}

/// The keys of the paragraphs of the body, in order.
pub(super) fn para_keys(tx: &Tx<'_>) -> Vec<BlockKey> {
    let view = tx.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    story
        .blocks()
        .iter()
        .filter(|e| e.kind == BlockKind::Paragraph)
        .map(|e| story.key(e))
        .collect()
}

/// The text of paragraph `i` of the body (counting paragraphs only) in a mode.
pub(super) fn text(tx: &Tx<'_>, i: usize, mode: Mode) -> String {
    texts(tx, mode).get(i).cloned().unwrap_or_default()
}

/// The text of every paragraph of the body, in a mode.
pub(super) fn texts(tx: &Tx<'_>, mode: Mode) -> Vec<String> {
    texts_of(&tx.parts(), mode)
}

/// The text of every paragraph of the body of a state, in a mode.
pub(super) fn state_texts(s: &State, mode: Mode) -> Vec<String> {
    texts_of(&s.parts(), mode)
}

/// The texts of the join runs of the body in a mode: paragraphs that read as one are one text. In
/// `Original`, a paragraph whose inserted mark has nothing to join into is left out, as law R1 does, unless it ends
/// its container after a paragraph, where rejecting keeps it (3.2.1).
pub(super) fn run_texts(view: &crate::state::PartsView<'_>, mode: Mode) -> Vec<String> {
    use crate::wordml::MARK_INSERTED;
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    let mut out = Vec::new();
    let mut i = 0;
    while i < story.blocks().len() {
        if story.blocks()[i].kind != BlockKind::Paragraph {
            i += 1;
            continue;
        }
        let run = story.join_run(tree, i, mode);
        let last = story.blocks()[run.end - 1];
        // Rejecting keeps it, emptied, when it ends its container after a paragraph (3.2.1).
        let kept = crate::wordml::ends_container(tree, last.node)
            && !super::resolve_mark::goes_whole(tree, last.node)
            && !super::resolve_mark::follows_a_block(tree, story.blocks()[run.start].node);
        let trailing = mode == Mode::Original && last.has(MARK_INSERTED) && !kept;
        if !trailing {
            out.push(
                run.clone()
                    .map(|k| story.paragraph(tree, k).unwrap().text(mode))
                    .collect::<String>(),
            );
        }
        i = run.end.max(i + 1);
    }
    out
}

fn texts_of(view: &crate::state::PartsView<'_>, mode: Mode) -> Vec<String> {
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    (0..story.blocks().len())
        .filter(|&i| story.blocks()[i].kind == BlockKind::Paragraph)
        .map(|i| story.paragraph(tree, i).unwrap().text(mode))
        .collect()
}

/// The body of the main part as XML text: from after `<w:body>` to before the section properties that
/// the builder adds.
pub(super) fn body_xml(tx: &Tx<'_>) -> String {
    let view = tx.parts();
    let id = view.id(MAIN).unwrap();
    slice_body(view.tree(id).unwrap().serialize())
}

/// The same, for a committed state.
pub(super) fn body_xml_of(state: &State) -> String {
    let id = state.parts().id(MAIN).unwrap();
    slice_body(state.parts().tree(id).unwrap().serialize())
}

fn slice_body(bytes: Vec<u8>) -> String {
    let s = String::from_utf8(bytes).unwrap();
    // A document of the builder has `<w:body>` and an empty `<w:sectPr/>` at the end. Others, from the
    // corpus, have their own: the whole body then.
    let from = s.find("<w:body>").map_or(0, |i| i + "<w:body>".len());
    let to = s
        .rfind("<w:sectPr/>")
        .or_else(|| s.rfind("</w:body>"))
        .unwrap_or(s.len());
    s.get(from..to.max(from)).unwrap_or_default().to_owned()
}

/// The schema violations that the transaction added to the main part, as readable keys.
pub(super) fn schema_added(s: &State, tx: &Tx<'_>) -> Vec<String> {
    let id = s.parts().id(MAIN).unwrap();
    let before = validate_part(PartKind::Document, s.parts().tree(id).unwrap()).set;
    let after = validate_part(PartKind::Document, tx.parts().tree(id).unwrap()).set;
    // The keys `check::edit` compares: a violation that an element carries through a rename keeps its key, and so does
    // one in a property container wherever the container stands (a run, a mark, the snapshot of a change).
    let key = |k: &crate::schema::ViolationKey| crate::check::Key::schema(&key_through_renames(k));
    let known: std::collections::BTreeSet<_> = before.iter().map(|(k, _)| key(k)).collect();
    after
        .iter()
        .map(|(k, _)| key(k))
        .filter(|k| !known.contains(k))
        .map(|k| format!("{k:?}"))
        .collect()
}

/// The ids of the revisions of the main part, in document order.
pub(super) fn revision_ids(tx: &Tx<'_>) -> Vec<i64> {
    let view = tx.parts();
    let id = view.id(MAIN).unwrap();
    crate::wordml::part_revisions(view.tree(id).unwrap())
        .iter()
        .filter_map(|r| r.id)
        .collect()
}

/// The ids that `outcome` says its transaction created, in order.
pub(super) fn created_ids(outcome: &Outcome) -> Vec<i64> {
    outcome.created.iter().map(|c| c.id).collect()
}

/// The main part of a view in the comparison form of `normalize`.
pub(super) fn normalized_part(view: &crate::state::PartsView<'_>) -> String {
    super::laws_norm::normalize(view.tree(view.id(MAIN).unwrap()).unwrap())
}

/// The comparison form of the part of a document with this body.
pub(super) fn normalized_body(body: &str) -> String {
    normalized_part(&state(body).parts())
}

/// What a run of compiler calls over a body left behind, all owned so a test can look at it after the
/// transaction is gone.
pub(super) struct Snap {
    /// The body XML after the calls.
    pub xml: String,
    /// The main part after the calls in the comparison form of `normalize`.
    pub norm: String,
    /// The text of each paragraph in `Final`, `Original` and `Redline`, after the calls.
    pub fin: Vec<String>,
    pub orig: Vec<String>,
    pub orig_runs: Vec<String>,
    pub red: Vec<String>,
    /// The same before the calls.
    pub before_fin: Vec<String>,
    pub before_orig: Vec<String>,
    pub before_orig_runs: Vec<String>,
    pub before_red: Vec<String>,
    /// The result of each call.
    pub effects: Vec<Result<super::outcome::Effect, crate::api::error::StetError>>,
    /// The result of `finish`.
    pub outcome: Result<Outcome, crate::api::error::StetError>,
    /// Schema violations that the calls added.
    pub schema: Vec<String>,
}

/// One text edit: the paragraph (counting paragraphs), the range in its `Final` text, the new text.
pub(super) type Edit<'a> = (usize, std::ops::Range<usize>, &'a str);

/// Runs `f` on a compiler of the given kind over a body: it gets the keys of all blocks and of the
/// paragraphs, and returns the result of each call it made.
pub(super) fn with_compiler(
    body: &str,
    track: Track,
    f: impl FnOnce(
        &mut Compiler<'_, '_>,
        &[BlockKey],
        &[BlockKey],
    ) -> Vec<Result<super::outcome::Effect, crate::api::error::StetError>>,
) -> Snap {
    let s = state(body);
    let mut tx = s.begin();
    let all = keys(&tx);
    let ks = para_keys(&tx);
    let (effects, outcome) = {
        let mut c = Compiler::new(&mut tx, track).unwrap();
        let effects = f(&mut c, &all, &ks);
        (effects, c.finish())
    };
    Snap {
        xml: body_xml(&tx),
        norm: normalized_part(&tx.parts()),
        fin: texts(&tx, Mode::Final),
        orig: texts(&tx, Mode::Original),
        orig_runs: run_texts(&tx.parts(), Mode::Original),
        red: texts(&tx, Mode::Redline),
        before_fin: state_texts(&s, Mode::Final),
        before_orig: state_texts(&s, Mode::Original),
        before_orig_runs: run_texts(&s.parts(), Mode::Original),
        before_red: state_texts(&s, Mode::Redline),
        effects,
        outcome,
        schema: schema_added(&s, &tx),
    }
}

/// Applies text edits to a body with a compiler of the given kind.
pub(super) fn apply(body: &str, track: Track, edits: &[Edit<'_>]) -> Snap {
    with_compiler(body, track, |c, _, ks| {
        edits
            .iter()
            .map(|(p, range, with)| c.replace_text(&ks[*p], range.clone(), with))
            .collect()
    })
}

/// Applies tracked edits by `Clauseo`.
pub(super) fn tracked_edits(body: &str, edits: &[Edit<'_>]) -> Snap {
    apply(body, Track::Tracked(stamp()), edits)
}

/// Applies untracked edits.
pub(super) fn untracked_edits(body: &str, edits: &[Edit<'_>]) -> Snap {
    apply(body, Track::Untracked, edits)
}

impl Snap {
    /// What every tracked text edit must leave: all calls and `finish` succeeded, `Original` is as it
    /// was, and the schema has nothing new to say.
    pub(super) fn assert_clean(&self) {
        for e in &self.effects {
            assert!(
                e.is_ok(),
                "a call failed: {:?}\n{}",
                e.as_ref().err(),
                self.xml
            );
        }
        assert!(
            self.outcome.is_ok(),
            "finish failed: {:?}\n{}",
            self.outcome.as_ref().err(),
            self.xml
        );
        assert_eq!(
            self.orig_runs, self.before_orig_runs,
            "Original changed\n{}",
            self.xml
        );
        assert!(
            self.schema.is_empty(),
            "schema violations {:?}\n{}",
            self.schema,
            self.xml
        );
    }

    /// Asserts that the result is, after `normalize`, the document with this body: what Word writes for
    /// the same edit, ids renamed, with the author and date of the stamp under test.
    pub(super) fn assert_word_shape(&self, expected_body: &str) {
        let want = normalized_body(expected_body);
        assert!(
            self.norm == want,
            "not the shape Word writes\n got : {}\n want: {}\n body: {}",
            self.norm,
            want,
            self.xml
        );
    }

    /// The error code of the call at `i`, if it failed.
    pub(super) fn code(&self, i: usize) -> Option<crate::api::error::ErrorCode> {
        self.effects
            .get(i)
            .and_then(|e| e.as_ref().err())
            .map(|e| e.code())
    }

    /// The effect of the call at `i`.
    pub(super) fn effect(&self, i: usize) -> &super::outcome::Effect {
        self.effects[i].as_ref().unwrap()
    }
}

// Fixtures shared by the text tests.

pub(super) fn p(inner: &str) -> String {
    format!("<w:p>{inner}</w:p>")
}

pub(super) fn r(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

pub(super) fn del(id: u32, text: &str) -> String {
    format!(
        "<w:del w:id=\"{id}\" {AT}><w:r><w:delText xml:space=\"preserve\">{text}</w:delText></w:r></w:del>"
    )
}

/// A run that the compiler wrote: `xml:space` only where the text needs it.
pub(super) fn new_run(text: &str) -> String {
    let lead = text.starts_with(' ') || text.ends_with(' ');
    let attr = if lead { " xml:space=\"preserve\"" } else { "" };
    format!("<w:r><w:t{attr}>{text}</w:t></w:r>")
}

pub(super) fn ins(id: u32, text: &str) -> String {
    format!("<w:ins w:id=\"{id}\" {AT}>{}</w:ins>", new_run(text))
}

// Property edits for the tests.

/// The `w` namespace URI.
pub(super) const WNS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// A property element in the `w` namespace with `w:` attributes and children.
pub(super) fn pel(
    local: &str,
    attrs: &[(&str, &str)],
    children: Vec<super::types::PropElement>,
) -> super::types::PropElement {
    super::types::PropElement {
        ns: WNS.to_owned(),
        local: local.to_owned(),
        attrs: attrs
            .iter()
            .map(|(k, v)| (WNS.to_owned(), (*k).to_owned(), (*v).to_owned()))
            .collect(),
        children,
    }
}

/// An edit that sets the given properties.
pub(super) fn set_edit(set: Vec<super::types::PropElement>) -> super::types::PropEdit {
    super::types::PropEdit {
        set,
        clear: Vec::new(),
    }
}

/// An edit that clears the properties with these local names.
pub(super) fn clear_edit(names: &[&str]) -> super::types::PropEdit {
    super::types::PropEdit {
        set: Vec::new(),
        clear: names
            .iter()
            .map(|n| (WNS.to_owned(), (*n).to_owned()))
            .collect(),
    }
}

/// Sets bold.
pub(super) fn bold() -> super::types::PropEdit {
    set_edit(vec![pel("b", &[], vec![])])
}

/// A state whose package also has `word/numbering.xml`, related from the main part.
pub(super) fn state_with_numbering(body: &str, numbering: &str) -> State {
    use stetdoc_testkit::fixtures::zip::Entry;
    let doc = parts::Doc::new().body(body).root_attrs(EXTRA_NS).bytes();
    let rels = "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId5\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/></Relationships>";
    let types = parts::content_types().replace(
        "</Types>",
        "<Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/></Types>",
    );
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    entries.push(Entry::new("word/numbering.xml", numbering));
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    State::open(Zip::new(entries).build(), &Limits::default()).unwrap()
}

/// The bytes of a part of a transaction, as text.
pub(super) fn part_xml(tx: &Tx<'_>, name: &str) -> String {
    let view = tx.parts();
    let id = view.id(name).unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

/// The keys of the blocks of one kind in the body, in order.
pub(super) fn keys_of(tx: &Tx<'_>, kind: BlockKind) -> Vec<BlockKey> {
    let view = tx.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    story
        .blocks()
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| story.key(e))
        .collect()
}
