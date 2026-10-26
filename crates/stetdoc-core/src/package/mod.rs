//! The ZIP container: part access and package-level byte preservation.
//!
//! **Owns.** The ZIP container. It gives lazy access to parts, copies the raw entries of
//! untouched parts, and registers new parts in the content types and relationships. It
//! enforces the container's input limits. Only this module writes the ZIP.
//!
//! **Interface.** `Package::{open, part, replace_part, add_part, write}`, and for what a
//! package says about itself `part_names`, `contains`, `content_type`, `relationships` and
//! `main_part`. Part names are ZIP entry names without a leading slash, such as
//! `word/document.xml`, and are compared ignoring ASCII case, as OPC does. A directory entry (a
//! name ending in `/`) is not a part: it is not listed and cannot be read or replaced, but a new
//! part cannot take its name.
//!
//! **Invariants.**
//! - An unmodified package writes back the input bytes. A modified one copies every
//!   untouched entry raw, as one slice: local header, compressed data and data descriptor.
//!   Central records are copied too, with only the local header offset patched. The ZIP comment,
//!   extra fields, any bytes in front of the first entry and any after the end record are kept,
//!   and so are the ZIP64 records, which are patched in step with the end record: a reader that
//!   finds a ZIP64 locator trusts the record over the end record, so the record always gets the
//!   new count, directory size and directory offset.
//! - A package is read as Python's `zipfile`, `unzip` and `bsdtar` read the same file, or it is
//!   refused. The last `PK\5\6` in the file is the end record, and its comment fits in the file;
//!   the central directory ends where the end record, or the ZIP64 end record, begins, and the
//!   offset stored in the end record only tells how far the archive is shifted; a ZIP64 locator
//!   needs a record at the place every reader looks, and agreeing with the end record; entries lie
//!   end to end up to the central directory, with no bytes between them that a reader going through
//!   the file in order could take for an entry; and the bytes in front of the first entry do not
//!   start with `PK`, because that would be another archive. A fake end record inside the archive
//!   comment wins, as it does for those readers, and the file is then refused unless it describes
//!   an archive that reads. What this crate opens, the others open to the same entries, and the
//!   file written after an edit reads that way too.
//! - A replaced part keeps its name, position, flags, extra fields and method. A new part goes
//!   after the existing ones, with the timestamp and host fields of the main document part, so
//!   output is deterministic. An encrypted part cannot be replaced (`unsupported`), because plain
//!   bytes under its flags would be a package no reader can open.
//! - `add_part` either does everything (the part, its content type, its relationship) or nothing:
//!   all the new bytes are built before the package changes.
//! - Parts are read lazily and checked (size, CRC-32) when read. Nothing is inflated at `open`.
//! - Input limits apply. The absolute ones (archive size, entry count, the declared size of an
//!   entry and of all entries) apply at `open`. The compression ratio of an entry applies when its
//!   part is read, because `open` inflates nothing and a package is not refused for an entry that
//!   is never read. A violation is `limit_exceeded`.
//! - Hostile input fails with a stable code and never panics, hangs or allocates the size a lying
//!   header declares: traversal and duplicate names, overlapping entries, a local header that
//!   disagrees with the central directory (in its name or its compression method), truncation,
//!   bombs, and the shapes above that two readers would read differently. An OLE compound file (a
//!   password-protected or legacy document) is `unsupported`. Only the last end record is tried,
//!   so a file that hides one archive behind another is refused and not read as the first.
//! - Content types and relationships are XML, so adding to them goes through the span tree, which
//!   keeps every other byte of the part. OPC parts have no element-order constraints, so these
//!   edits do not use `schema::insert_in_order` (rule 3 is about WordprocessingML parts).
//!   Relationship targets are URI references: `resolve_target` decodes `%XX`, and the target of a
//!   new relationship is written encoded.
//!
//! Relationship data reaches `wordml` through `state`, which depends on both and passes plain data
//! down, so `wordml` never names this module.
//!
//! **Depends on.** `api`, `xml`.
//!
//! **Start reading.** `package/mod.rs`, then `package/layout.rs` (reading the ZIP) and
//! `package/write.rs` (writing it). The design notes are in
//! [`docs/design/package-and-xml.md`](../../../../docs/design/package-and-xml.md).

#![expect(
    unused_imports,
    reason = "the re-exports are the interface that `Document` and `state` name"
)]
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "nothing outside this module uses the container yet"
    )
)]

mod add;
mod codec;
mod endrec;
mod errors;
mod layout;
mod le;
mod names;
pub mod opc;
mod write;

