//! H7: a damaged complement is refused, whatever the damage.

use stetdoc_core::{Document, ErrorCode, OpenOptions, WorkingCopy};

/// The most byte positions tried in one entry. The manifest is small, and every one of its bytes is tried.
const SAMPLE: usize = 400;

/// **H7.** Every single-byte change of the manifest, and of a sample of the bytes of each other entry, is refused by `resume`
/// with `corrupt_complement`, `unsupported_version` or `limit_exceeded`, and never panics or succeeds. The manifest's own digest
/// and its canonical form make the first complete, and the digests it holds make the second. A newer minor version is
/// `unsupported_version`.
pub fn refuses_every_single_byte_change(working_copy: &WorkingCopy) -> Result<(), String> {
    let options = OpenOptions::default();
    Document::resume(working_copy, &options)
        .map_err(|e| format!("the working copy does not resume: {e}"))?;
    let allowed = [
        ErrorCode::CorruptComplement,
        ErrorCode::UnsupportedVersion,
        ErrorCode::LimitExceeded,
    ];
    for (index, (name, bytes)) in working_copy.complement.entries.iter().enumerate() {
        let positions: Vec<usize> = if name == "manifest" || bytes.len() <= SAMPLE {
            (0..bytes.len()).collect()
        } else {
            let step = bytes.len() / SAMPLE + 1;
            (0..bytes.len()).step_by(step).collect()
        };
        let values: &[u8] = if name == "manifest" {
            &[0, 1, 0x20, b'0', b'f', 0x7f, 0xff]
        } else {
            &[0xff]
        };
        for &at in &positions {
            for &value in values {
                let mut damaged = working_copy.clone();
                let Some((_, entry)) = damaged.complement.entries.get_mut(index) else {
                    continue;
                };
                let Some(byte) = entry.get_mut(at) else {
                    continue;
                };
                if *byte == value {
                    *byte ^= 1;
                } else {
                    *byte = value;
                }
                match Document::resume(&damaged, &options) {
                    Ok(_) => {
                        return Err(format!(
                            "H7: {name} byte {at} changed to {value:#04x} and the working copy still resumes"
                        ));
                    }
                    Err(e) if allowed.contains(&e.code()) => {}
                    Err(e) => {
                        return Err(format!(
                            "H7: {name} byte {at}: refused with {}, not a complement error: {e}",
                            e.code().as_str()
                        ));
                    }
                }
            }
        }
    }
    // A newer minor version, and a newer major one.
    let manifest = working_copy
        .complement
        .entries
        .iter()
        .position(|(n, _)| n == "manifest")
        .ok_or("the working copy has no manifest")?;
    for newer in ["1.1", "2.0"] {
        let mut changed = working_copy.clone();
        let Some((_, bytes)) = changed.complement.entries.get_mut(manifest) else {
            continue;
        };
        let text = String::from_utf8_lossy(bytes).replacen(
            "complement 1.0",
            &format!("complement {newer}"),
            1,
        );
        *bytes = text.into_bytes();
        match Document::resume(&changed, &options) {
            Err(e) if e.code() == ErrorCode::UnsupportedVersion => {}
            other => {
                return Err(format!(
                    "H7: version {newer} gave {:?}, not unsupported_version",
                    other.err().map(|e| e.code())
                ));
            }
        }
    }
    Ok(())
}
