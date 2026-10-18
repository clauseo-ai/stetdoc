//! The edits the `xml` target makes: one is chosen from four bytes, made, and checked in the tree.

use super::is_edit_refusal;
use super::xml_laws::{count_elements, identities, identities_are_kept, of_kind, walk};
use crate::xml::{NodeId, NodeKind, NodeRef, SpanTree};

/// The namespace of what edits create.
const FUZZ_NS: &str = "urn:fuzz";

/// Text that is hard to write: markup characters, white space that a parser normalizes, non-ASCII
/// and astral characters, two characters XML forbids, and a character reference.
const VALUES: [&str; 10] = [
    "",
    "x",
    "two words",
    "<&>\"'",
    "\t\n\r",
    "\r\n",
    "é€😀",
    "\u{0}",
    "\u{FFFE}",
    "&#x41;",
];

/// One edit, with everything it needs copied out of the tree it will change.
#[derive(Debug)]
pub(super) enum Edit {
    SetAttr {
        el: NodeId,
        ns: String,
        local: String,
        value: &'static str,
    },
    RemoveAttr {
        el: NodeId,
        ns: String,
        local: String,
    },
    SetText {
        node: NodeId,
        value: &'static str,
        /// The text afterwards.
        expected: String,
    },
    ReplaceRange {
        node: NodeId,
        start: usize,
        end: usize,
        with: &'static str,
        /// The text afterwards, when the range lies inside the text.
        expected: Option<String>,
    },
    AppendElement {
        parent: NodeId,
        ns: String,
    },
    AppendText {
        parent: NodeId,
        value: &'static str,
    },
    Detach {
        node: NodeId,
    },
    CloneBefore {
        node: NodeId,
        parent: NodeId,
        /// How many elements the copy adds.
        elements: usize,
    },
}

/// What an edit that succeeded left behind.
enum Done {
    Nothing,
    Removed(bool),
    Created(NodeId),
}

/// Chooses the edit from the four bytes: its kind, a position and a value. `None` when the tree
/// has nothing to put that kind of edit on.
pub(super) fn plan(tree: &SpanTree, head: [u8; 4]) -> Option<Edit> {
    let [kind, hi, lo, v] = head;
    let pick = usize::from(hi) << 8 | usize::from(lo);
    let value = VALUES.get(usize::from(v) % VALUES.len()).copied()?;
    let all = walk(tree.document());
    let elements = of_kind(&all, NodeKind::Element);
    let element = *elements.get(pick.checked_rem(elements.len())?)?;
    match kind % 8 {
        0 | 1 => {
            let own: Vec<_> = element
                .attrs()
                .filter(|a| !a.is_namespace_declaration())
                .collect();
            let existing = own
                .get(pick.checked_rem(own.len()).unwrap_or(0))
                .map(|a| (a.namespace().to_owned(), a.local_name().to_owned()));
            if kind % 8 == 1 {
                let (ns, local) = existing.unwrap_or((String::new(), "absent".to_owned()));
                return Some(Edit::RemoveAttr {
                    el: element.id(),
                    ns,
                    local,
                });
            }
            let (ns, local) = match existing {
                Some(found) if v & 1 == 0 => found,
                _ => (
                    if v & 2 == 0 { FUZZ_NS } else { "" }.to_owned(),
                    "fz".to_owned(),
                ),
            };
            Some(Edit::SetAttr {
                el: element.id(),
                ns,
                local,
                value,
            })
        }
        2 | 3 => {
            let texts = of_kind(&all, NodeKind::Text);
            let text = *texts.get(pick.checked_rem(texts.len())?)?;
            // Outside the root element only white space can be written, as itself, because a
            // reference is not allowed there, so a carriage return in it is read as a line feed.
            let outside = text
                .parent()
                .is_some_and(|p| p.kind() == NodeKind::Document);
            let shown = if outside {
                as_read(value)
            } else {
                value.to_owned()
            };
            if kind % 8 == 2 {
                return Some(Edit::SetText {
                    node: text.id(),
                    value,
                    expected: shown,
                });
            }
            let start = usize::from(v >> 4);
            let end = start + usize::from(v & 7);
            let old: Vec<char> = text.text().chars().collect();
            let expected = (end <= old.len()).then(|| {
                let mut s: String = old.iter().take(start).collect();
                s.push_str(&shown);
                s.extend(old.iter().skip(end));
                s
            });
            Some(Edit::ReplaceRange {
                node: text.id(),
                start,
                end,
                with: value,
                expected,
            })
        }
        4 => Some(Edit::AppendElement {
            parent: element.id(),
            ns: if v & 1 == 0 {
                FUZZ_NS.to_owned()
            } else {
                element.namespace().to_owned()
            },
        }),
        5 => Some(Edit::AppendText {
            parent: element.id(),
            value,
        }),
        6 => {
            // Anything but the document and its root element, which an edit may not remove.
            let below: Vec<NodeRef<'_>> = all
                .iter()
                .map(|(n, _)| *n)
                .filter(|n| n.kind() != NodeKind::Document && n.id() != tree.root().id())
                .collect();
            Some(Edit::Detach {
                node: below.get(pick.checked_rem(below.len())?)?.id(),
            })
        }
        _ => {
            let node = *elements
                .iter()
                .skip(1)
                .nth(pick.checked_rem(elements.len().saturating_sub(1))?)?;
            Some(Edit::CloneBefore {
                node: node.id(),
                parent: node.parent()?.id(),
                elements: count_elements(node),
            })
        }
    }
}

