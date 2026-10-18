//! The helpers of the laws: the census of revisions, the comparison form of a part, the part after a
//! resolution, the `Final` reading by a join rule of its own, and the part with the touched blocks left
//! out.

use super::index::ChangeIndex;
use super::laws_norm::normalize;
use super::resolve::resolve;
use super::tests_support::*;
use super::types::Step;
use crate::state::State;
use crate::wordml::{BlockKind, Mode, RevKind, part_revisions};
use std::collections::HashSet;

/// What a revision is, for the census: its id, kind, author and date, and its content, which is what it
/// would take with it: the text that an insertion or a deletion holds, and the snapshot of a property
/// change.
pub(super) type Census = Vec<(Option<i64>, RevKind, Option<String>, Option<String>, String)>;

/// The text below a node, visible and deleted alike, with a marker for what is not text, in order. Run
/// properties are left out, because formatting inside a revision is the revision of whoever formatted it.
fn text_below(n: &crate::xml::NodeRef<'_>, out: &mut String) {
    for c in n.children() {
        if !c.is_element() {
            out.push_str(&c.text());
            continue;
        }
        match c.local_name() {
            "rPr"
            | "pPr"
            | "rPrChange"
            | "pPrChange"
            | "proofErr"
            | "lastRenderedPageBreak"
            | "commentReference" => {}
            name if super::join::MARKERS.contains(&name) => {}
            "t" | "delText" | "instrText" | "delInstrText" => out.push_str(&c.text_content()),
            "ins" | "del" | "moveFrom" | "moveTo" | "r" | "hyperlink" | "smartTag" | "sdt"
            | "sdtContent" | "customXml" | "fldSimple" => text_below(&c, out),
            other => {
                out.push('<');
                out.push_str(other);
                if let Some(kind) = super::names::w_attr(&c, "fldCharType") {
                    out.push(' ');
                    out.push_str(&kind);
                }
                out.push('>');
            }
        }
    }
}

/// What a revision element holds, for the census.
fn content_of(tree: &crate::xml::SpanTree, node: crate::wordml::NodeId, kind: RevKind) -> String {
    match kind {
        RevKind::Ins | RevKind::Del | RevKind::MoveFrom | RevKind::MoveTo => {
            let mut out = String::new();
            text_below(&tree.node_ref(node), &mut out);
            out
        }
        RevKind::RunProps
        | RevKind::ParagraphProps
        | RevKind::SectionProps
        | RevKind::TableProps
        | RevKind::TableExProps
        | RevKind::TableGrid
        | RevKind::RowProps
        | RevKind::CellProps
        | RevKind::NumberingChange => dump_node(tree, node),
        _ => String::new(),
    }
}

pub(super) fn census(view: &crate::state::PartsView<'_>) -> Census {
    let id = view.id(MAIN).unwrap();
    let tree = view.tree(id).unwrap();
    part_revisions(tree)
        .into_iter()
        .map(|r| {
            let content = content_of(tree, r.node, r.kind);
            (r.id, r.kind, r.author, r.date, content)
        })
        .collect()
}

pub(super) fn normalized(view: &crate::state::PartsView<'_>) -> String {
    normalize(view.tree(view.id(MAIN).unwrap()).unwrap())
}

/// Removes the bookmarks that the generator places, which are not revisions, so a reject cannot take
/// them away, and which would keep a paragraph alive that a reject removes.
pub(super) fn strip_law_markers(tx: &mut crate::state::Tx<'_>) {
    let id = tx.parts().id(MAIN).unwrap();
    let mut ids: Vec<String> = Vec::new();
    let mut nodes: Vec<crate::wordml::NodeId> = Vec::new();
    {
        let view = tx.parts();
        let tree = view.tree(id).unwrap();
        let mut stack = vec![tree.root()];
        while let Some(n) = stack.pop() {
            if n.local_name() == "bookmarkStart"
                && super::names::w_attr(&n, "name").is_some_and(|v| v.starts_with("lawbm"))
            {
                ids.push(
                    super::names::w_attr(&n, "id")
                        .unwrap_or_default()
                        .to_string(),
                );
                nodes.push(n.id());
            }
            stack.extend(n.child_elements());
        }
        let mut stack = vec![tree.root()];
        while let Some(n) = stack.pop() {
            if n.local_name() == "bookmarkEnd"
                && super::names::w_attr(&n, "id").is_some_and(|v| ids.iter().any(|i| *i == v))
            {
                nodes.push(n.id());
            }
            stack.extend(n.child_elements());
        }
    }
    let tree = tx.part_mut(MAIN).unwrap();
    for n in nodes {
        tree.edit().detach(n).unwrap();
    }
}

