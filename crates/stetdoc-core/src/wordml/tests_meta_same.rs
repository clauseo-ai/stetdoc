//! Insensitivity (`docs/design/wordml.md` 5.2, item 9) as metamorphic properties through the real
//! `xml::Editor`: a rewrite that changes nothing Word shows changes no mode's text, and the items
//! that were there are where they were. Splitting a run at any character, merging adjacent equal
//! runs, adding and removing `w:proofErr`, bookmarks, comment range markers, `w:permStart`,
//! `w:lastRenderedPageBreak`, `w:rsid*` attributes and `xml:space`, and wrapping runs in a smart
//! tag, custom XML or a hyperlink. The helpers and the sweep are in `tests_meta.rs`.

use super::names::{W_T as W, XML};
use super::tests_gen_data::Rng;
use super::tests_meta::{check_refresh, digest, firsts, map_of, scan, starts, sweep, texts};
use super::*;
use crate::xml::{NodeKind, SpanTree};
use std::collections::HashSet;

// ---- insensitivity ------------------------------------------------------------------------------

/// The element that holds a run's text, if the run is a plain one: properties and one `w:t`.
fn plain_run(tree: &SpanTree, run: NodeId) -> Option<NodeId> {
    let r = tree.node_ref(run);
    let mut kids = r.child_elements().filter(|c| c.local_name() != "rPr");
    let t = kids
        .next()
        .filter(|t| matches!(t.local_name(), "t" | "delText"))?;
    let one_text = t.children().count() == 1 && t.first_child()?.kind() == NodeKind::Text;
    (kids.next().is_none() && one_text).then(|| t.id())
}

fn text_item_of_plain_run(tree: &SpanTree, m: &TextMap, rng: &mut Rng) -> Option<usize> {
    let candidates: Vec<usize> = (0..m.items().len())
        .filter(|&j| {
            let item = &m.items()[j];
            item.kind == ItemKind::Text && item.run.is_some_and(|r| plain_run(tree, r).is_some())
        })
        .collect();
    (!candidates.is_empty()).then(|| candidates[rng.below(candidates.len())])
}

/// A new `w:` element with attributes.
fn element(edited: &mut SpanTree, local: &str, attrs: &[(&str, &str)]) -> NodeId {
    let mut ed = edited.edit();
    let el = ed.create_element(W, local).unwrap();
    for (name, value) in attrs {
        ed.set_attr(el, W, name, value).unwrap();
    }
    el
}

/// The texts of every mode are unchanged, and the old items are where they were.
fn assert_same_reading(before: &TextMap, after: &TextMap, skip: &HashSet<NodeId>) {
    assert_eq!(texts(before), texts(after));
    for mode in Mode::ALL {
        let at = starts(before, mode);
        for (j, item) in firsts(before)
            .into_iter()
            .filter(|(_, it)| !skip.contains(&it.node))
        {
            assert_eq!(
                after.offset_of(mode, item.node, 0),
                Some(at[j]),
                "{mode:?} {item:?}"
            );
        }
    }
}

