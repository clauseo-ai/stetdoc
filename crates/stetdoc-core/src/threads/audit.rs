//! How consistent a document's comment parts are: findings with a stable code.
//!
//! `check` compares the findings before and after a batch under the no-new-violations rule, so a
//! finding that the input already has is reported and never blocks. An **error** is a state that Word
//! may repair or that a reader cannot follow. A **note** is legal and worth saying.

use super::anchors;
use super::parts;
use super::read::Threads;
use crate::api::error::StetError;
use crate::state::PartsView;
use crate::wordml::{IdKind, StoryKind, existing_ids};
use std::collections::{HashMap, HashSet};

/// How serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Class {
    /// Word may repair it, or a reader cannot follow it.
    Error,
    /// Legal, and worth saying.
    Note,
}

/// One thing the audit found.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Finding {
    /// The stable name of the rule.
    pub(crate) code: &'static str,
    /// Error or note.
    pub(crate) class: Class,
    /// The comment it is about, if it is about one.
    pub(crate) comment: Option<i64>,
    /// The part it is about, if it is about one.
    pub(crate) part: Option<String>,
    /// A sentence that says what is wrong.
    pub(crate) message: String,
}

fn finding(
    code: &'static str,
    class: Class,
    comment: Option<i64>,
    part: Option<&str>,
    message: String,
) -> Finding {
    Finding {
        code,
        class,
        comment,
        part: part.map(str::to_owned),
        message,
    }
}

/// What the markers of one id add up to.
#[derive(Default)]
struct Tally {
    starts: usize,
    ends: usize,
    references: usize,
}