use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::xml::Bytes;
use errors::{invalid, limit, malformed, not_found, unsupported};
use layout::{ENCRYPTION_FLAGS, Layout};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

pub use opc::{ContentTypes, Relationship, Relationships};

/// The compression-ratio limit applies only to parts at least this large. Small, highly
/// compressible parts (an EMF image, a repetitive app.xml) are legitimate, and the size limits
/// bound everything else.
const RATIO_FLOOR: u64 = 16 << 20;

#[derive(Clone)]
enum Slot {
    /// Not changed. The decompressed bytes are cached after the first read.
    Original(OnceLock<Bytes>),
    Replaced(Bytes),
}

#[derive(Clone)]
struct Added {
    name: String,
    content: Bytes,
}

/// A part to add, with the content type to register and an optional relationship.
pub struct NewPart<'a> {
    /// The part name, without a leading slash, such as `word/comments.xml`.
    pub name: &'a str,
    /// The content type to register for the part.
    pub content_type: &'a str,
    /// The part's uncompressed bytes.
    pub bytes: Vec<u8>,
    /// `(source part, relationship type)`: also add a relationship from `source` (empty for the
    /// package itself) to the new part. The new relationship id is returned by `add_part`.
    pub related_from: Option<(&'a str, &'a str)>,
}

/// An opened package. Cloning is cheap: the input and the decompressed parts are shared.
#[derive(Clone)]
pub struct Package {
    src: Bytes,
    layout: Arc<Layout>,
    lookup: Arc<HashMap<String, usize>>,
    slots: Vec<Slot>,
    added: Vec<Added>,
    limits: Limits,
}