/// Adds a mark or an attribute that changes nothing Word shows, checks the reading, then removes it
/// and checks that the map is exactly the old one.
fn add_and_remove(story: &Story, tree: &SpanTree, i: usize, rng: &mut Rng) -> Option<()> {
    let para = story.paragraph(tree, i)?;
    let m = para.map();
    let mut edited = tree.clone();
    let id = "9001";
    let mut created: Vec<NodeId> = Vec::new();
    let mut restore: Option<(NodeId, &str, &str)> = None;
    let anchor = (!m.items().is_empty()).then(|| m.items()[rng.below(m.items().len())]);
    let run = anchor.and_then(|a| a.run);
    let choice = rng.below(7);
    match choice {
        0..=4 => {
            let (names, attrs): (&[&str], Vec<(&str, &str)>) = match choice {
                0 => (&["proofErr"], vec![("type", "spellStart")]),
                1 => (
                    &["bookmarkStart", "bookmarkEnd"],
                    vec![("id", id), ("name", "m")],
                ),
                2 => (&["commentRangeStart", "commentRangeEnd"], vec![("id", id)]),
                3 => (&["permStart"], vec![("id", id)]),
                _ => (&["lastRenderedPageBreak"], vec![]),
            };
            for name in names {
                let attrs: Vec<(&str, &str)> = attrs
                    .iter()
                    .filter(|(a, _)| *name != "bookmarkEnd" || *a == "id")
                    .copied()
                    .collect();
                created.push(element(&mut edited, name, &attrs));
            }
            let (parent, before) = match run {
                Some(r) => (edited.node_ref(r).parent()?.id(), Some(r)),
                None => (para.id(), None),
            };
            // a break goes inside a run, the others beside it
            let (parent, before) = if choice == 4 {
                (run.unwrap_or(para.id()), None)
            } else {
                (parent, before)
            };
            let mut ed = edited.edit();
            for &el in &created {
                ed.insert_before(parent, before, el).unwrap();
            }
        }
        5 => {
            let (run, t) = {
                let j = text_item_of_plain_run(&edited, &m, rng)?;
                let run = m.items()[j].run?;
                (run, plain_run(&edited, run)?)
            };
            let name = *rng.pick(&["rsidR", "rsidRPr", "rsidDel"]);
            edited.edit().set_attr(run, W, name, "00A1B2C3").unwrap();
            restore = Some((run, W, name));
            let _ = t;
        }
        _ => {
            let j = text_item_of_plain_run(&edited, &m, rng)?;
            let t = plain_run(&edited, m.items()[j].run?)?;
            let present = edited.node_ref(t).attr(XML, "space").is_some();
            if present {
                edited.edit().remove_attr(t, XML, "space").unwrap();
            } else {
                edited.edit().set_attr(t, XML, "space", "preserve").unwrap();
            }
            restore = Some((t, XML, "space"));
        }
    }
    let scanned = scan(&edited);
    let after = map_of(&scanned, &edited, para.id());
    assert_same_reading(&m, &after, &HashSet::new());
    assert_eq!(
        after.items().len(),
        m.items().len()
            + created
                .len()
                .min(2)
                .min(usize::from(choice != 0 && choice != 4) * 2)
    );
    check_refresh(story, &edited, para.id(), &scanned);
    // and back
    if let Some((el, ns, name)) = restore {
        // restoring xml:space puts the attribute back to what it was
        let was_there = tree.node_ref(el).attr(ns, name).is_some();
        if was_there {
            edited.edit().set_attr(el, ns, name, "preserve").unwrap();
        } else {
            edited.edit().remove_attr(el, ns, name).unwrap();
        }
    }
    for el in created {
        edited.edit().detach(el).unwrap();
    }
    let again = scan(&edited);
    assert_eq!(digest(&map_of(&again, &edited, para.id())), digest(&m));
    Some(())
}

/// Splits the run of a random text item at a random character, as Word does when it saves.
fn split_run(story: &Story, tree: &SpanTree, i: usize, rng: &mut Rng) -> Option<()> {
    let para = story.paragraph(tree, i)?;
    let m = para.map();
    let j = text_item_of_plain_run(tree, &m, rng)?;
    let item = m.items()[j];
    let run = item.run?;
    let k = 1 + rng.below((item.len as usize).saturating_sub(1).max(1));
    if k >= item.len as usize {
        return None;
    }
    let mut edited = tree.clone();
    let parent = edited.node_ref(run).parent()?.id();
    let next = edited.node_ref(run).next_sibling().map(|n| n.id());
    let clone = edited.edit().clone_subtree(run).unwrap();
    let clone_t = plain_run(&edited, clone)?;
    let clone_text = edited.node_ref(clone_t).first_child()?.id();
    {
        let mut ed = edited.edit();
        ed.replace_text_range(item.node, k, item.len as usize, "")
            .unwrap();
        ed.replace_text_range(clone_text, 0, k, "").unwrap();
        ed.insert_before(parent, next, clone).unwrap();
    }
    let scanned = scan(&edited);
    let after = map_of(&scanned, &edited, para.id());
    assert_eq!(after.items().len(), m.items().len() + 1);
    assert_same_reading(&m, &after, &HashSet::from([item.node]));
    for mode in Mode::ALL {
        let at = starts(&m, mode)[j];
        assert_eq!(after.offset_of(mode, item.node, 0), Some(at));
        let second = at + if item.vis.in_mode(mode) { k } else { 0 };
        assert_eq!(
            after.offset_of(mode, clone_text, 0),
            Some(second),
            "{mode:?}"
        );
    }
    check_refresh(story, &edited, para.id(), &scanned);
    Some(())
}

/// Merges two adjacent runs that differ in nothing.
fn merge_runs(story: &Story, tree: &SpanTree, i: usize, rng: &mut Rng) -> Option<()> {
    let para = story.paragraph(tree, i)?;
    let m = para.map();
    let items = m.items();
    let pairs: Vec<usize> = (0..items.len().saturating_sub(1))
        .filter(|&j| {
            let (a, b) = (&items[j], &items[j + 1]);
            let (Some(ra), Some(rb)) = (a.run, b.run) else {
                return false;
            };
            let adjacent = tree
                .node_ref(ra)
                .next_sibling()
                .is_some_and(|n| n.id() == rb);
            let plain = |r| plain_run(tree, r).is_some();
            let props = |r: NodeId| {
                let first = tree.node_ref(r).first_child();
                first
                    .filter(|f| f.local_name() == "rPr")
                    .map(|f| f.source_bytes().to_vec())
            };
            a.kind == ItemKind::Text
                && b.kind == ItemKind::Text
                && adjacent
                && plain(ra)
                && plain(rb)
                && props(ra) == props(rb)
                && a.flags & CDATA == 0
                && b.flags & CDATA == 0
        })
        .collect();
    let j = *pairs.get(rng.below(pairs.len().max(1)))?;
    let (a, b) = (items[j], items[j + 1]);
    let second = tree.node_ref(b.node).text().into_owned();
    let mut edited = tree.clone();
    {
        let mut ed = edited.edit();
        let n = a.len as usize;
        ed.replace_text_range(a.node, n, n, &second).unwrap();
        ed.detach(b.run?).unwrap();
    }
    let scanned = scan(&edited);
    let after = map_of(&scanned, &edited, para.id());
    assert_eq!(after.items().len(), items.len() - 1);
    assert_same_reading(&m, &after, &HashSet::from([b.node]));
    check_refresh(story, &edited, para.id(), &scanned);
    Some(())
}

