//! The persisted form of the table: a canonical text blob, `stet-refs 1`.
//!
//! The format has exactly one encoding for a table, and `restore` accepts only that encoding, which
//! is the main defence against hostile input. See `docs/design/refs-and-state.md`, section 8.
//!
//! ```text
//! stet-refs 1
//! namespace 3f9a0c11d2b4e577 opened
//! scan 1 1
//! version 2
//! lineage 2
//! 51ac09f2e8b34d11
//! 7be0c4a5d1f93268
//! counts 6 1 2
//! stories 2
//! story word/document.xml body 6
//! t1 9f3a0c11d2b4e577
//! r1 0123456789abcdef 2
//! ...
//! gone 2
//! p4 d 1
//! p5 m p3 2
//! end
//! ```

use super::base::NamespaceOrigin;
use super::digest::hex;
use super::errors::internal;
use super::layout::Layout;
use super::table::{Reason, RefTable, Slot};
use super::token::{BlockRef, RefKind};
use crate::api::error::StetError;
use crate::wordml::{FINGERPRINT_VERSION, SCAN_VERSION, StoryKind};

/// The first word of a blob.
pub(super) const MAGIC: &str = "stet-refs";
/// The version of this format. It changes with every change to the format, because the canonical form
/// leaves no room for optional records.
pub(super) const FORMAT: u32 = 1;

/// How a story kind is written.
pub(super) fn kind_word(kind: StoryKind) -> &'static str {
    match kind {
        StoryKind::Body => "body",
        StoryKind::Header => "header",
        StoryKind::Footer => "footer",
        StoryKind::Footnotes => "footnotes",
        StoryKind::Endnotes => "endnotes",
        StoryKind::Comments => "comments",
    }
}

/// A part name with every byte outside `[A-Za-z0-9._/-]` written as `%XX` in upper-case hex.
pub(super) fn encode_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-') {
            out.push(char::from(b));
        } else {
            out.push('%');
            out.push_str(&hex(&[b]).to_ascii_uppercase());
        }
    }
    out
}

fn origin_line(namespace: &str, origin: &NamespaceOrigin) -> String {
    match origin {
        NamespaceOrigin::Opened => format!("namespace {namespace} opened\n"),
        NamespaceOrigin::Derived {
            label,
            parents,
            extra,
        } => {
            let mut line = format!("namespace {namespace} derived {label} {}", parents.len());
            for p in parents {
                line.push(' ');
                line.push_str(&p.to_string());
            }
            line.push(' ');
            if extra.is_empty() {
                line.push('-');
            } else {
                line.push_str(&hex(extra));
            }
            line.push('\n');
            line
        }
    }
}

impl RefTable {
    /// The table as a blob, with the blocks of each story in the order of `layout`, which must list
    /// every live block once: a block with no ref, or a live ref that no story lists, is a bug in the
    /// caller and is `internal`. The same table and layout always give the same bytes.
    pub(crate) fn persist(&self, layout: &Layout) -> Result<Vec<u8>, StetError> {
        let mut out = String::new();
        out.push_str(&format!("{MAGIC} {FORMAT}\n"));
        out.push_str(&origin_line(&self.namespace().to_string(), self.origin()));
        out.push_str(&format!("scan {SCAN_VERSION} {FINGERPRINT_VERSION}\n"));
        out.push_str(&format!("version {}\n", self.version()));
        out.push_str(&format!("lineage {}\n", self.history().len()));
        for link in self.history() {
            out.push_str(&format!("{link}\n"));
        }
        out.push_str(&format!(
            "counts {} {} {}\n",
            self.count(RefKind::Paragraph),
            self.count(RefKind::Table),
            self.count(RefKind::Row)
        ));
        out.push_str(&format!("stories {}\n", layout.stories.len()));
        let mut listed = 0u64;
        for story in &layout.stories {
            out.push_str(&format!(
                "story {} {} {}\n",
                encode_name(&story.part),
                kind_word(story.kind),
                story.blocks.len()
            ));
            for block in &story.blocks {
                let r = self.ref_of(&block.key).ok_or_else(|| {
                    internal(format!(
                        "a block of {} has no ref, so the table cannot be persisted",
                        story.part
                    ))
                })?;
                if r.kind() != RefKind::of(block.kind) {
                    return Err(internal(format!("{r} names a block of another kind")));
                }
                let changed = match self.slot(r) {
                    Some(Slot::Live { changed, .. }) => *changed,
                    _ => return Err(internal(format!("{r} is not live"))),
                };
                out.push_str(&format!("{r} {:016x}", block.fingerprint));
                if changed > 0 {
                    out.push_str(&format!(" {changed}"));
                }
                out.push('\n');
                listed += 1;
            }
        }
        let live = self
            .slots()
            .iter()
            .flatten()
            .filter(|s| matches!(s, Slot::Live { .. }))
            .count() as u64;
        if live != listed {
            return Err(internal(format!(
                "the table has {live} live refs, and the layout lists {listed} blocks"
            )));
        }
        let mut gone = String::new();
        let mut gone_count = 0u64;
        for kind in RefKind::ALL {
            for (i, slot) in self
                .slots()
                .get(kind.index())
                .into_iter()
                .flatten()
                .enumerate()
            {
                let Slot::Gone { reason, at } = slot else {
                    continue;
                };
                let r = u32::try_from(i + 1)
                    .ok()
                    .and_then(|n| BlockRef::new(kind, n))
                    .ok_or_else(|| internal("a ref number is out of range"))?;
                gone.push_str(&match reason {
                    Reason::Deleted => format!("{r} d {at}\n"),
                    Reason::Rejected => format!("{r} x {at}\n"),
                    Reason::MergedInto(target) => format!("{r} m {target} {at}\n"),
                });
                gone_count += 1;
            }
        }
        out.push_str(&format!("gone {gone_count}\n"));
        out.push_str(&gone);
        out.push_str("end\n");
        Ok(out.into_bytes())
    }
}
