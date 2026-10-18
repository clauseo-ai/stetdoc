//! The operations on properties: `set`, and the typed `format`, `paragraph`, `style` and `list` over it (design
//! `document-and-ops.md` 2.1, 2.3 and 2.7).
//!
//! Each builds a `PropEdit` and makes one `set_props` call on the container it names, which writes the matching `*PrChange` when
//! the edit is tracked. The typed fields write what Word's own interface writes, as `ops/props_typed.rs` lists. `set` takes
//! property elements by name, with human units for their values. A section is named by its number; the last section's
//! properties are the body's own `w:sectPr`, which no paragraph holds, so under a scope `set` on it is `scope_violation`.

use super::context::{Context, Done, SideEffect};
use super::props_typed;
use super::target::{self, Within, warning};
use super::units;
use crate::api::error::{ErrorCode, StetError};
use crate::api::op::{FormatOp, ListOp, ParagraphOp, PropSet, SetOp, StyleOp};
use crate::api::view::WarningCode;
use crate::refs::BaseToken;
use crate::revisions::compiler::Compiler;
use crate::revisions::types::{PropEdit, PropElement, PropTarget};
use crate::state::Tx;
use crate::wordml::{BlockKey, BlockKind, Paragraph, StoryKind, StyleKind};

pub(super) fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The namespace of the `w` vocabulary as the part of `key` writes it: Transitional or Strict.
pub(super) fn w_ns(tx: &Tx<'_>, key: &BlockKey) -> Result<String, StetError> {
    let view = tx.parts();
    let id = view.id(key.part()).ok_or_else(|| {
        StetError::new(
            ErrorCode::Internal,
            "internal: a block's part is not in the document",
        )
    })?;
    Ok(view.tree(id)?.node_ref(key.node()).namespace().to_string())
}

/// A property element in the `w` namespace.
pub(super) fn element(ns: &str, local: &str, attrs: &[(&str, String)]) -> PropElement {
    PropElement {
        ns: ns.to_string(),
        local: local.to_string(),
        attrs: attrs
            .iter()
            .map(|(name, value)| (ns.to_string(), (*name).to_string(), value.clone()))
            .collect(),
        children: Vec::new(),
    }
}

/// A local name as the payload gives it: a `w:` prefix is accepted, and it must be a name.
fn local_name(name: &str) -> Result<String, StetError> {
    let local = name.trim().trim_start_matches("w:");
    let ok = local
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && local.chars().all(|c| c.is_ascii_alphanumeric());
    if ok {
        Ok(local.to_string())
    } else {
        Err(invalid(format!(
            "`{name}` is not a property name. Name an element of the w vocabulary, such as jc or spacing."
        )))
    }
}

/// One property element of `set`, with its values in the attributes' own units.
fn prop_element(ns: &str, p: &PropSet) -> Result<PropElement, StetError> {
    let local = local_name(&p.name)?;
    let mut attrs: Vec<(String, String, String)> = Vec::new();
    if let Some(val) = &p.val {
        attrs.push((
            ns.to_string(),
            "val".to_string(),
            units::convert(&local, "val", val)?,
        ));
    }
    for (name, value) in &p.attrs {
        let attr = local_name(name)?;
        let converted = units::convert(&local, &attr, value)?;
        attrs.push((ns.to_string(), attr, converted));
    }
    let children = p
        .children
        .iter()
        .map(|c| prop_element(ns, c))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PropElement {
        ns: ns.to_string(),
        local,
        attrs,
        children,
    })
}

/// The names of `clear`, as `(namespace, local name)`.
fn clear_names(ns: &str, names: &[String]) -> Result<Vec<(String, String)>, StetError> {
    names
        .iter()
        .map(|n| local_name(n).map(|l| (ns.to_string(), l)))
        .collect()
}

/// True for an element that records a revision, which only the compiler writes and only `accept` and `reject` remove: the text
/// and mark insertions and deletions, the moves and their ranges, every `*PrChange`, `tblGridChange`, `numberingChange`, the cell
/// insertions, deletions and merges, and the custom XML ranges of a revision.
fn is_revision(local: &str) -> bool {
    matches!(
        local,
        "ins" | "del" | "moveFrom" | "moveTo" | "cellIns" | "cellDel" | "cellMerge"
    ) || local.ends_with("Change")
        || local.starts_with("moveFromRange")
        || local.starts_with("moveToRange")
        || local.starts_with("customXmlIns")
        || local.starts_with("customXmlDel")
        || local.starts_with("customXmlMove")
}

