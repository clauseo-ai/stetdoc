//! One entry of a fixture archive, with every header field under the caller's control.

use super::Descriptor;

/// One archive entry. Every field has the value a normal writer would use until changed.
#[derive(Clone, Debug)]
pub struct Entry {
    pub(super) name: Vec<u8>,
    pub(super) data: Vec<u8>,
    pub(super) method: u16,
    pub(super) level: u32,
    pub(super) descriptor: Descriptor,
    pub(super) utf8: Option<bool>,
    pub(super) extra_local: Vec<u8>,
    pub(super) extra_central: Vec<u8>,
    pub(super) comment: Vec<u8>,
    pub(super) mtime: Option<(u16, u16)>,
    pub(super) version_made: (u8, u8),
    pub(super) version_needed: u16,
    pub(super) external: u32,
    pub(super) internal: u16,
    pub(super) crc: Option<u32>,
    pub(super) usize: Option<u32>,
    pub(super) cdata: Option<Vec<u8>>,
    pub(super) local_name: Option<Vec<u8>>,
    pub(super) flags_extra: u16,
    pub(super) zip64: bool,
    pub(super) csize: Option<u32>,
    pub(super) local_time: Option<Option<(u16, u16)>>,
    pub(super) cd_offset: Option<u32>,
}

/// DOS time and date for 2024-05-17 10:30:12.
const DEFAULT_TIME: (u16, u16) = (
    (10 << 11) | (30 << 5) | 6,
    ((2024 - 1980) << 9) | (5 << 5) | 17,
);

/// DOS time and date for 1980-01-01 00:00:00, which Word writes.
pub const DOS_1980: (u16, u16) = (0, 0x0021);

impl Entry {
    /// A deflated entry with default metadata.
    pub fn new(name: impl AsRef<[u8]>, data: impl AsRef<[u8]>) -> Entry {
        Entry {
            name: name.as_ref().to_vec(),
            data: data.as_ref().to_vec(),
            method: 8,
            level: 6,
            descriptor: Descriptor::None,
            utf8: None,
            extra_local: Vec::new(),
            extra_central: Vec::new(),
            comment: Vec::new(),
            mtime: Some(DEFAULT_TIME),
            version_made: (0, 20),
            version_needed: 20,
            external: 0,
            internal: 0,
            crc: None,
            usize: None,
            csize: None,
            local_time: None,
            cd_offset: None,
            cdata: None,
            local_name: None,
            flags_extra: 0,
            zip64: false,
        }
    }

    /// Stores the data without compression.
    #[must_use]
    pub fn stored(mut self) -> Entry {
        self.method = 0;
        self
    }

    /// Sets the compression method number. Method 8 deflates; anything else needs [`Entry::raw`].
    #[must_use]
    pub fn method(mut self, method: u16) -> Entry {
        self.method = method;
        self
    }

    /// Sets the zlib level; it also decides the level hint bits of the flags.
    #[must_use]
    pub fn level(mut self, level: u32) -> Entry {
        self.level = level;
        self
    }

    /// Records sizes in a data descriptor.
    #[must_use]
    pub fn descriptor(mut self, d: Descriptor) -> Entry {
        self.descriptor = d;
        if d == Descriptor::Zip64Signed {
            self.zip64 = true;
            self.version_needed = 45;
        }
        self
    }

    /// Forces the UTF-8 flag on or off. The default sets it when the name is not ASCII.
    #[must_use]
    pub fn utf8_flag(mut self, on: bool) -> Entry {
        self.utf8 = Some(on);
        self
    }

    /// Extra field bytes for the local header.
    #[must_use]
    pub fn extra_local(mut self, extra: &[u8]) -> Entry {
        self.extra_local = extra.to_vec();
        self
    }

    /// Extra field bytes for the central directory record.
    #[must_use]
    pub fn extra_central(mut self, extra: &[u8]) -> Entry {
        self.extra_central = extra.to_vec();
        self
    }

    /// The same extra field bytes in both places.
    #[must_use]
    pub fn extra_both(self, extra: &[u8]) -> Entry {
        self.extra_local(extra).extra_central(extra)
    }

    /// A central directory comment.
    #[must_use]
    pub fn comment(mut self, comment: &[u8]) -> Entry {
        self.comment = comment.to_vec();
        self
    }

    /// DOS time and date as `(time, date)`; `None` writes zero for both.
    #[must_use]
    pub fn mtime(mut self, mtime: Option<(u16, u16)>) -> Entry {
        self.mtime = mtime;
        self
    }

    /// Version made by (host, version) and the Unix-style external attributes.
    #[must_use]
    pub fn host(mut self, host: u8, version: u8, internal: u16, external: u32) -> Entry {
        self.version_made = (host, version);
        self.internal = internal;
        self.external = external;
        self
    }

    /// Overrides the recorded CRC-32.
    #[must_use]
    pub fn crc(mut self, crc: u32) -> Entry {
        self.crc = Some(crc);
        self
    }

    /// Overrides the recorded uncompressed size.
    #[must_use]
    pub fn declared_usize(mut self, size: u32) -> Entry {
        self.usize = Some(size);
        self
    }

    /// Overrides the recorded compressed size, in both headers.
    #[must_use]
    pub fn declared_csize(mut self, size: u32) -> Entry {
        self.csize = Some(size);
        self
    }

    /// A different DOS time and date in the local header than the central directory has (`None`
    /// writes zeros in the local header).
    #[must_use]
    pub fn local_mtime(mut self, mtime: Option<(u16, u16)>) -> Entry {
        self.local_time = Some(mtime);
        self
    }

    /// A different offset of the local header in the central directory record.
    #[must_use]
    pub fn central_offset(mut self, offset: u32) -> Entry {
        self.cd_offset = Some(offset);
        self
    }

    /// Replaces the compressed bytes with these, whatever the data is.
    #[must_use]
    pub fn raw(mut self, compressed: &[u8]) -> Entry {
        self.cdata = Some(compressed.to_vec());
        self
    }

    /// A different name in the local header than in the central directory.
    #[must_use]
    pub fn local_name(mut self, name: &[u8]) -> Entry {
        self.local_name = Some(name.to_vec());
        self
    }

    /// Extra general-purpose flag bits, such as bit 0 for encryption.
    #[must_use]
    pub fn flags(mut self, bits: u16) -> Entry {
        self.flags_extra = bits;
        self
    }

    /// The entry name as written to the central directory.
    pub fn name_bytes(&self) -> &[u8] {
        &self.name
    }

    pub(super) fn hint_bits(&self) -> u16 {
        if self.method != 8 {
            return 0;
        }
        match self.level {
            9.. => 0x2,
            1 => 0x6,
            2 | 3 => 0x4,
            _ => 0,
        }
    }
}
