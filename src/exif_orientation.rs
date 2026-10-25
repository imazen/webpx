//! EXIF orientation lookup for [`ImageInfo`](crate::ImageInfo).
//!
//! WebP stores orientation only as the Orientation tag (0x0112) of the TIFF
//! blob in its `EXIF` chunk; the bitstream itself is always in stored order.
//! This reads that one tag with the same rules as zencodec's EXIF parser
//! (`zencodec::helpers::parse_exif_orientation`), so the native and zencodec
//! APIs agree on every input. `tests/orientation.rs` checks the two against
//! each other.

use zenpixels::Orientation;

const TAG_ORIENTATION: u16 = 0x0112;
const TIFF_SHORT: u16 = 3;
const TIFF_LONG: u16 = 4;
const TIFF_HEADER_SIZE: usize = 8;
/// zencodec rejects an IFD with more entries than this.
const MAX_IFD_ENTRIES: u16 = 1000;
const EXIF_PREFIX: &[u8] = b"Exif\0\0";

/// The orientation in a WebP file's `EXIF` chunk, or `None` when the file has
/// no `EXIF` chunk, the blob doesn't parse, or it has no valid Orientation tag.
pub(crate) fn from_webp(webp: &[u8]) -> Option<Orientation> {
    parse(find_chunk(webp, *b"EXIF")?)
}

/// The Orientation tag of a TIFF/EXIF blob (raw TIFF or `Exif\0\0`-prefixed).
pub(crate) fn parse(data: &[u8]) -> Option<Orientation> {
    let tiff = data.strip_prefix(EXIF_PREFIX).unwrap_or(data);
    if tiff.len() < TIFF_HEADER_SIZE {
        return None;
    }
    let big = match [tiff[0], tiff[1]] {
        [b'M', b'M'] => true,
        [b'I', b'I'] => false,
        _ => return None,
    };
    if rd16(tiff, 2, big)? != 42 {
        return None;
    }
    let ifd0 = rd32(tiff, 4, big)? as usize;
    let count = rd16(tiff, ifd0, big)?;
    if count > MAX_IFD_ENTRIES {
        return None;
    }
    let entries = ifd0.checked_add(2)?;
    entries.checked_add(count as usize * 12)?;
    for i in 0..count as usize {
        let e = entries + i * 12;
        if e + 12 > tiff.len() {
            break; // truncated entry table: keep what was read
        }
        // Entries zencodec can't resolve (unknown type, value out of bounds)
        // are skipped, so they can't shadow a later Orientation entry.
        let Some((tag, kind, value)) = resolve_entry(tiff, e, big) else {
            continue;
        };
        if tag == TAG_ORIENTATION {
            // The first resolvable Orientation entry decides.
            let raw = match kind {
                TIFF_SHORT => u32::from(rd16(value, 0, big)?),
                TIFF_LONG => rd32(value, 0, big)?,
                _ => return None,
            };
            return Orientation::from_exif(u8::try_from(raw).ok()?);
        }
    }
    None
}

/// Tag, type and value bytes of the 12-byte IFD entry at `e`, or `None` for an
/// unknown type, an overflowing size, or an out-of-bounds value.
fn resolve_entry(tiff: &[u8], e: usize, big: bool) -> Option<(u16, u16, &[u8])> {
    let tag = rd16(tiff, e, big)?;
    let kind = rd16(tiff, e + 2, big)?;
    let count = rd32(tiff, e + 4, big)?;
    let size = match kind {
        1 | 2 | 6 | 7 | 129 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => return None,
    };
    let len = (count as usize).checked_mul(size)?;
    let value = if len <= 4 {
        tiff.get(e + 8..e + 8 + len)?
    } else {
        let off = rd32(tiff, e + 8, big)? as usize;
        tiff.get(off..off.checked_add(len)?)?
    };
    Some((tag, kind, value))
}

/// The payload of the first top-level RIFF chunk named `fourcc`.
fn find_chunk(webp: &[u8], fourcc: [u8; 4]) -> Option<&[u8]> {
    if webp.len() < 12 || &webp[..4] != b"RIFF" || &webp[8..12] != b"WEBP" {
        return None;
    }
    let mut pos = 12usize;
    while let Some(header) = webp.get(pos..pos.checked_add(8)?) {
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let start = pos + 8;
        let end = start.checked_add(size)?;
        if header[..4] == fourcc {
            return webp.get(start..end);
        }
        // Chunk payloads are padded to an even length.
        pos = end.checked_add(size & 1)?;
    }
    None
}