/// The normalized main part of a state after a resolution of selected elements. The elements named in
/// `gone` are removed first, each with what it holds: the revisions that a call declared as swallowed.
pub(super) fn after(state: &State, gone: &[i64], steps: &[Step]) -> String {
    after_with(state, gone, steps, false)
}

/// [`after`], with the properties of the last paragraph of the body left out if `last_ppr_free`.
pub(super) fn after_with(
    state: &State,
    gone: &[i64],
    steps: &[Step],
    last_ppr_free: bool,
) -> String {
    let mut tx = state.begin();
    strip_law_markers(&mut tx);
    if !gone.is_empty() {
        let id = tx.parts().id(MAIN).unwrap();
        let nodes: Vec<crate::wordml::NodeId> = part_revisions(tx.parts().tree(id).unwrap())
            .into_iter()
            .filter(|r| r.id.is_some_and(|i| gone.contains(&i)))
            .map(|r| r.node)
            .collect();
        let tree = tx.part_mut(MAIN).unwrap();
        for n in nodes {
            tree.edit().detach(n).unwrap();
        }
    }
    if !steps.is_empty() {
        resolve(&mut tx, &mut ChangeIndex::new(), steps)
            .unwrap_or_else(|e| panic!("resolve failed: {e}"));
    }
    let view = tx.parts();
    let tree = view.tree(view.id(MAIN).unwrap()).unwrap();
    if last_ppr_free {
        super::laws_norm::normalize_without_last_ppr(tree)
    } else {
        normalize(tree)
    }
}

/// True if the last paragraph of the body of a state has a mark that is an insertion.
pub(super) fn ends_in_an_inserted_mark(state: &State) -> bool {
    let view = state.parts();
    let Some(id) = view.id(MAIN) else {
        return false;
    };
    let Ok(story) = view.story(id) else {
        return false;
    };
    story
        .blocks()
        .iter()
        .rev()
        .find(|e| e.kind == BlockKind::Paragraph && e.parent.is_none())
        .is_some_and(|e| e.mark_revs().inserted)
}

/// The first place two strings differ, with some context.
pub(super) fn first_difference(a: &str, b: &str) -> String {
    let at = a
        .bytes()
        .zip(b.bytes())
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    let from = at.saturating_sub(120);
    let cut = |s: &str| {
        s.get(from..(at + 200).min(s.len()))
            .unwrap_or("")
            .to_owned()
    };
    format!(
        "at byte {at}\n  left : ...{}\n  right: ...{}",
        cut(a),
        cut(b)
    )
}

/// The Final reading of the body of a view: see [`final_runs_in`].
pub(super) fn final_runs(view: &crate::state::PartsView<'_>) -> Vec<String> {
    final_runs_in(view, MAIN)
}

/// True if the node is in a row or a cell that a tracked change inserts.
pub(super) fn in_inserted_row_or_cell(
    tree: &crate::xml::SpanTree,
    node: crate::wordml::NodeId,
) -> bool {
    let mut cur = tree.node_ref(node).parent();
    while let Some(n) = cur {
        let revised = |holder: &str, kind: &str| {
            super::names::is_w(&n, if holder == "trPr" { "tr" } else { "tc" })
                && n.child_elements()
                    .find(|c| super::names::is_w(c, if holder == "trPr" { "trPr" } else { "tcPr" }))
                    .is_some_and(|pr| pr.child_elements().any(|c| super::names::is_w(&c, kind)))
        };
        if revised("trPr", "ins") || revised("tcPr", "cellIns") {
            return true;
        }
        cur = n.parent();
    }
    false
}

/// True if a paragraph holds a range marker or a comment reference anywhere in it.
fn holds_markers(tree: &crate::xml::SpanTree, para: crate::wordml::NodeId) -> bool {
    let mut stack = vec![tree.node_ref(para)];
    while let Some(n) = stack.pop() {
        if n.is_element() && super::join::is_leftover(&n) {
            return true;
        }
        stack.extend(n.child_elements());
    }
    false
}