/// What text written as itself reads as: a carriage return line feed pair, and a carriage return
/// alone, are read as a line feed.
fn as_read(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Makes the edit. A refusal is fine, and so is any change that leaves the tree readable. What the
/// edit said to do must be true of the tree afterwards.
pub(super) fn apply(tree: &mut SpanTree, edit: &Edit) -> Result<(), String> {
    let elements_before = count_elements(tree.document());
    let nodes_before = identities(tree);
    let result = {
        let mut ed = tree.edit();
        match edit {
            Edit::SetAttr {
                el,
                ns,
                local,
                value,
            } => ed.set_attr(*el, ns, local, value).map(|()| Done::Nothing),
            Edit::RemoveAttr { el, ns, local } => ed.remove_attr(*el, ns, local).map(Done::Removed),
            Edit::SetText { node, value, .. } => ed.set_text(*node, value).map(|()| Done::Nothing),
            Edit::ReplaceRange {
                node,
                start,
                end,
                with,
                ..
            } => ed
                .replace_text_range(*node, *start, *end, with)
                .map(|()| Done::Nothing),
            Edit::AppendElement { parent, ns } => ed
                .create_element(ns, "fz")
                .and_then(|c| ed.append_child(*parent, c).map(|()| Done::Created(c))),
            Edit::AppendText { parent, value } => ed
                .create_text(value)
                .and_then(|c| ed.append_child(*parent, c).map(|()| Done::Created(c))),
            Edit::Detach { node } => ed.detach(*node).map(|()| Done::Nothing),
            Edit::CloneBefore { node, parent, .. } => ed.clone_subtree(*node).and_then(|c| {
                ed.insert_before(*parent, Some(*node), c)
                    .map(|()| Done::Created(c))
            }),
        }
    };
    identities_are_kept(tree, &nodes_before)?;
    match result {
        Err(e) if is_edit_refusal(&e) => Ok(()),
        Err(e) => Err(format!(
            "{edit:?} failed with {:?}, which is not how an edit is refused: {e}",
            e.code()
        )),
        Ok(done) => expect_done(tree, edit, &done, elements_before),
    }
}

/// What the edit said must be true of the tree now.
fn expect_done(
    tree: &SpanTree,
    edit: &Edit,
    done: &Done,
    elements_before: usize,
) -> Result<(), String> {
    let wrong = |what: String| Err(format!("{edit:?}: {what}"));
    match (edit, done) {
        (
            Edit::SetAttr {
                el,
                ns,
                local,
                value,
            },
            _,
        ) => {
            let got = tree.node_ref(*el).attr(ns, local);
            if got.as_deref() != Some(*value) {
                return wrong(format!("the attribute reads back as {got:?}"));
            }
        }
        (Edit::RemoveAttr { el, ns, local }, Done::Removed(true)) => {
            if tree.node_ref(*el).attr(ns, local).is_some() {
                return wrong("the attribute is still there".into());
            }
        }
        (Edit::RemoveAttr { .. }, Done::Removed(false)) => {
            if tree.is_dirty() {
                return wrong("removing an attribute that is not there changed the tree".into());
            }
        }
        (Edit::SetText { node, expected, .. }, _) => {
            let got = tree.node_ref(*node).text();
            if got != expected.as_str() {
                return wrong(format!("the text reads back as {got:?}, not {expected:?}"));
            }
        }
        (
            Edit::ReplaceRange {
                node,
                expected: Some(want),
                ..
            },
            _,
        ) => {
            let got = tree.node_ref(*node).text();
            if got != want.as_str() {
                return wrong(format!("the text reads back as {got:?}, not {want:?}"));
            }
        }
        (Edit::AppendElement { parent, ns }, Done::Created(c)) => {
            let last = tree.node_ref(*parent).last_child();
            if last.map(|n| n.id()) != Some(*c) || !tree.node_ref(*c).is(ns, "fz") {
                return wrong(
                    "the new element is not the last child, or has the wrong name".into(),
                );
            }
        }
        (Edit::AppendText { parent, value }, Done::Created(c)) => {
            let last = tree.node_ref(*parent).last_child();
            if last.map(|n| n.id()) != Some(*c) || tree.node_ref(*c).text() != *value {
                return wrong(
                    "the new text is not the last child, or reads back differently".into(),
                );
            }
        }
        (Edit::Detach { node }, _) => {
            if tree.node_ref(*node).parent().is_some() {
                return wrong("the node still has a parent".into());
            }
        }
        (
            Edit::CloneBefore {
                node,
                parent,
                elements,
            },
            Done::Created(c),
        ) => {
            let kids: Vec<NodeId> = tree.node_ref(*parent).children().map(|k| k.id()).collect();
            let at = kids.iter().position(|k| k == node);
            let before = at.and_then(|i| i.checked_sub(1)).and_then(|i| kids.get(i));
            if before != Some(c) {
                return wrong("the copy is not the sibling right before the original".into());
            }
            let now = count_elements(tree.document());
            if now != elements_before + elements {
                return wrong(format!(
                    "there are {now} elements, not {elements_before} and {elements} more"
                ));
            }
        }
        _ => {}
    }
    Ok(())
}
