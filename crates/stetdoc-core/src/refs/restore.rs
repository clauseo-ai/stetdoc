//! `restore`: reading the persisted table back, and every check on hostile input.
//!
//! The agent can write to the complement, so nothing in the blob is trusted. The blob is checked
//! against the document it claims to describe, a freshly scanned head package (the `Layout`), and
//! then written out again from the table that was built, which must give the same bytes: the format
//! has exactly one encoding, so that one comparison rejects every spelling the line parser did not
//! already catch. See `docs/design/refs-and-state.md`, section 8.4.

use super::base::{Lineage, MAX_EXTRA, MAX_LABEL, MAX_PARENTS, NamespaceId, NamespaceOrigin};
use super::digest::unhex;
use super::errors::{corrupt, limit, unsupported_version};
use super::layout::Layout;
use super::persist::{FORMAT, MAGIC, encode_name, kind_word};
use super::table::{Reason, RefTable, Slot};
use super::token::{BlockRef, RefKind};
use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::wordml::{FINGERPRINT_VERSION, NodeId, SCAN_VERSION};
use std::sync::Arc;

/// The longest line a blob may have: a part name is at most 1,024 bytes, and written out it is at
/// most three times that.
const MAX_LINE: usize = 4096;

/// The lines of a blob, with the number of the one being read for messages.
struct Lines<'a> {
    rest: std::str::Split<'a, char>,
    number: usize,
}

impl<'a> Lines<'a> {
    fn next(&mut self) -> Result<&'a str, StetError> {
        self.number += 1;
        let line = self
            .rest
            .next()
            .ok_or_else(|| corrupt("the blob ends early, so it is cut short"))?;
        if line.len() > MAX_LINE {
            return Err(self.fail("the line is too long"));
        }
        Ok(line)
    }

    fn fail(&self, what: &str) -> StetError {
        corrupt(format!("line {}: {what}", self.number))
    }

    /// The next line, split into fields on single spaces, which must be `words` long and start
    /// with `first`.
    fn record(
        &mut self,
        first: &str,
        words: std::ops::RangeInclusive<usize>,
    ) -> Result<Vec<&'a str>, StetError> {
        let line = self.next()?;
        let fields: Vec<&str> = line.split(' ').collect();
        if fields.first() != Some(&first) || !words.contains(&fields.len()) || fields.contains(&"")
        {
            return Err(self.fail(&format!("expected a {first} line")));
        }
        Ok(fields)
    }
}

/// A number written the one canonical way: decimal, no sign, no leading zero, at most `u32::MAX`.
fn number(text: &str) -> Option<u32> {
    let canonical = !text.is_empty()
        && text.len() <= 10
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    canonical.then(|| text.parse().ok()).flatten()
}

fn fingerprint(text: &str) -> Option<u64> {
    unhex::<8>(text).map(u64::from_be_bytes)
}

fn at_field(lines: &Lines<'_>, text: &str, version: u32) -> Result<u32, StetError> {
    match number(text) {
        Some(at) if (1..=version).contains(&at) => Ok(at),
        _ => Err(lines.fail("the version a ref was retired in is not between 1 and the version")),
    }
}

fn ref_field(lines: &Lines<'_>, text: &str, counts: &[u32; 3]) -> Result<BlockRef, StetError> {
    let r = BlockRef::parse(text).map_err(|_| lines.fail("expected a ref"))?;
    if r.number() > counts.get(r.kind().index()).copied().unwrap_or(0) {
        return Err(lines.fail(&format!("{r} is above the number of refs minted")));
    }
    Ok(r)
}

fn origin_of(
    lines: &Lines<'_>,
    fields: &[&str],
) -> Result<(NamespaceId, NamespaceOrigin), StetError> {
    let bad = || lines.fail("the namespace line is not in its form");
    let id = NamespaceId::parse(fields.get(1).copied().unwrap_or("")).ok_or_else(bad)?;
    let origin = match fields.get(2).copied() {
        Some("opened") if fields.len() == 3 => NamespaceOrigin::Opened,
        Some("derived") => {
            let label = fields.get(3).copied().ok_or_else(bad)?;
            let count = number(fields.get(4).copied().unwrap_or("")).ok_or_else(bad)? as usize;
            if count > MAX_PARENTS || fields.len() != count + 6 {
                return Err(bad());
            }
            let parents = fields
                .iter()
                .skip(5)
                .take(count)
                .map(|p| NamespaceId::parse(p))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(bad)?;
            let extra = match fields.last().copied() {
                Some("-") => Vec::new(),
                Some(hex) if hex.len() % 2 == 0 && hex.len() <= MAX_EXTRA * 2 => {
                    unhex_vec(hex).ok_or_else(bad)?
                }
                _ => return Err(bad()),
            };
            let origin = NamespaceOrigin::Derived {
                label: label.to_owned(),
                parents,
                extra,
            };
            if !origin.within_limits() || label.len() > MAX_LABEL {
                return Err(bad());
            }
            origin
        }
        _ => return Err(bad()),
    };
    Ok((id, origin))
}