/// The reading of one story part of a view, as a list of paragraph texts: a paragraph whose mark is gone
/// in the mode joins the next one (`Story::join_run`). The last paragraph of a container whose mark is
/// gone and which has no text is left out, because resolving removes it (law R8), unless that would leave
/// its container with none and it held range markers, which resolving keeps a paragraph for, and unless its mark
/// is inserted and it ends its container after a paragraph, which rejecting keeps, emptied (3.2.1). With `lenient`, a
/// paragraph in a row or cell that does not exist in the mode is not read, nor one inside a block-level
/// wrapper that removes it (a `w:moveFrom` around paragraphs, which tools write). The paragraphs of the notes
/// whose ids are in `notes` are not read: a resolution that takes out the last reference to a note removes the
/// note (design 3.6.4), so the reading from before holds paragraphs that the one from after cannot.
pub(super) fn runs_in(
    view: &crate::state::PartsView<'_>,
    part: &str,
    mode: Mode,
    lenient: bool,
    notes: &[i64],
) -> Vec<String> {
    let Some(id) = view.id(part) else {
        return Vec::new();
    };
    let (Ok(story), Ok(tree)) = (view.story(id), view.tree(id)) else {
        return Vec::new();
    };
    let absent = |node: crate::wordml::NodeId| {
        if !lenient {
            return false;
        }
        let (rows, wrappers): (bool, &[&str]) = match mode {
            Mode::Original => (in_inserted_row_or_cell(tree, node), &["ins", "moveTo"]),
            _ => (
                super::compiler::in_deleted_row_or_cell(tree, node),
                &["del", "moveFrom"],
            ),
        };
        rows || {
            let mut cur = tree.node_ref(node).parent();
            let mut found = false;
            while let Some(n) = cur {
                if wrappers.iter().any(|w| super::names::is_w(&n, w)) {
                    found = true;
                    break;
                }
                cur = n.parent();
            }
            found
        }
    };
    // What a run of paragraphs reads as, in which group of paragraphs that are next to each other (the
    // same parent, with no table between), whether it is dropped (its last mark is gone and it has no
    // text) and whether a paragraph in it holds range markers.
    type Group = (Option<crate::wordml::NodeId>, usize);
    struct Piece {
        group: Group,
        text: String,
        dropped: bool,
        markers: bool,
    }
    let mut pieces: Vec<Piece> = Vec::new();
    let mut text = String::new();
    let mut run_markers = false;
    let mut first: Option<crate::wordml::NodeId> = None;
    let mut tables: std::collections::HashMap<Option<crate::wordml::NodeId>, usize> =
        std::collections::HashMap::new();
    for (i, e) in story.blocks().iter().enumerate() {
        let parent = tree.node_ref(e.node).parent();
        if e.kind == BlockKind::Table {
            *tables.entry(parent.map(|p| p.id())).or_default() += 1;
            continue;
        }
        if e.kind != BlockKind::Paragraph || absent(e.node) {
            continue;
        }
        let in_removed_note = usize::try_from(e.unit)
            .ok()
            .and_then(|u| story.units().get(u))
            .and_then(|u| u.id)
            .is_some_and(|id| notes.contains(&id));
        if in_removed_note {
            continue;
        }
        let Some(p) = story.paragraph(tree, i) else {
            continue;
        };
        text.push_str(&p.text(mode));
        run_markers |= holds_markers(tree, e.node);
        let run_first = *first.get_or_insert(e.node);
        if story.join_run(tree, i, mode).len() > 1 {
            continue;
        }
        first = None;
        // Rejecting an inserted mark that ends its container keeps the paragraph (3.2.1), unless the run it ends
        // follows a block that is not a paragraph.
        let kept = mode == Mode::Original
            && crate::wordml::ends_container(tree, e.node)
            && !super::resolve_mark::goes_whole(tree, e.node)
            && !super::resolve_mark::follows_a_block(tree, run_first);
        // A cell always has a paragraph, which the resolution makes if it has none.
        let in_cell = parent.as_ref().is_some_and(|p| p.local_name() == "tc");
        let container = parent.map(|p| p.id());
        pieces.push(Piece {
            group: (container, tables.get(&container).copied().unwrap_or(0)),
            dropped: e.mark_revs().gone_in(mode) && text.is_empty() && !kept,
            text: std::mem::take(&mut text),
            markers: run_markers || in_cell,
        });
        run_markers = false;
    }
    // A group whose paragraphs are all dropped keeps one, to hold the range markers that were in them
    // (a cell keeps one whatever it held): a paragraph joins only the one after it and sends its
    // markers to the one before, so what is left of a run of them is its first.
    let groups: Vec<Group> = pieces.iter().map(|p| p.group).collect();
    let mut kept_one: std::collections::HashSet<Group> = std::collections::HashSet::new();
    for g in groups {
        if kept_one.contains(&g) {
            continue;
        }
        let mine = || pieces.iter().filter(|p| p.group == g);
        if mine().all(|p| p.dropped) && mine().any(|p| p.markers) {
            if let Some(first) = pieces.iter_mut().find(|p| p.group == g) {
                first.dropped = false;
            }
            kept_one.insert(g);
        }
    }
    pieces
        .into_iter()
        .filter(|p| !p.dropped)
        .map(|p| p.text)
        .collect()
}