fn rd16(d: &[u8], o: usize, big: bool) -> Option<u16> {
    let b = d.get(o..o.checked_add(2)?)?;
    Some(if big {
        u16::from_be_bytes([b[0], b[1]])
    } else {
        u16::from_le_bytes([b[0], b[1]])
    })
}

fn rd32(d: &[u8], o: usize, big: bool) -> Option<u32> {
    let b = d.get(o..o.checked_add(4)?)?;
    Some(if big {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// One-entry IFD0 TIFF with the Orientation tag as `kind` (SHORT/LONG).
    fn tiff(value: u32, big: bool, kind: u16) -> Vec<u8> {
        let w16 = |v: u16| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let w32 = |v: u32| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let mut t = Vec::new();
        t.extend_from_slice(if big { b"MM" } else { b"II" });
        t.extend_from_slice(&w16(42));
        t.extend_from_slice(&w32(8));
        t.extend_from_slice(&w16(1));
        t.extend_from_slice(&w16(TAG_ORIENTATION));
        t.extend_from_slice(&w16(kind));
        t.extend_from_slice(&w32(1));
        if kind == TIFF_SHORT {
            t.extend_from_slice(&w16(value as u16));
            t.extend_from_slice(&[0, 0]);
        } else {
            t.extend_from_slice(&w32(value));
        }
        t.extend_from_slice(&w32(0));
        t
    }

    #[test]
    fn reads_every_orientation_both_orders_and_types() {
        for v in 1..=8u32 {
            for big in [false, true] {
                for kind in [TIFF_SHORT, TIFF_LONG] {
                    let t = tiff(v, big, kind);
                    assert_eq!(parse(&t), Orientation::from_exif(v as u8));
                    let mut prefixed = EXIF_PREFIX.to_vec();
                    prefixed.extend_from_slice(&t);
                    assert_eq!(parse(&prefixed), Orientation::from_exif(v as u8));
                }
            }
        }
        assert_eq!(parse(&tiff(0, false, TIFF_SHORT)), None);
        assert_eq!(parse(&tiff(9, false, TIFF_SHORT)), None);
        assert_eq!(parse(&tiff(256 + 6, false, TIFF_LONG)), None);
    }

    /// Same answer as zencodec's parser on valid blobs and on many corrupted
    /// variants of them (deterministic xorshift mutations and truncations).
    #[cfg(feature = "zencodec")]
    #[test]
    fn matches_zencodec_parser() {
        let mut seeds = Vec::new();
        for v in [1u32, 3, 6, 8, 9] {
            for big in [false, true] {
                for kind in [TIFF_SHORT, TIFF_LONG, 7] {
                    seeds.push(tiff(v, big, kind));
                }
            }
        }
        // An unresolvable entry (unknown type) ahead of the Orientation entry.
        let mut two = tiff(6, false, TIFF_SHORT);
        two[8] = 2;
        two.splice(10..10, [0x00, 0x01, 0x63, 0x00, 1, 0, 0, 0, 0, 0, 0, 0]);
        seeds.push(two);
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut checked = 0;
        for seed in &seeds {
            for len in 0..=seed.len() {
                let t = &seed[..len];
                assert_eq!(
                    parse(t),
                    zencodec::helpers::parse_exif_orientation(t),
                    "{t:?}"
                );
                checked += 1;
            }
            for _ in 0..2000 {
                let mut t = seed.clone();
                for _ in 0..1 + next() % 3 {
                    let i = (next() % t.len() as u64) as usize;
                    t[i] = next() as u8;
                }
                assert_eq!(
                    parse(&t),
                    zencodec::helpers::parse_exif_orientation(&t),
                    "{t:?}"
                );
                checked += 1;
            }
        }
        assert!(checked > 60_000);
    }

    #[test]
    fn finds_exif_chunk_after_other_chunks() {
        let t = tiff(6, false, TIFF_SHORT);
        let mut w = b"RIFF\0\0\0\0WEBP".to_vec();
        w.extend_from_slice(b"VP8X");
        w.extend_from_slice(&10u32.to_le_bytes());
        w.extend_from_slice(&[0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        w.extend_from_slice(b"ICCP");
        w.extend_from_slice(&3u32.to_le_bytes());
        w.extend_from_slice(&[1, 2, 3, 0]); // odd size + pad byte
        w.extend_from_slice(b"EXIF");
        w.extend_from_slice(&(t.len() as u32).to_le_bytes());
        w.extend_from_slice(&t);
        assert_eq!(from_webp(&w), Some(Orientation::Rotate90));
        assert_eq!(from_webp(&w[..w.len() - 1]), None);
        assert_eq!(from_webp(b"RIFF\0\0\0\0WEBPVP8 "), None);
    }
}
