//! The `package` target: open, read every part, write after replacing one part.

use super::{fail, is_refusal, split};
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::package::Package;
use crate::xml::Bytes;

/// The largest part a replacement is made from. A bigger part is replaced by a small made-up one,
/// so a run does not spend its time deflating what a bomb inflated.
const MAX_REPLACED: usize = 1 << 20;

/// What reading a part gave: its bytes, or the code it was refused with.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Read {
    Bytes(Bytes),
    Refused(ErrorCode),
}

/// Runs the target on one input, and aborts when a law breaks.
pub(super) fn run(data: &[u8]) {
    if let Err(why) = check(data, &Limits::default()) {
        fail(&why);
    }
}

/// Checks the laws of `Package` on one input under `limits`.
///
/// Whatever opens must write back its own bytes, by the shortcut and by the rebuild that edits
/// use. Every part must read, or be refused, the same way twice. Then one part, chosen by the
/// first four bytes of the input, is replaced and the package written: the output must open, hold
/// the same parts in the same order, give the replacement back, and leave every other entry as it
/// was, byte for byte.
pub(super) fn check(data: &[u8], limits: &Limits) -> Result<(), String> {
    let Some((head, doc)) = split(data) else {
        return Ok(());
    };
    let pkg = match Package::open(doc.to_vec(), limits) {
        Ok(pkg) => pkg,
        Err(e) if is_refusal(&e) => return Ok(()),
        Err(e) => return Err(not_a_refusal("open", &e)),
    };
    writes_back_its_input(&pkg, doc)?;
    let names: Vec<String> = pkg.part_names().map(str::to_owned).collect();
    let before = read_every_part(&pkg, &names, limits)?;
    replace_one_part(&pkg, &names, &before, head, limits)
}

fn not_a_refusal(what: &str, e: &StetError) -> String {
    format!(
        "{what} failed with {:?}, which is not how hostile input is refused: {e}",
        e.code()
    )
}

/// The first byte at which two buffers differ, or the length of the shorter one.
fn first_difference(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()))
}

fn same_bytes(what: &str, out: &[u8], doc: &[u8]) -> Result<(), String> {
    if out == doc {
        return Ok(());
    }
    Err(format!(
        "{what} writes {} bytes where the input had {}, and they first differ at byte {}",
        out.len(),
        doc.len(),
        first_difference(out, doc)
    ))
}

/// L1 for the container: an unmodified package writes its input, by the shortcut and by the
/// rebuild that a package with a replaced part goes through.
fn writes_back_its_input(pkg: &Package, doc: &[u8]) -> Result<(), String> {
    if pkg.is_modified() {
        return Err("a package that was only opened says it was modified".into());
    }
    let shortcut = pkg
        .write()
        .map_err(|e| format!("write of an unmodified package failed: {e}"))?;
    same_bytes("write of an unmodified package", &shortcut, doc)?;
    let rebuilt = pkg
        .write_rebuilt()
        .map_err(|e| format!("the rebuild of an unmodified package failed: {e}"))?;
    same_bytes("the rebuild of an unmodified package", &rebuilt, doc)
}

/// Reads one part. A refusal must be one of the ways hostile input is refused.
fn read(pkg: &Package, name: &str, limits: &Limits) -> Result<Read, String> {
    match pkg.part(name) {
        Ok(Some(bytes)) => {
            if bytes.len() as u64 > limits.max_entry_bytes {
                return Err(format!(
                    "part {name:?} read as {} bytes, over the limit of {}",
                    bytes.len(),
                    limits.max_entry_bytes
                ));
            }
            Ok(Read::Bytes(bytes))
        }
        Ok(None) => Err(format!(
            "{name:?} is listed as a part, but part() finds none"
        )),
        Err(e) if is_refusal(&e) => Ok(Read::Refused(e.code())),
        Err(e) => Err(not_a_refusal(&format!("reading {name:?}"), &e)),
    }
}