/// The first revision element of an element, its children or theirs.
fn revision_in(e: &PropElement) -> Option<&str> {
    if is_revision(&e.local) {
        return Some(&e.local);
    }
    e.children.iter().find_map(revision_in)
}

/// What a tracked property change of this container cannot record, because the snapshot its `*PrChange` holds has no place for
/// it: a section break (`w:sectPr`) in a paragraph's properties (`CT_PPrBase`), and the header and footer references of a
/// section (`CT_SectPrBase`).
fn untrackable(target: &PropTarget, local: &str) -> Option<&'static str> {
    match (target, local) {
        (PropTarget::Paragraph { .. }, "sectPr") => Some(
            "a section break is not a property change Word can track, so in 1.0 it is added only by an untracked edit",
        ),
        (
            PropTarget::Section { .. } | PropTarget::BodySection { .. },
            "headerReference" | "footerReference",
        ) => Some(
            "the header and footer references are not among the properties a section's tracked change keeps, so they change only in an untracked edit",
        ),
        _ => None,
    }
}

/// Refuses an edit that would write or remove revision markup, which would forge or erase someone's change, and a property that a
/// tracked change of the container cannot record. The mark's `w:rPr` belongs to the `mark` target, never to a paragraph's set.
fn guard(target: &PropTarget, edit: &PropEdit, tracked: bool, what: &str) -> Result<(), StetError> {
    let revision = edit.set.iter().find_map(revision_in).or_else(|| {
        edit.clear
            .iter()
            .map(|(_, l)| l.as_str())
            .find(|l| is_revision(l))
    });
    if let Some(local) = revision {
        return Err(invalid(format!(
            "{what} names w:{local}, which records a revision. Revisions are written by the edits that make them and removed by `accept` and `reject`, never set or cleared as properties."
        )));
    }
    let names = edit
        .set
        .iter()
        .map(|e| e.local.as_str())
        .chain(edit.clear.iter().map(|(_, l)| l.as_str()));
    for local in names {
        if matches!(target, PropTarget::Paragraph { .. }) && local == "rPr" {
            return Err(invalid(format!(
                "{what} names w:rPr, the properties of the paragraph mark. Set them with `target: mark`."
            )));
        }
        if tracked && let Some(why) = untrackable(target, local) {
            return Err(invalid(format!("{what} names w:{local}: {why}.")));
        }
    }
    super::props_names::allowed(target, edit)
}

/// Makes the call, and says when it changed nothing.
pub(super) fn apply(
    c: &mut Compiler<'_, '_>,
    target: &PropTarget,
    edit: &PropEdit,
    named: &BlockKey,
    what: &str,
) -> Result<Done, StetError> {
    if edit.is_empty() {
        return Err(invalid(format!(
            "{what} names no property to set or clear."
        )));
    }
    guard(target, edit, c.tracked(), what)?;
    let effect = c.set_props(target, edit)?;
    let mut done = Done::naming(&[named]);
    if effect.is_empty() {
        done.warnings.push(warning(
            WarningCode::NoChange,
            c.tx(),
            Some(named),
            format!("{what} changed nothing: the properties already read that way."),
        ));
    }
    done.effects.push(effect);
    Ok(done)
}

/// The block of the paragraph that ends section `n` (from 1), or `None` for the last section, which the body's own `w:sectPr`
/// holds. The number is the one `inspect sections` shows.
fn section(tx: &Tx<'_>, n: u32) -> Result<(Option<BlockKey>, std::sync::Arc<str>), StetError> {
    let view = tx.parts();
    let specs = view.story_specs()?;
    let body = specs
        .0
        .iter()
        .find(|s| s.kind == StoryKind::Body)
        .ok_or_else(|| StetError::new(ErrorCode::Internal, "internal: the document has no body"))?;
    let part = view
        .id(&body.part)
        .ok_or_else(|| StetError::new(ErrorCode::Internal, "internal: the body is not a part"))?;
    let (story, tree) = (view.story(part)?, view.tree(part)?);
    let rels = view.rels(part)?;
    let sections = story.sections(tree, &rels);
    let found = n
        .checked_sub(1)
        .and_then(|i| sections.get(i as usize))
        .ok_or_else(|| {
            StetError::new(
                ErrorCode::NotFound,
                format!(
                    "the document has no section {n}: it has {}.",
                    sections.len()
                ),
            )
            .with_next("Number a section as `inspect sections` shows it, from 1.")
        })?;
    let block = found
        .ends_at
        .and_then(|i| story.blocks().get(i))
        .map(|e| story.key(e));
    Ok((block, body.part.clone()))
}