/// The `Final` reading of a story part: see [`runs_in`].
pub(super) fn final_runs_in(view: &crate::state::PartsView<'_>, part: &str) -> Vec<String> {
    runs_in(view, part, Mode::Final, true, &[])
}

/// The `Original` reading of a story part: see [`runs_in`].
pub(super) fn original_runs_in(view: &crate::state::PartsView<'_>, part: &str) -> Vec<String> {
    runs_in(view, part, Mode::Original, true, &[])
}

/// The reading of a story part in a mode with no paragraph left out for where it sits: what `wordml`
/// reads, joined by its own rule.
pub(super) fn strict_runs_in(
    view: &crate::state::PartsView<'_>,
    part: &str,
    mode: Mode,
) -> Vec<String> {
    runs_in(view, part, mode, false, &[])
}

/// A raw dump of a node: names with prefixes, attributes in order, text.
pub(super) fn dump_node(tree: &crate::xml::SpanTree, id: crate::wordml::NodeId) -> String {
    fn walk(n: &crate::xml::NodeRef<'_>, out: &mut String) {
        if !n.is_element() {
            out.push_str(&n.text());
            return;
        }
        out.push('<');
        out.push_str(n.name());
        for a in n.attrs() {
            out.push_str(&format!(" {}={}", a.name(), a.value()));
        }
        out.push('>');
        for c in n.children() {
            walk(&c, out);
        }
        out.push_str("</>");
    }
    let mut s = String::new();
    walk(&tree.node_ref(id), &mut s);
    s
}

/// A dump of a part in which the touched and the new blocks are left out: a paragraph, a new row or a
/// new table entirely, and a touched row or table down to what it holds (its properties are the block's
/// own). What is left is what a batch may not change.
pub(super) fn skeleton(
    tree: &crate::xml::SpanTree,
    touched: &HashSet<crate::wordml::NodeId>,
    new: &HashSet<crate::wordml::NodeId>,
) -> String {
    fn walk(
        n: &crate::xml::NodeRef<'_>,
        touched: &HashSet<crate::wordml::NodeId>,
        new: &HashSet<crate::wordml::NodeId>,
        out: &mut String,
    ) {
        if !n.is_element() {
            out.push_str(&n.text());
            return;
        }
        let local = n.local_name();
        let id = n.id();
        let block = matches!(local, "p" | "tbl" | "tr") && super::names::is_word_ns(n.namespace());
        // `new` holds blocks, and in the edited tree also the range markers that were in a removed or
        // touched block before and were hoisted between blocks (design 3.6.5, 6.2).
        if new.contains(&id) {
            return;
        }
        if block && touched.contains(&id) {
            match local {
                "p" => return,
                _ => {
                    // The block's own properties are its own; the blocks inside are looked at.
                    out.push_str(&format!("<{local}>"));
                    for c in n.child_elements() {
                        let cl = c.local_name();
                        if matches!(cl, "tblPr" | "tblGrid" | "tblPrEx" | "trPr") {
                            continue;
                        }
                        if cl == "tc" {
                            out.push_str("<tc>");
                            for g in c.child_elements().filter(|g| g.local_name() != "tcPr") {
                                walk(&g, touched, new, out);
                            }
                            out.push_str("</tc>");
                            continue;
                        }
                        walk(&c, touched, new, out);
                    }
                    out.push_str("</>");
                    return;
                }
            }
        }
        out.push('<');
        out.push_str(n.name());
        for a in n.attrs() {
            out.push_str(&format!(" {}={}", a.name(), a.value()));
        }
        out.push('>');
        for c in n.children() {
            walk(&c, touched, new, out);
        }
        out.push_str("</>");
    }
    let mut s = String::new();
    walk(&tree.root(), touched, new, &mut s);
    s
}