/// Wraps one run, or a few adjacent ones, in a smart tag, custom XML or a hyperlink.
fn wrap_runs(story: &Story, tree: &SpanTree, i: usize, rng: &mut Rng) -> Option<()> {
    let para = story.paragraph(tree, i)?;
    let m = para.map();
    let anchor = m
        .items()
        .iter()
        .filter_map(|it| it.run)
        .nth(rng.below(m.items().len().max(1)))?;
    let (name, kind, attrs): (&str, WrapperKind, &[(&str, &str)]) = match rng.below(3) {
        0 => (
            "smartTag",
            WrapperKind::SmartTag,
            &[("uri", "u"), ("element", "e")],
        ),
        1 => (
            "customXml",
            WrapperKind::CustomXml,
            &[("uri", "x"), ("element", "y")],
        ),
        _ => ("hyperlink", WrapperKind::Hyperlink, &[("anchor", "a")]),
    };
    let mut edited = tree.clone();
    let mut runs = vec![anchor];
    for _ in 0..rng.below(3) {
        let last = *runs.last()?;
        match edited.node_ref(last).next_sibling() {
            Some(n) if n.local_name() == "r" => runs.push(n.id()),
            _ => break,
        }
    }
    let parent = edited.node_ref(anchor).parent()?.id();
    let wrapper = element(&mut edited, name, attrs);
    {
        let mut ed = edited.edit();
        ed.insert_before(parent, Some(anchor), wrapper).unwrap();
        for &r in &runs {
            ed.detach(r).unwrap();
            ed.append_child(wrapper, r).unwrap();
        }
    }
    let scanned = scan(&edited);
    let after = map_of(&scanned, &edited, para.id());
    assert_same_reading(&m, &after, &HashSet::new());
    assert_eq!(after.items().len(), m.items().len());
    for (old, new) in m.items().iter().zip(after.items()) {
        assert_eq!(
            (old.kind, old.node, old.vis, old.field),
            (new.kind, new.node, new.vis, new.field)
        );
        let (old_chain, new_chain): (Vec<_>, Vec<_>) = (
            m.context(old.ctx).map(|c| c.kind).collect(),
            after.context(new.ctx).map(|c| c.kind).collect(),
        );
        let inside = {
            let mut cur = edited.node_ref(new.node).parent();
            let mut found = false;
            while let Some(a) = cur {
                found |= a.id() == wrapper;
                cur = a.parent();
            }
            found
        };
        if inside {
            assert_eq!(new_chain.len(), old_chain.len() + 1);
            let mut without = new_chain.clone();
            let hits = (0..without.len())
                .filter(|&p| without[p] == kind)
                .collect::<Vec<_>>();
            assert!(
                hits.iter().any(|&p| {
                    let mut c = new_chain.clone();
                    c.remove(p);
                    c == old_chain
                }),
                "{new_chain:?} vs {old_chain:?}"
            );
            without.clear();
        } else {
            assert_eq!(new_chain, old_chain);
        }
    }
    check_refresh(story, &edited, para.id(), &scanned);
    Some(())
}

#[test]
fn marks_and_attributes_that_change_nothing_leave_the_reading_alone() {
    let applied = sweep(40..52, 100, 3, add_and_remove);
    assert!(applied > 1500, "applied {applied} times");
}

#[test]
fn splitting_a_run_anywhere_changes_no_text() {
    let applied = sweep(60..72, 100, 3, split_run);
    assert!(applied > 300, "applied {applied} times");
}

#[test]
fn merging_adjacent_equal_runs_changes_no_text() {
    let applied = sweep(80..104, 100, 3, merge_runs);
    assert!(applied > 200, "applied {applied} times");
}

#[test]
fn wrapping_runs_in_a_smart_tag_custom_xml_or_hyperlink_changes_no_text() {
    let applied = sweep(100..112, 100, 3, wrap_runs);
    assert!(applied > 1000, "applied {applied} times");
}