/// `set`: any property of a paragraph, run, paragraph mark, section, table, row or cell.
pub(super) fn set(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &SetOp,
) -> Result<Done, StetError> {
    let within = Within {
        find: op.find.as_deref(),
        occurrence: op.occurrence,
        start: op.start,
        end: op.end,
    };
    let asked = op.target.as_deref().map(str::trim);
    if asked == Some("section") || (op.section.is_some() && op.r#ref.is_none()) {
        let n = op.section.ok_or_else(|| {
            invalid("`target: section` takes `section`, the number `inspect sections` shows.")
        })?;
        let (block, part) = section(c.tx(), n)?;
        let (target, named) = match block {
            Some(block) => (
                PropTarget::Section {
                    block: block.clone(),
                },
                block,
            ),
            None => {
                if let Some(scope) = cx.scope {
                    return Err(StetError::new(
                        ErrorCode::ScopeViolation,
                        format!(
                            "section {n} is the last section, whose properties are the body's own and name no paragraph, so no scope ({}) holds them.",
                            scope.text()
                        ),
                    )
                    .with_next("Set the last section's properties in a batch without a scope."));
                }
                let body = body_key(c.tx(), &part)?;
                (PropTarget::BodySection { part }, body)
            }
        };
        let ns = w_ns(c.tx(), &named)?;
        let edit = PropEdit {
            set: op
                .props
                .iter()
                .map(|p| prop_element(&ns, p))
                .collect::<Result<_, _>>()?,
            clear: clear_names(&ns, op.clear.as_deref().unwrap_or_default())?,
        };
        let mut done = apply(c, &target, &edit, &named, &format!("set on section {n}"))?;
        if matches!(target, PropTarget::BodySection { .. }) {
            // The body's own section properties belong to no block: the operation names none.
            done.targets.clear();
        }
        return Ok(done);
    }
    let name = op
        .r#ref
        .as_deref()
        .ok_or_else(|| invalid("`set` takes `ref`, or `section` with `target: section`."))?;
    let key = target::block(c.tx(), base, name)?;
    let (kind, _) = target::kind_of(c.tx(), &key)?;
    let ranged = !within.is_whole();
    let chosen = asked.unwrap_or(match (kind, op.cell.is_some(), ranged) {
        (BlockKind::Paragraph, _, true) => "run",
        (BlockKind::Paragraph, _, false) => "paragraph",
        (BlockKind::Table, ..) => "table",
        (BlockKind::Row, true, _) => "cell",
        (BlockKind::Row, false, _) => "row",
    });
    let target = match (chosen, kind) {
        ("paragraph", BlockKind::Paragraph) => PropTarget::Paragraph { block: key.clone() },
        ("mark", BlockKind::Paragraph) => PropTarget::ParagraphMark { block: key.clone() },
        ("run", BlockKind::Paragraph) => {
            let para = target::paragraph(c.tx(), &key, name)?;
            let found = target::range(c.tx(), &para, name, &within, cx.unit)?;
            PropTarget::Run {
                block: key.clone(),
                range: found.range,
            }
        }
        ("table", BlockKind::Table) => PropTarget::Table { table: key.clone() },
        ("row", BlockKind::Row) => PropTarget::Row { row: key.clone() },
        ("cell", BlockKind::Row) => {
            let cell = op
                .cell
                .and_then(|n| n.checked_sub(1))
                .ok_or_else(|| invalid("`target: cell` takes `cell`, counted from 1."))?;
            PropTarget::Cell {
                row: key.clone(),
                cell: cell as usize,
            }
        }
        (
            "paragraph" | "mark" | "run" | "table" | "row" | "cell",
            BlockKind::Paragraph | BlockKind::Table | BlockKind::Row,
        ) => {
            return Err(invalid(format!(
                "`target: {chosen}` does not fit {name}, which is a {}.",
                match kind {
                    BlockKind::Paragraph => "paragraph",
                    BlockKind::Table => "table",
                    BlockKind::Row => "row",
                }
            )));
        }
        (other, _) => {
            return Err(invalid(format!(
                "`target` is paragraph, run, mark, section, table, row or cell, not `{other}`."
            )));
        }
    };
    if ranged && chosen != "run" {
        return Err(invalid(
            "a text range (`find`, `start`, `end`) names runs: use it with `target: run`.",
        ));
    }
    let ns = w_ns(c.tx(), &key)?;
    let edit = PropEdit {
        set: op
            .props
            .iter()
            .map(|p| prop_element(&ns, p))
            .collect::<Result<_, _>>()?,
        clear: clear_names(&ns, op.clear.as_deref().unwrap_or_default())?,
    };
    apply(c, &target, &edit, &key, &format!("set on {name}"))
}