pub(super) fn dump(label: &str, body: &str, log: &[String], extra: &str) -> String {
    format!("{label}\nbatch: {log:#?}\ndocument body: {body}\n{extra}")
}

/// The range markers of a tree, but for those below an element that `skip` names.
pub(super) fn range_markers(
    t: &crate::xml::SpanTree,
    skip: &dyn Fn(&crate::xml::NodeRef<'_>) -> bool,
) -> HashSet<crate::wordml::NodeId> {
    let mut out = HashSet::new();
    let mut stack = vec![t.root()];
    while let Some(n) = stack.pop() {
        for c in n.child_elements() {
            if skip(&c) {
                continue;
            }
            if super::join::MARKERS
                .iter()
                .any(|m| super::names::is_w(&c, m))
            {
                out.insert(c.id());
            }
            stack.push(c);
        }
    }
    out
}

/// Where the range markers that a batch made have stood: for each marker that the tree did not have before
/// the batch, the innermost block (a paragraph, a table or a row) that held it at each step that was looked
/// at. A marker that the batch made in a block, and that a later call hoisted out of it when the block went,
/// stands between blocks at the end; this says where it came from.
pub(super) struct MarkerHomes {
    /// Every range marker of the tree before the batch.
    old: HashSet<crate::wordml::NodeId>,
    homes: std::collections::HashMap<crate::wordml::NodeId, HashSet<crate::wordml::NodeId>>,
}

impl MarkerHomes {
    /// Starts from the tree before the batch.
    pub(super) fn new(before: &crate::xml::SpanTree) -> MarkerHomes {
        MarkerHomes {
            old: range_markers(before, &|_| false),
            homes: std::collections::HashMap::new(),
        }
    }

    /// Notes where each marker that the batch made stands now.
    pub(super) fn note(&mut self, tree: &crate::xml::SpanTree) {
        for m in range_markers(tree, &|_| false) {
            if self.old.contains(&m) {
                continue;
            }
            let mut cur = tree.node_ref(m).parent();
            while let Some(n) = cur {
                if super::names::is_word_ns(n.namespace())
                    && matches!(n.local_name(), "p" | "tbl" | "tr")
                {
                    self.homes.entry(m).or_default().insert(n.id());
                    break;
                }
                cur = n.parent();
            }
        }
    }

    /// True if the tree had the marker before the batch.
    pub(super) fn is_old(&self, m: crate::wordml::NodeId) -> bool {
        self.old.contains(&m)
    }

    /// True if the batch made the marker in one of `blocks`, at one of the steps that were looked at.
    pub(super) fn made_in(
        &self,
        m: crate::wordml::NodeId,
        blocks: &HashSet<crate::wordml::NodeId>,
    ) -> bool {
        self.homes
            .get(&m)
            .is_some_and(|h| h.iter().any(|b| blocks.contains(b)))
    }
}

/// The range markers of the edited tree that R6 leaves out because they were hoisted there on purpose. One that
/// was inside a block that went or was touched, and now stands between blocks, keeps its place when its
/// paragraph goes; so does one that the batch made in a block it made or touched, which a later call hoisted.
/// Every other marker is compared: one that stood outside such blocks before, and a new one that the batch
/// wrote anywhere else.
pub(super) fn hoisted_markers(
    old_tree: &crate::xml::SpanTree,
    tree: &crate::xml::SpanTree,
    removed: &HashSet<crate::wordml::NodeId>,
    touched: &HashSet<crate::wordml::NodeId>,
    new: &HashSet<crate::wordml::NodeId>,
    homes: &MarkerHomes,
) -> Vec<crate::wordml::NodeId> {
    let elided = |c: &crate::xml::NodeRef<'_>| {
        removed.contains(&c.id()) || (super::names::is_w(c, "p") && touched.contains(&c.id()))
    };
    let kept = range_markers(old_tree, &elided);
    let blocks: HashSet<_> = touched.iter().chain(new).chain(removed).copied().collect();
    range_markers(tree, &|_| false)
        .into_iter()
        .filter(|m| {
            if homes.is_old(*m) {
                !kept.contains(m)
            } else {
                homes.made_in(*m, &blocks)
            }
        })
        .collect()
}