impl Package {
    /// Reads the ZIP structure. Nothing is decompressed.
    pub fn open(bytes: Vec<u8>, limits: &Limits) -> Result<Package, StetError> {
        let layout = layout::parse(&bytes, limits)?;
        // Names are unique ignoring ASCII case (checked above), so a lowercased name finds one entry.
        let lookup: HashMap<String, usize> = layout
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.to_ascii_lowercase(), i))
            .collect();
        let slots = layout
            .entries
            .iter()
            .map(|_| Slot::Original(OnceLock::new()))
            .collect();
        Ok(Package {
            src: Bytes::from(bytes),
            layout: Arc::new(layout),
            lookup: Arc::new(lookup),
            slots,
            added: Vec::new(),
            limits: limits.clone(),
        })
    }

    /// The size in bytes of the input the package was opened from, whatever has changed since.
    pub fn opened_len(&self) -> usize {
        self.src.len()
    }

    /// The input the package was opened from, whatever has changed since: a shared handle, so keeping it costs no copy.
    pub fn opened(&self) -> Bytes {
        self.src.clone()
    }

    /// The index of the entry with this name, ignoring ASCII case as OPC compares part names.
    /// Directory entries are found too.
    fn index_of(&self, name: &str) -> Option<usize> {
        self.lookup.get(&name.to_ascii_lowercase()).copied()
    }

    /// The index of the entry of a part. A directory entry (a name ending in `/`) is not a part.
    fn part_index(&self, name: &str) -> Option<usize> {
        let i = self.index_of(name)?;
        let e = self.layout.entries.get(i)?;
        (!e.name.ends_with('/')).then_some(i)
    }

    fn added_index(&self, name: &str) -> Option<usize> {
        self.added
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
    }

    /// Part names in central directory order, then added parts in the order they were added.
    /// Directory entries are not parts and are left out.
    pub fn part_names(&self) -> impl Iterator<Item = &str> {
        self.layout
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .filter(|n| !n.ends_with('/'))
            .chain(self.added.iter().map(|a| a.name.as_str()))
    }

    /// True if a part with this name exists, original or added. Names are compared ignoring ASCII
    /// case.
    pub fn contains(&self, name: &str) -> bool {
        self.part_index(name).is_some() || self.added_index(name).is_some()
    }

    /// The decompressed bytes of a part, or `None` if there is no such part. Names are compared
    /// ignoring ASCII case. The first call inflates the entry and checks its size and CRC-32; later
    /// calls share the result.
    pub fn part(&self, name: &str) -> Result<Option<Bytes>, StetError> {
        if let Some(i) = self.part_index(name) {
            return self.read_original(i).map(Some);
        }
        Ok(self
            .added_index(name)
            .and_then(|i| self.added.get(i))
            .map(|a| a.content.clone()))
    }

    fn read_original(&self, i: usize) -> Result<Bytes, StetError> {
        let (Some(e), Some(slot)) = (self.layout.entries.get(i), self.slots.get(i)) else {
            return Err(not_found("no such part"));
        };
        let cell = match slot {
            Slot::Replaced(b) => return Ok(b.clone()),
            Slot::Original(cell) => cell,
        };
        if let Some(b) = cell.get() {
            return Ok(b.clone());
        }
        if (e.flags | e.lfh_flags) & ENCRYPTION_FLAGS != 0 {
            return Err(unsupported(format!("part {:?} is encrypted", e.name)));
        }
        let data = usize::try_from(e.csize)
            .ok()
            .and_then(|c| e.data_start.checked_add(c))
            .and_then(|end| self.src.get(e.data_start..end))
            .ok_or_else(|| malformed(format!("part {:?}: data is out of range", e.name)))?;
        let size = usize::try_from(e.usize_)
            .map_err(|_| limit(format!("part {:?} is too large for this platform", e.name)))?;
        // The ratio limit guards inflation, so it is checked when a part is read, not at open: a
        // package is not refused for an entry that is never decompressed.
        if e.method == 8
            && e.usize_ >= RATIO_FLOOR
            && e.usize_ / e.csize.max(1) > u64::from(self.limits.max_compression_ratio)
        {
            return Err(limit(format!(
                "part {:?} compresses {}:1, over the limit of {}:1",
                e.name,
                e.usize_ / e.csize.max(1),
                self.limits.max_compression_ratio
            )));
        }
        let arc: Bytes = match e.method {
            0 => {
                if e.csize != e.usize_ {
                    return Err(malformed(format!(
                        "part {:?}: stored size mismatch",
                        e.name
                    )));
                }
                Bytes::from_part(data.to_vec())
            }
            8 => {
                // Deflate cannot expand by more than about 1032 to 1, so a larger declared size is a lie.
                // Refusing it here keeps a lying header from making us allocate the declared size.
                if e.usize_ > e.csize.saturating_mul(1032).saturating_add(64) {
                    return Err(malformed(format!(
                        "part {:?}: declares {} bytes from {} compressed bytes, which deflate cannot produce",
                        e.name, e.usize_, e.csize
                    )));
                }
                codec::inflate_to_bytes(data, size, &e.name)?
            }
            m => {
                return Err(unsupported(format!(
                    "part {:?} uses compression method {m}",
                    e.name
                )));
            }
        };
        if codec::crc32(&arc) != e.crc {
            return Err(malformed(format!(
                "part {:?}: CRC-32 does not match",
                e.name
            )));
        }
        let _ = cell.set(arc.clone());
        Ok(arc)
    }

    /// Replaces the bytes of an existing part. The entry keeps its name, position and
    /// metadata, and is recompressed with the method it used. An encrypted part, or one stored
    /// with a method other than stored or deflate, is `unsupported`: writing plain bytes under its
    /// flags would make a package that no reader can open.
    pub fn replace_part(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), StetError> {
        self.check_replace(name, bytes.len())?;
        self.replace_checked(name, bytes);
        Ok(())
    }

    /// Everything that can refuse a replacement, so that callers can check before they change
    /// anything.
    fn check_replace(&self, name: &str, len: usize) -> Result<(), StetError> {
        if len as u64 > self.limits.max_entry_bytes {
            return Err(limit(format!(
                "part {name:?} would be {len} bytes, over the limit"
            )));
        }
        if let Some(e) = self
            .part_index(name)
            .and_then(|i| self.layout.entries.get(i))
        {
            if (e.flags | e.lfh_flags) & ENCRYPTION_FLAGS != 0 {
                return Err(unsupported(format!(
                    "part {name:?} is encrypted, so it cannot be replaced"
                )));
            }
            if e.method != 0 && e.method != 8 {
                return Err(unsupported(format!(
                    "part {name:?} uses compression method {}",
                    e.method
                )));
            }
            return Ok(());
        }
        if self.added_index(name).is_some() {
            return Ok(());
        }
        Err(not_found(format!("no part named {name:?}")))
    }

    /// Replaces a part that `check_replace` accepted. It cannot fail.
    fn replace_checked(&mut self, name: &str, bytes: Vec<u8>) {
        if let Some(i) = self.part_index(name) {
            if let Some(slot) = self.slots.get_mut(i) {
                *slot = Slot::Replaced(Bytes::from(bytes));
            }
        } else if let Some(i) = self.added_index(name)
            && let Some(a) = self.added.get_mut(i)
        {
            a.content = Bytes::from(bytes);
        }
    }

    /// The content type registered for a part.
    pub fn content_type(&self, name: &str) -> Result<Option<String>, StetError> {
        let Some(b) = self.part(opc::CONTENT_TYPES_PART)? else {
            return Ok(None);
        };
        Ok(ContentTypes::parse(&b, &self.limits)?
            .type_of(name)
            .map(str::to_owned))
    }

    /// The relationships of a source part (empty string for the package itself).
    pub fn relationships(&self, source: &str) -> Result<Relationships, StetError> {
        let name = opc::rels_part_name(source);
        match self.part(&name)? {
            Some(b) => Relationships::parse(&name, &b, &self.limits),
            None => Ok(Relationships::default()),
        }
    }

    /// The main document part's name: the target of the `officeDocument` relationship, else
    /// `word/document.xml` if present.
    pub fn main_part(&self) -> Option<&str> {
        let i = write::main_part_index(self)?;
        self.layout.entries.get(i).map(|e| e.name.as_str())
    }

    /// True if a part was replaced or added.
    pub fn is_modified(&self) -> bool {
        !self.added.is_empty() || self.slots.iter().any(|s| matches!(s, Slot::Replaced(_)))
    }

    /// The package bytes. If nothing was modified these are the input bytes.
    pub fn write(&self) -> Result<Vec<u8>, StetError> {
        if !self.is_modified() {
            return Ok(self.src.to_vec());
        }
        write::rebuild(self)
    }

    /// Always takes the rebuild path, even with nothing modified. The tests and the fuzz targets
    /// use it to prove that the code `replace_part` relies on reproduces the input.
    #[cfg(any(test, feature = "fuzzing"))]
    pub fn write_rebuilt(&self) -> Result<Vec<u8>, StetError> {
        write::rebuild(self)
    }

    /// The raw bytes of an entry (local header, data, descriptor), for L2 checks.
    #[cfg(any(test, feature = "fuzzing"))]
    pub fn raw_region(&self, name: &str) -> Option<&[u8]> {
        let e = self.layout.entries.get(self.index_of(name)?)?;
        self.src.get(e.lfh..e.entry_end)
    }

    /// The raw central record of an entry, for L2 checks.
    #[cfg(test)]
    pub fn raw_central_record(&self, name: &str) -> Option<&[u8]> {
        let e = self.layout.entries.get(self.index_of(name)?)?;
        self.src.get(e.cd.clone())
    }

    /// Structural facts about the container, for the census.
    #[cfg(test)]
    pub fn describe(&self) -> Vec<String> {
        let l = &self.layout;
        let mut f = Vec::new();
        if l.prefix_end > 0 {
            f.push("prefix-bytes".to_owned());
        }
        if l.zip64.is_some() {
            f.push("zip64-end-records".to_owned());
        }
        if l.base != 0 {
            f.push("offset-base-shift".to_owned());
        }
        if u16_comment_len(&self.src, l.eocd_pos) > 0 {
            f.push("archive-comment".to_owned());
        }
        if l.eocd_pos + 22 + u16_comment_len(&self.src, l.eocd_pos) < self.src.len() {
            f.push("tail-bytes".to_owned());
        }
        if l.local_order.iter().enumerate().any(|(k, &i)| k != i) {
            f.push("cd-order-differs".to_owned());
        }
        for e in &l.entries {
            f.push(format!("method-{}", e.method));
            if e.dd.is_some() {
                f.push(
                    if e.dd.is_some_and(|d| d.signature) {
                        "dd-with-signature"
                    } else {
                        "dd-without-signature"
                    }
                    .to_owned(),
                );
            }
            if e.lfh_zip64 {
                f.push("entry-zip64".to_owned());
            }
            if (e.flags | e.lfh_flags) & ENCRYPTION_FLAGS != 0 {
                f.push("encrypted".to_owned());
            }
            if e.flags != e.lfh_flags {
                f.push("lfh-flags-differ".to_owned());
            }
            if e.name.ends_with('/') {
                f.push("directory-entry".to_owned());
            }
        }
        f.sort();
        f.dedup();
        f
    }
}

fn u16_comment_len(src: &[u8], eocd_pos: usize) -> usize {
    le::u16_at(src, eocd_pos + 20).map_or(0, usize::from)
}

#[cfg(test)]
mod corpus_laws;
#[cfg(test)]
mod hostile_tests;
#[cfg(test)]
mod reference_reader;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_locate;
#[cfg(test)]
mod tests_write;
#[cfg(test)]
mod testzip;