/// The key of the body element of a part, for the receipt of an edit of the body's own section properties.
fn body_key(tx: &Tx<'_>, part: &std::sync::Arc<str>) -> Result<BlockKey, StetError> {
    let view = tx.parts();
    let id = view
        .id(part)
        .ok_or_else(|| StetError::new(ErrorCode::Internal, "internal: the body is not a part"))?;
    Ok(BlockKey::new(part.clone(), view.tree(id)?.root().id()))
}

/// `format`: run formatting on a range, or on the whole paragraph.
pub(super) fn format(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &FormatOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    let para = target::paragraph(c.tx(), &key, &op.r#ref)?;
    let within = Within {
        find: op.find.as_deref(),
        occurrence: op.occurrence,
        start: op.start,
        end: op.end,
    };
    let found = target::range(c.tx(), &para, &op.r#ref, &within, cx.unit)?;
    let ns = w_ns(c.tx(), &key)?;
    let edit = props_typed::run_edit(c.tx(), &ns, op)?;
    let target = PropTarget::Run {
        block: key.clone(),
        range: found.range,
    };
    apply(c, &target, &edit, &key, &format!("format {}", op.r#ref))
}

/// `paragraph`: paragraph formatting.
pub(super) fn paragraph(
    c: &mut Compiler<'_, '_>,
    base: &BaseToken,
    op: &ParagraphOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    target::paragraph(c.tx(), &key, &op.r#ref)?;
    let ns = w_ns(c.tx(), &key)?;
    let edit = props_typed::paragraph_edit(c.tx(), &key, &ns, op)?;
    let target = PropTarget::Paragraph { block: key.clone() };
    apply(c, &target, &edit, &key, &format!("paragraph {}", op.r#ref))
}

/// The `w:styleId` of a style of `kind`, by id first and then by name, ignoring case. A style that does not exist is
/// `invalid_payload` and the message lists the nearest names of that kind (design 2.5), and one name that two styles share is
/// `ambiguous_match`.
pub(super) fn style_id(tx: &Tx<'_>, kind: StyleKind, given: &str) -> Result<String, StetError> {
    let defs = tx.parts().style_defs()?;
    let of_kind = || defs.styles().iter().filter(|s| s.kind == kind);
    if let Some(def) = of_kind().find(|s| &*s.id == given) {
        return Ok(def.id.to_string());
    }
    let named: Vec<&crate::wordml::StyleDef> = of_kind()
        .filter(|s| s.name.eq_ignore_ascii_case(given.trim()))
        .collect();
    let what = match kind {
        StyleKind::Character => "character",
        StyleKind::Table => "table",
        StyleKind::Numbering => "numbering",
        _ => "paragraph",
    };
    match named.as_slice() {
        [one] => Ok(one.id.to_string()),
        [] => {
            let names: Vec<String> = of_kind().map(|s| s.name.to_string()).collect();
            let message = if names.is_empty() {
                format!("the document has no {what} style `{given}`, and no {what} styles at all.")
            } else {
                format!(
                    "the document has no {what} style `{given}`. The nearest are {}.",
                    super::insert::nearest(given, &names)
                )
            };
            Err(invalid(message)
                .with_next("Name a style by its id or by its name as the view shows it."))
        }
        several => Err(StetError::new(
            ErrorCode::AmbiguousMatch,
            format!(
                "`{given}` names {} {what} styles ({}). Name one by its id.",
                several.len(),
                several
                    .iter()
                    .map(|s| format!("`{}`", s.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// `style`: the paragraph style.
pub(super) fn style(
    c: &mut Compiler<'_, '_>,
    base: &BaseToken,
    op: &StyleOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    target::paragraph(c.tx(), &key, &op.r#ref)?;
    let id = style_id(c.tx(), StyleKind::Paragraph, &op.style)?;
    let ns = w_ns(c.tx(), &key)?;
    let edit = PropEdit {
        set: vec![element(&ns, "pStyle", &[("val", id)])],
        clear: Vec::new(),
    };
    let target = PropTarget::Paragraph { block: key.clone() };
    apply(c, &target, &edit, &key, &format!("style {}", op.r#ref))
}

/// The list a paragraph is in: its own `w:numPr`, else the one its style gives.
fn list_of(tx: &Tx<'_>, key: &BlockKey) -> Result<(Option<i64>, u8), StetError> {
    let view = tx.parts();
    let id = view.id(key.part()).ok_or_else(|| {
        StetError::new(
            ErrorCode::Internal,
            "internal: a block's part is not in the document",
        )
    })?;
    let tree = view.tree(id)?;
    let p = Paragraph::new(tree, key.node()).ok_or_else(|| {
        StetError::new(
            ErrorCode::Internal,
            "internal: a paragraph entry is not a paragraph",
        )
    })?;
    let own = p.num_pr();
    let from_style = view.cascade()?.style_numbering(p.style_id().as_deref());
    let num_id = own.and_then(|n| n.num_id).or(from_style.num_id);
    let ilvl = own.and_then(|n| n.ilvl).or(from_style.ilvl).unwrap_or(0);
    Ok((num_id.filter(|n| *n > 0), ilvl))
}

/// The name of the numbering part, for the side effect of a restart.
fn numbering_part(tx: &Tx<'_>) -> Option<String> {
    let view = tx.parts();
    let specs = view.story_specs().ok()?;
    let body = specs.0.iter().find(|s| s.kind == StoryKind::Body)?;
    let main = view.id(&body.part)?;
    let id = view
        .related_part(main, &|t| t.ends_with("/relationships/numbering"))
        .ok()??;
    view.name(id).map(|n| n.to_string())
}

/// `list`: a list level, a restart at a number, or no list.
pub(super) fn list(
    c: &mut Compiler<'_, '_>,
    base: &BaseToken,
    op: &ListOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    target::paragraph(c.tx(), &key, &op.r#ref)?;
    let ns = w_ns(c.tx(), &key)?;
    let (num_id, ilvl) = list_of(c.tx(), &key)?;
    let not_listed = || {
        invalid(format!(
            "{} is not in a list, so it has no level to change and no numbering to restart.",
            op.r#ref
        ))
    };
    let num_pr = |level: u8, num: i64| PropElement {
        children: vec![
            element(&ns, "ilvl", &[("val", level.to_string())]),
            element(&ns, "numId", &[("val", num.to_string())]),
        ],
        ..element(&ns, "numPr", &[])
    };
    let mut side = None;
    let set = match (op.level, op.restart, op.remove) {
        (Some(level), None, None) => {
            let level = u8::try_from(level)
                .ok()
                .filter(|l| *l <= 8)
                .ok_or_else(|| {
                    invalid(format!("a list has levels 0 to 8, and {level} is not one."))
                })?;
            num_pr(level, num_id.ok_or_else(not_listed)?)
        }
        (None, Some(at), None) => {
            let num = num_id.ok_or_else(not_listed)?;
            let fresh = c.new_list_instance(num, ilvl, i64::from(at))?;
            side = numbering_part(c.tx());
            num_pr(ilvl, fresh)
        }
        (None, None, Some(true)) => num_pr(0, 0),
        _ => {
            return Err(invalid(
                "`list` takes exactly one of `level`, `restart` and `remove: true`.",
            ));
        }
    };
    let edit = PropEdit {
        set: vec![set],
        clear: Vec::new(),
    };
    let target = PropTarget::Paragraph { block: key.clone() };
    let mut done = apply(c, &target, &edit, &key, &format!("list {}", op.r#ref))?;
    if let Some(part) = side {
        done.side_effects.push(SideEffect {
            part,
            why: format!(
                "the list instance that restarts the numbering of {}",
                op.r#ref
            ),
            styles: None,
        });
    }
    Ok(done)
}