/// The findings for the comment parts of a document, in rule order and then document order. A document
/// with no comments part, no extension part and no marker has none.
pub(crate) fn audit(view: &PartsView<'_>) -> Result<Vec<Finding>, StetError> {
    let threads = Threads::of(view)?;
    let markers = anchors::markers(view)?;
    let mut out = Vec::new();
    let comments = threads.comments();
    let comments_name = |view: &PartsView<'_>| {
        threads
            .parts
            .comments
            .and_then(|p| view.name(p))
            .map(|n| n.to_string())
    };
    let part_name = comments_name(view);

    // Ids and markers.
    let mut seen_ids = HashSet::new();
    let mut reported = HashSet::new();
    for c in comments {
        if !seen_ids.insert(c.id) && reported.insert(c.id) {
            out.push(finding(
                "comment_id_duplicate",
                Class::Error,
                Some(c.id),
                part_name.as_deref(),
                format!("Two comments have the id c{}.", c.id),
            ));
        }
    }
    let mut tally: HashMap<i64, Tally> = HashMap::new();
    for m in &markers.0 {
        let t = tally.entry(m.anchor.id).or_default();
        t.starts += usize::from(m.anchor.start.is_some());
        t.ends += usize::from(m.anchor.end.is_some());
        t.references += usize::from(m.anchor.reference.is_some());
        if m.anchor.start.is_some() != m.anchor.end.is_some() {
            out.push(finding(
                "marker_unpaired",
                Class::Error,
                Some(m.anchor.id),
                Some(&m.story),
                format!(
                    "The range of comment c{} has a {} and no {}.",
                    m.anchor.id,
                    if m.anchor.start.is_some() {
                        "start"
                    } else {
                        "end"
                    },
                    if m.anchor.start.is_some() {
                        "end"
                    } else {
                        "start"
                    },
                ),
            ));
        }
    }
    let mut orphan_ids: Vec<i64> = tally
        .keys()
        .filter(|id| !seen_ids.contains(id))
        .copied()
        .collect();
    orphan_ids.sort_unstable();
    for id in orphan_ids {
        out.push(finding(
            "marker_without_comment",
            Class::Error,
            Some(id),
            None,
            format!("A range marker or reference names comment c{id}, which does not exist."),
        ));
    }
    let mut checked = HashSet::new();
    for c in comments {
        if !checked.insert(c.id) {
            continue;
        }
        let t = tally.get(&c.id);
        let (s, e, r) = t.map_or((0, 0, 0), |t| (t.starts, t.ends, t.references));
        if s > 0 && e > 0 && r == 0 {
            out.push(finding(
                "reference_missing",
                Class::Error,
                Some(c.id),
                None,
                format!(
                    "Comment c{} has a range and no reference run, so Word does not show it.",
                    c.id
                ),
            ));
        }
        if r > 1 {
            out.push(finding(
                "reference_duplicate",
                Class::Error,
                Some(c.id),
                None,
                format!("Comment c{} has {r} reference runs.", c.id),
            ));
        }
        if s + e + r == 0 {
            out.push(finding(
                "unanchored",
                Class::Note,
                Some(c.id),
                None,
                format!(
                    "Comment c{} has no range and no reference, so Word does not show it.",
                    c.id
                ),
            ));
        }
    }

    // Paragraph ids of the comments.
    let paras: HashSet<u32> = comments.iter().filter_map(|c| c.para_id).collect();
    let mut owners: HashMap<u32, i64> = HashMap::new();
    for c in comments {
        if let Some(p) = c.para_id {
            owners.entry(p).or_insert(c.id);
        }
    }
    for c in comments {
        if c.para_id.is_none() {
            out.push(finding(
                "para_id_missing",
                Class::Note,
                Some(c.id),
                part_name.as_deref(),
                format!(
                    "The last paragraph of comment c{} has no paraId, so it cannot be threaded.",
                    c.id
                ),
            ));
        }
    }
    if let Some(part) = threads.parts.comments {
        let mut all = Vec::new();
        existing_ids(IdKind::ParaId, view.tree(part)?, &mut all);
        let mut seen = HashSet::new();
        let mut reported = HashSet::new();
        for id in &all {
            if !seen.insert(*id) && reported.insert(*id) {
                out.push(finding(
                    "para_id_duplicate",
                    Class::Error,
                    owners.get(id).copied(),
                    part_name.as_deref(),
                    format!("Two paragraphs of the comments have the paraId {id:08X}."),
                ));
            }
        }
        let mut high: Vec<u32> = seen
            .iter()
            .copied()
            .filter(|id| *id >= 0x8000_0000)
            .collect();
        high.sort_unstable();
        for id in high {
            out.push(finding(
                "para_id_range",
                Class::Note,
                owners.get(&id).copied(),
                part_name.as_deref(),
                format!("The paraId {id:08X} is at or above 0x80000000, which the schema does not allow."),
            ));
        }
        // a comment paragraph id that a paragraph of another story also has
        for spec in view
            .story_specs()?
            .0
            .iter()
            .filter(|s| s.kind != StoryKind::Comments)
        {
            let Some(other) = view.id(&spec.part) else {
                continue;
            };
            let mut ids = Vec::new();
            existing_ids(IdKind::ParaId, view.tree(other)?, &mut ids);
            let mut hit: Vec<u32> = ids.into_iter().filter(|i| seen.contains(i)).collect();
            hit.sort_unstable();
            hit.dedup();
            for id in hit {
                if reported.insert(id) {
                    out.push(finding(
                        "para_id_duplicate",
                        Class::Error,
                        owners.get(&id).copied(),
                        Some(&spec.part),
                        format!("A paragraph of {} and a paragraph of a comment have the paraId {id:08X}.", spec.part),
                    ));
                }
            }
        }
    }

    // Rows.
    let rows = &threads.rows;
    let ex_name = threads
        .parts
        .extended
        .and_then(|p| view.name(p))
        .map(|n| n.to_string());
    let ids_name = threads
        .parts
        .ids
        .and_then(|p| view.name(p))
        .map(|n| n.to_string());
    let ext_name = threads
        .parts
        .extensible
        .and_then(|p| view.name(p))
        .map(|n| n.to_string());
    for row in &rows.extended {
        match row.para {
            Some(p) if paras.contains(&p) => {}
            _ => out.push(finding(
                "row_without_comment",
                Class::Error,
                None,
                ex_name.as_deref(),
                format!("A row of commentsExtended names paragraph {}, which is no comment's last paragraph.", show(row.para)),
            )),
        }
    }
    for row in &rows.ids {
        match row.para {
            Some(p) if paras.contains(&p) => {}
            _ => out.push(finding(
                "row_without_comment",
                Class::Error,
                None,
                ids_name.as_deref(),
                format!("A row of commentsIds names paragraph {}, which is no comment's last paragraph.", show(row.para)),
            )),
        }
    }
    let ids_durable: HashSet<u32> = rows.ids.iter().filter_map(|r| r.durable).collect();
    if rows.has_ids {
        for row in &rows.extensible {
            match row.durable {
                Some(d) if ids_durable.contains(&d) => {}
                _ => out.push(finding(
                    "row_without_comment",
                    Class::Error,
                    None,
                    ext_name.as_deref(),
                    format!("A row of commentsExtensible names durable id {}, which no row of commentsIds has.", show(row.durable)),
                )),
            }
        }
    }
    let ex_paras: HashSet<u32> = rows.extended.iter().filter_map(|r| r.para).collect();
    let id_paras: HashSet<u32> = rows.ids.iter().filter_map(|r| r.para).collect();
    let ext_durable: HashSet<u32> = rows.extensible.iter().filter_map(|r| r.durable).collect();
    for c in comments {
        if let Some(p) = c.para_id {
            if rows.has_extended && !ex_paras.contains(&p) {
                out.push(finding("row_missing", Class::Note, Some(c.id), ex_name.as_deref(),
                    format!("Comment c{} has no row in commentsExtended, so Word reads it as open and with no parent.", c.id)));
            }
            if rows.has_ids && !id_paras.contains(&p) {
                out.push(finding(
                    "row_missing",
                    Class::Note,
                    Some(c.id),
                    ids_name.as_deref(),
                    format!("Comment c{} has no row in commentsIds.", c.id),
                ));
            }
        }
        if let Some(d) = c.durable_id
            && rows.has_extensible
            && !ext_durable.contains(&d)
        {
            out.push(finding(
                "row_missing",
                Class::Note,
                Some(c.id),
                ext_name.as_deref(),
                format!("Comment c{} has no row in commentsExtensible.", c.id),
            ));
        }
    }
    for (i, row) in rows.extended.iter().enumerate() {
        if row.has_parent && !row.parent.is_some_and(|p| paras.contains(&p)) {
            let owner = row.para.and_then(|p| owners.get(&p).copied());
            out.push(finding(
                "parent_missing",
                Class::Note,
                owner,
                ex_name.as_deref(),
                format!("Row {} of commentsExtended names a parent paragraph that is no comment's last paragraph.", i + 1),
            ));
        }
    }
    // Loops of parents, once per loop, at the member that comes first in the file.
    for first in &threads.loops {
        out.push(finding(
            "thread_cycle",
            Class::Error,
            comments.get(*first).map(|c| c.id),
            ex_name.as_deref(),
            "Comments name each other as parents in a loop.".to_owned(),
        ));
    }
    // Durable ids used twice within a part.
    for (name, list) in [
        (
            &ids_name,
            rows.ids
                .iter()
                .filter_map(|r| r.durable)
                .collect::<Vec<_>>(),
        ),
        (
            &ext_name,
            rows.extensible
                .iter()
                .filter_map(|r| r.durable)
                .collect::<Vec<_>>(),
        ),
    ] {
        let mut seen = HashSet::new();
        let mut reported = HashSet::new();
        for d in list {
            if !seen.insert(d) && reported.insert(d) {
                out.push(finding(
                    "durable_mismatch",
                    Class::Error,
                    None,
                    name.as_deref(),
                    format!("The durable id {d:08X} is used by two rows."),
                ));
            }
        }
    }
    // A part that exists and that nothing relates.
    for (kind, path) in parts::default_paths() {
        let related = match kind {
            crate::schema::PartKind::Comments => threads.parts.comments,
            crate::schema::PartKind::CommentsExtended => threads.parts.extended,
            crate::schema::PartKind::CommentsIds => threads.parts.ids,
            crate::schema::PartKind::CommentsExtensible => threads.parts.extensible,
            _ => threads.parts.people,
        };
        if related.is_none() && view.contains(path) {
            out.push(finding(
                "part_unrelated",
                Class::Error,
                None,
                Some(path),
                format!("{path} is in the package, and the main part has no relationship to it."),
            ));
        }
    }
    Ok(out)
}

fn show(v: Option<u32>) -> String {
    v.map_or_else(|| "(none)".to_owned(), |v| format!("{v:08X}"))
}