/// Reads every part twice, which must give the same result.
fn read_every_part(pkg: &Package, names: &[String], limits: &Limits) -> Result<Vec<Read>, String> {
    let mut all = Vec::with_capacity(names.len());
    for name in names {
        if !pkg.contains(name) {
            return Err(format!(
                "{name:?} is listed as a part, but contains() denies it"
            ));
        }
        let first = read(pkg, name, limits)?;
        if first != read(pkg, name, limits)? {
            return Err(format!("reading {name:?} twice gave different results"));
        }
        all.push(first);
    }
    Ok(all)
}

/// What to put in place of a part. Which of four ways is the low two bits of `mode`: its own bytes,
/// its bytes with one flipped, its bytes cut short, or a made-up part of `at % 256 * 16` bytes of
/// `fill`. A part that did not read, or is large, gets the made-up one.
pub(super) fn replacement(mode: u8, at: usize, fill: u8, original: Option<&[u8]>) -> Vec<u8> {
    match (mode % 4, original) {
        (0, Some(own)) => own.to_vec(),
        (1, Some(own)) if !own.is_empty() => {
            let mut flipped = own.to_vec();
            if let Some(byte) = flipped.get_mut(at % own.len()) {
                *byte ^= 0xFF;
            }
            flipped
        }
        (2, Some(own)) => own.get(..at % (own.len() + 1)).unwrap_or(own).to_vec(),
        _ => vec![fill; (at % 256) * 16],
    }
}

/// Replaces one part and writes, and checks what came out.
fn replace_one_part(
    pkg: &Package,
    names: &[String],
    before: &[Read],
    head: [u8; 4],
    limits: &Limits,
) -> Result<(), String> {
    let [pick, mode, hi, lo] = head;
    let Some(index) = usize::from(pick).checked_rem(names.len()) else {
        return Ok(());
    };
    let (Some(name), Some(old)) = (names.get(index), before.get(index)) else {
        return Ok(());
    };
    let own = match old {
        Read::Bytes(bytes) if bytes.len() <= MAX_REPLACED => Some(bytes.as_slice()),
        _ => None,
    };
    let at = usize::from(hi) << 8 | usize::from(lo);
    let new = replacement(mode, at, lo, own);
    let mut edited = pkg.clone();
    match edited.replace_part(name, new.clone()) {
        Ok(()) => {}
        Err(e) if matches!(e.code(), ErrorCode::Unsupported | ErrorCode::LimitExceeded) => {
            return Ok(());
        }
        Err(e) => return Err(not_a_refusal(&format!("replacing {name:?}"), &e)),
    }
    if !edited.is_modified() {
        return Err(format!(
            "a package with {name:?} replaced says it was not modified"
        ));
    }
    let out = match edited.write() {
        Ok(out) => out,
        Err(e) if matches!(e.code(), ErrorCode::Unsupported | ErrorCode::LimitExceeded) => {
            return Ok(());
        }
        Err(e) => {
            return Err(not_a_refusal(
                &format!("writing after replacing {name:?}"),
                &e,
            ));
        }
    };
    if edited.write().ok().as_ref() != Some(&out) {
        return Err("write is not deterministic".into());
    }
    let reopened = Package::open(out, limits).map_err(|e| {
        format!("the output of a write after replacing {name:?} does not open: {e}")
    })?;
    if !reopened.part_names().eq(names.iter().map(String::as_str)) {
        return Err(format!(
            "the parts or their order changed when {name:?} was replaced"
        ));
    }
    for (i, other) in names.iter().enumerate() {
        let now = read(&reopened, other, limits)?;
        if i == index {
            if now != Read::Bytes(Bytes::from(new.as_slice())) {
                return Err(format!("{name:?} does not read back as its replacement"));
            }
        } else if before.get(i) != Some(&now) {
            return Err(format!(
                "part {other:?} reads differently, although only {name:?} was replaced"
            ));
        } else if reopened.raw_region(other) != pkg.raw_region(other) {
            return Err(format!(
                "the bytes of entry {other:?} changed, although only {name:?} was replaced"
            ));
        }
    }
    Ok(())
}
