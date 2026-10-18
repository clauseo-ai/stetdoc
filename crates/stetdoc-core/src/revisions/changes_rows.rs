//! Rows as changes: a row revision and the revisions of the same side, author and date in its cells.

use super::changes::{ChangeKind, Group, take};
use super::changes_text::Atoms;
use crate::wordml::{BlockKind, RevKind, Story};
use crate::xml::SpanTree;
use std::ops::Range;

/// The end of the entries that belong to entry `i`: a row's cells' blocks follow it in pre-order.
pub(super) fn subtree_end(story: &Story, i: usize) -> usize {
    let entries = story.blocks();
    let mut j = i + 1;
    while let Some(e) = entries.get(j) {
        let mut cur = e.parent;
        let mut inside = false;
        while let Some(p) = cur {
            if p as usize == i {
                inside = true;
                break;
            }
            cur = entries.get(p as usize).and_then(|x| x.parent);
        }
        if !inside {
            break;
        }
        j += 1;
    }
    j
}

/// Forms the `RowInserted` and `RowDeleted` changes, claiming what they take.
pub(super) fn row_groups(
    story: &Story,
    _tree: &SpanTree,
    atoms: &Atoms,
    claimed: &mut [bool],
    groups: &mut Vec<Group>,
) {
    for i in atoms.window.clone() {
        let Some(e) = story.blocks().get(i) else {
            break;
        };
        if e.kind != BlockKind::Row {
            continue;
        }
        let end = subtree_end(story, i);
        let own: Vec<usize> = atoms
            .by_block
            .get(&i)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|&a| {
                        atoms
                            .list
                            .get(a)
                            .is_some_and(|r| matches!(r.kind, RevKind::RowIns | RevKind::RowDel))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for r in own {
            let Some(key) = atoms.key(r) else { continue };
            let kind = if key.0 == RevKind::Ins {
                ChangeKind::RowInserted
            } else {
                ChangeKind::RowDeleted
            };
            let mut members = vec![r];
            for b in i..end {
                for &a in atoms.by_block.get(&b).into_iter().flatten() {
                    let same = atoms.key(a).as_ref() == Some(&key);
                    let inner = atoms.list.get(a).is_some_and(|x| {
                        !matches!(
                            x.kind,
                            RevKind::RowIns | RevKind::RowDel | RevKind::NumPrIns
                        )
                    });
                    if same && inner {
                        members.push(a);
                    }
                }
            }
            let span: Range<usize> = i..end;
            take(claimed, groups, kind, members, Some(span));
        }
    }
}
