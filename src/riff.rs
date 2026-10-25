//! WebP RIFF container helpers, in safe Rust.

/// The payload of the first top-level chunk named `fourcc`.
pub(crate) fn find_chunk<'a>(webp: &'a [u8], fourcc: &[u8; 4]) -> Option<&'a [u8]> {
    if webp.len() < 12 || &webp[..4] != b"RIFF" || &webp[8..12] != b"WEBP" {
        return None;
    }
    let mut pos = 12usize;
    while let Some(header) = webp.get(pos..pos.checked_add(8)?) {
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let start = pos + 8;
        let end = start.checked_add(size)?;
        if &header[..4] == fourcc {
            return webp.get(start..end);
        }
        pos = end.checked_add(size & 1)?;
    }
    None
}
