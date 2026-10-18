//! Checked little-endian reads and writes. Every offset comes from input, so none of these
//! can index out of range or overflow.

pub(crate) fn slice(b: &[u8], start: usize, end: usize) -> Option<&[u8]> {
    b.get(start..end)
}

pub(crate) fn slice_len(b: &[u8], start: usize, len: usize) -> Option<&[u8]> {
    b.get(start..start.checked_add(len)?)
}

pub(crate) fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    let s = slice_len(b, o, 2)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

pub(crate) fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    let s = slice_len(b, o, 4)?;
    let a: [u8; 4] = s.try_into().ok()?;
    Some(u32::from_le_bytes(a))
}

pub(crate) fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    let s = slice_len(b, o, 8)?;
    let a: [u8; 8] = s.try_into().ok()?;
    Some(u64::from_le_bytes(a))
}

pub(crate) fn put(b: &mut [u8], o: usize, bytes: &[u8]) -> Option<()> {
    let end = o.checked_add(bytes.len())?;
    b.get_mut(o..end)?.copy_from_slice(bytes);
    Some(())
}

pub(crate) fn put_u16(b: &mut [u8], o: usize, v: u16) -> Option<()> {
    put(b, o, &v.to_le_bytes())
}

pub(crate) fn put_u32(b: &mut [u8], o: usize, v: u32) -> Option<()> {
    put(b, o, &v.to_le_bytes())
}

pub(crate) fn put_u64(b: &mut [u8], o: usize, v: u64) -> Option<()> {
    put(b, o, &v.to_le_bytes())
}

pub(crate) fn push_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn push_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(crate) fn push_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}