fn unhex_vec(text: &str) -> Option<Vec<u8>> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).ok()?;
            unhex::<1>(pair).map(|[b]| b)
        })
        .collect()
}

impl RefTable {
    /// Reads a blob that `persist` wrote, for a document whose stories are `layout`: the head
    /// package, scanned afresh. `original` is the original package bytes, which an opened namespace
    /// is checked against.
    ///
    /// Everything that is not exactly what `persist` writes for this document is refused:
    /// `corrupt_complement` for a blob that is damaged or does not match the layout,
    /// `unsupported_version` for one written with another format, block-scan or fingerprint
    /// version, and `limit_exceeded` for one that would make a table over `Limits::max_refs`.
    pub(crate) fn restore(
        bytes: &[u8],
        layout: &Layout,
        original: Option<&[u8]>,
        limits: &Limits,
    ) -> Result<RefTable, StetError> {
        let max = u64::from(limits.max_refs);
        if bytes.len() as u64 > 64 * max + 65_536 {
            return Err(limit(format!(
                "the ref map is {} bytes, more than {} refs could need",
                bytes.len(),
                limits.max_refs
            )));
        }
        if let Some(at) = bytes
            .iter()
            .position(|b| !(*b == b'\n' || (0x20..=0x7e).contains(b)))
        {
            return Err(corrupt(format!("byte {at} is not printable ASCII")));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| corrupt("the blob is not text"))?;
        let mut lines = Lines {
            rest: text.split('\n'),
            number: 0,
        };

        // The format version, then the rest of the header.
        let first = lines.next()?;
        let version_word = first
            .strip_prefix(MAGIC)
            .and_then(|rest| rest.strip_prefix(' '))
            .ok_or_else(|| corrupt("this is not a ref map"))?;
        match number(version_word) {
            Some(v) if v > FORMAT => {
                return Err(unsupported_version(format!(
                    "this ref map is format {v}, and this release reads format {FORMAT}"
                )));
            }
            Some(FORMAT) => {}
            _ => return Err(lines.fail("the format version is not a version")),
        }
        let ns_fields = lines.record("namespace", 3..=40)?;
        let scan = lines.record("scan", 3..=3)?;
        let recorded = (
            number(scan.get(1).copied().unwrap_or("")),
            number(scan.get(2).copied().unwrap_or("")),
        );
        let (Some(scan_version), Some(fp_version)) = recorded else {
            return Err(lines.fail("the scan line is not in its form"));
        };
        if (scan_version, fp_version) != (SCAN_VERSION, FINGERPRINT_VERSION) {
            return Err(unsupported_version(format!(
                "this working copy was written with block scan version {scan_version} and \
                 fingerprint version {fp_version}, and this release uses {SCAN_VERSION} and \
                 {FINGERPRINT_VERSION}, so it would number the blocks differently"
            )));
        }
        let (namespace, origin) = origin_of(&lines, &ns_fields)?;
        match origin.recompute(original, scan_version) {
            Some(expected) if expected == namespace => {}
            Some(_) => {
                return Err(corrupt(
                    "the namespace does not follow from how the ref map says it was made",
                ));
            }
            None => {
                return Err(corrupt(
                    "the original package bytes are needed to check the namespace, and there are none",
                ));
            }
        }
        let version_line = lines.record("version", 2..=2)?;
        let version = number(version_line.get(1).copied().unwrap_or(""))
            .filter(|v| u64::from(*v) <= max && u64::from(*v) <= bytes.len() as u64 / 17)
            .ok_or_else(|| lines.fail("the version is not a version this table could have"))?;
        let lineage_line = lines.record("lineage", 2..=2)?;
        if number(lineage_line.get(1).copied().unwrap_or("")) != Some(version) {
            return Err(lines.fail("the lineage does not have one link for each version"));
        }
        let mut history = Vec::new();
        for _ in 0..version {
            let link = lines.next()?;
            history
                .push(Lineage::parse(link).ok_or_else(|| lines.fail("expected a lineage link"))?);
        }
        let counts_line = lines.record("counts", 4..=4)?;
        let mut counts = [0u32; 3];
        for (slot, text) in counts.iter_mut().zip(counts_line.iter().skip(1)) {
            *slot = number(text).ok_or_else(|| lines.fail("a count is not a number"))?;
        }
        let total: u64 = counts.iter().map(|c| u64::from(*c)).sum();
        if total > max || total > bytes.len() as u64 / 4 {
            return Err(limit(format!(
                "the ref map claims {total} refs, more than it or this release could hold"
            )));
        }

        // The stories, aligned with the layout.
        let stories_line = lines.record("stories", 2..=2)?;
        if number(stories_line.get(1).copied().unwrap_or("")) != Some(layout.stories.len() as u32) {
            return Err(lines.fail(&format!(
                "the ref map lists a different number of stories than the document has ({})",
                layout.stories.len()
            )));
        }
        let mut slots: [Vec<Option<Slot>>; 3] = counts.map(|c| vec![None; c as usize]);
        let part_names: Vec<Arc<str>> =
            layout.stories.iter().map(|s| Arc::clone(&s.part)).collect();
        for (part_number, story) in layout.stories.iter().enumerate() {
            let head = lines.record("story", 4..=4)?;
            if head.get(1).copied() != Some(encode_name(&story.part).as_str())
                || head.get(2).copied() != Some(kind_word(story.kind))
            {
                return Err(lines.fail(&format!(
                    "this story is not {}, the story the document has here",
                    story.part
                )));
            }
            if number(head.get(3).copied().unwrap_or("")) != Some(story.blocks.len() as u32) {
                return Err(lines.fail(&format!(
                    "{} has {} blocks, and the ref map lists another number",
                    story.part,
                    story.blocks.len()
                )));
            }
            let part = u16::try_from(part_number).map_err(|_| limit("too many parts"))?;
            for (ordinal, block) in story.blocks.iter().enumerate() {
                let fields = lines.record_block()?;
                let r = ref_field(&lines, fields.first().copied().unwrap_or(""), &counts)?;
                if r.kind() != RefKind::of(block.kind) {
                    return Err(lines.fail(&format!(
                        "{r} is block {ordinal} of {}, and the document has another kind of block there",
                        story.part
                    )));
                }
                if fingerprint(fields.get(1).copied().unwrap_or("")) != Some(block.fingerprint) {
                    return Err(lines.fail(&format!(
                        "{r} is block {ordinal} of {}, and its text is not the text it had when \
                         this working copy was exported",
                        story.part
                    )));
                }
                let changed = match fields.get(2) {
                    None => 0,
                    Some(text) => number(text)
                        .filter(|c| (1..=version).contains(c))
                        .ok_or_else(|| {
                            lines.fail(
                                "the version a block changed in is not between 1 and the version",
                            )
                        })?,
                };
                let slot =
                    slot_of(&mut slots, r).ok_or_else(|| lines.fail("a ref is out of range"))?;
                if slot.is_some() {
                    return Err(lines.fail(&format!("{r} is listed twice")));
                }
                *slot = Some(Slot::Live {
                    part,
                    node: node_of(block),
                    changed,
                });
            }
        }

        // The retired refs, in order.
        let gone_line = lines.record("gone", 2..=2)?;
        let gone_count = number(gone_line.get(1).copied().unwrap_or(""))
            .ok_or_else(|| lines.fail("the number of retired refs is not a number"))?;
        let mut previous: Option<BlockRef> = None;
        for _ in 0..gone_count {
            let fields = lines.record_gone()?;
            let r = ref_field(&lines, fields.first().copied().unwrap_or(""), &counts)?;
            if previous.is_some_and(|p| p >= r) {
                return Err(lines.fail("the retired refs are not in order, or one is listed twice"));
            }
            previous = Some(r);
            let (reason, at) = match (fields.get(1).copied(), fields.len()) {
                (Some("d"), 3) => (
                    Reason::Deleted,
                    at_field(&lines, fields.get(2).copied().unwrap_or(""), version)?,
                ),
                (Some("x"), 3) => (
                    Reason::Rejected,
                    at_field(&lines, fields.get(2).copied().unwrap_or(""), version)?,
                ),
                (Some("m"), 4) => (
                    Reason::MergedInto(ref_field(
                        &lines,
                        fields.get(2).copied().unwrap_or(""),
                        &counts,
                    )?),
                    at_field(&lines, fields.get(3).copied().unwrap_or(""), version)?,
                ),
                _ => return Err(lines.fail("a retired ref is not in its form")),
            };
            let slot = slot_of(&mut slots, r).ok_or_else(|| lines.fail("a ref is out of range"))?;
            if slot.is_some() {
                return Err(lines.fail(&format!("{r} is both live and retired")));
            }
            *slot = Some(Slot::Gone { reason, at });
        }
        if lines.next()? != "end" {
            return Err(lines.fail("expected the end line"));
        }
        if lines.rest.next() != Some("") || lines.rest.next().is_some() {
            return Err(corrupt("there are bytes after the end line"));
        }

        // The table must be whole: every number up to the count is live or retired.
        let mut dense: [Vec<Slot>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for kind in RefKind::ALL {
            let source = slots
                .get_mut(kind.index())
                .map(std::mem::take)
                .unwrap_or_default();
            for (i, slot) in source.into_iter().enumerate() {
                let slot = slot.ok_or_else(|| {
                    corrupt(format!(
                        "{}{} is neither live nor retired",
                        kind.letter(),
                        i + 1
                    ))
                })?;
                if let Some(target) = dense.get_mut(kind.index()) {
                    target.push(slot);
                }
            }
        }
        check_merges(&dense, &counts)?;
        let table = RefTable::from_parts(
            namespace,
            origin,
            history,
            &part_names,
            dense,
            limits.max_refs,
        )?;
        // The canonical form: the table, written out again, is the blob.
        if table.persist(layout)? != bytes {
            return Err(corrupt("the blob is not in the one form the format allows"));
        }
        Ok(table)
    }
}

/// The element a layout block is.
fn node_of(block: &super::layout::BlockSpec) -> NodeId {
    block.key.node()
}

fn slot_of(slots: &mut [Vec<Option<Slot>>; 3], r: BlockRef) -> Option<&mut Option<Slot>> {
    let index = usize::try_from(r.number()).ok()?.checked_sub(1)?;
    slots.get_mut(r.kind().index())?.get_mut(index)
}

/// A merged ref points at a ref of the table, not at itself, and at one that is live or was
/// retired in a later version, so a chain of merges cannot loop.
fn check_merges(slots: &[Vec<Slot>; 3], counts: &[u32; 3]) -> Result<(), StetError> {
    let slot = |r: BlockRef| {
        let index = usize::try_from(r.number()).ok()?.checked_sub(1)?;
        slots.get(r.kind().index())?.get(index)
    };
    for kind in RefKind::ALL {
        for (i, s) in slots.get(kind.index()).into_iter().flatten().enumerate() {
            let Slot::Gone {
                reason: Reason::MergedInto(target),
                at,
            } = s
            else {
                continue;
            };
            let me = u32::try_from(i + 1)
                .ok()
                .and_then(|n| BlockRef::new(kind, n));
            if me == Some(*target) {
                return Err(corrupt(format!("{target} is merged into itself")));
            }
            let ok = match slot(*target) {
                Some(Slot::Live { .. }) => true,
                Some(Slot::Gone { at: later, .. }) => later > at,
                None => false,
            };
            if !ok || target.number() > counts.get(target.kind().index()).copied().unwrap_or(0) {
                return Err(corrupt(format!(
                    "{target} cannot be what a ref was merged into in version {at}"
                )));
            }
        }
    }
    Ok(())
}

impl<'a> Lines<'a> {
    /// A live block's line: a ref, a fingerprint and perhaps a version.
    fn record_block(&mut self) -> Result<Vec<&'a str>, StetError> {
        let line = self.next()?;
        let fields: Vec<&str> = line.split(' ').collect();
        if !(2..=3).contains(&fields.len()) || fields.contains(&"") {
            return Err(
                self.fail("expected a block line: a ref, a fingerprint and perhaps a version")
            );
        }
        Ok(fields)
    }

    /// A retired ref's line.
    fn record_gone(&mut self) -> Result<Vec<&'a str>, StetError> {
        let line = self.next()?;
        let fields: Vec<&str> = line.split(' ').collect();
        if !(3..=4).contains(&fields.len()) || fields.contains(&"") {
            return Err(self.fail("expected a retired ref"));
        }
        Ok(fields)
    }
}
