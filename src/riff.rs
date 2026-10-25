//! WebP RIFF container helpers, in safe Rust.
//!
//! libwebp rewrites a one-frame animation as a still image in two places:
//! `WebPAnimEncoderAssemble` (`OptimizeSingleFrame`) and `WebPMuxAssemble`
//! (`MuxCleanup`, for a single full-canvas ANMF frame). The still has the same
//! pixels but no ANIM/ANMF chunks, so the loop count, the frame duration and
//! the background colour are lost, and the file no longer reports itself as
//! animated. [`still_to_animation`] puts them back.

use alloc::vec::Vec;

/// One top-level RIFF chunk.
pub(crate) struct Chunk<'a> {
    pub(crate) fourcc: [u8; 4],
    pub(crate) payload: &'a [u8],
}

/// The top-level chunks after the `RIFF....WEBP` header, or `None` if the
/// container is malformed or truncated.
pub(crate) fn chunks(webp: &[u8]) -> Option<Vec<Chunk<'_>>> {
    if webp.len() < 12 || &webp[..4] != b"RIFF" || &webp[8..12] != b"WEBP" {
        return None;
    }
    let mut out = Vec::new();
    let mut pos = 12usize;
    while pos < webp.len() {
        let header = webp.get(pos..pos.checked_add(8)?)?;
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        let start = pos + 8;
        let end = start.checked_add(size)?;
        out.push(Chunk {
            fourcc: [header[0], header[1], header[2], header[3]],
            payload: webp.get(start..end)?,
        });
        // Payloads are padded to an even length; the last pad may be absent.
        pos = end.checked_add(size & 1)?.min(webp.len());
    }
    Some(out)
}

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

/// The parameters of a one-frame animation that a still image drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OneFrame {
    /// ANIM background colour, as stored (bytes B, G, R, A read little-endian).
    pub(crate) bgcolor: u32,
    pub(crate) loop_count: u16,
    /// ANMF duration in milliseconds (24 bits).
    pub(crate) duration: u32,
    /// ANMF blending/disposal byte.
    pub(crate) flags: u8,
}

/// If `webp` is an animation with exactly one ANMF frame covering the whole
/// canvas (the shape libwebp collapses), its parameters.
pub(crate) fn one_frame_animation(webp: &[u8]) -> Option<OneFrame> {
    let chunks = chunks(webp)?;
    let vp8x = chunks.iter().find(|c| &c.fourcc == b"VP8X")?.payload;
    if vp8x.len() < 10 || vp8x[0] & ANIMATION == 0 {
        return None;
    }
    let canvas = (u24(&vp8x[4..7]) + 1, u24(&vp8x[7..10]) + 1);
    let anim = chunks.iter().find(|c| &c.fourcc == b"ANIM")?.payload;
    let mut frames = chunks.iter().filter(|c| &c.fourcc == b"ANMF");
    let anmf = frames.next()?.payload;
    if frames.next().is_some() || anim.len() < 6 || anmf.len() < 16 {
        return None;
    }
    if (u24(&anmf[6..9]) + 1, u24(&anmf[9..12]) + 1) != canvas {
        return None;
    }
    Some(OneFrame {
        bgcolor: u32::from_le_bytes([anim[0], anim[1], anim[2], anim[3]]),
        loop_count: u16::from_le_bytes([anim[4], anim[5]]),
        duration: u24(&anmf[12..15]),
        flags: anmf[15],
    })
}

/// Whether `webp`'s VP8X header has the animation flag.
pub(crate) fn is_animated(webp: &[u8]) -> bool {
    find_chunk(webp, b"VP8X").is_some_and(|v| !v.is_empty() && v[0] & ANIMATION != 0)
}

const ICC: u8 = 0x20;
const ALPHA: u8 = 0x10;
const EXIF: u8 = 0x08;
const XMP: u8 = 0x04;
const ANIMATION: u8 = 0x02;

/// Rewrap the still image `still` as a one-frame animation with `p`'s
/// parameters, keeping its ICCP/EXIF/XMP and unknown chunks. `None` when
/// `still` is not a parseable still image or `p.duration` doesn't fit.
pub(crate) fn still_to_animation(still: &[u8], p: &OneFrame) -> Option<Vec<u8>> {
    if p.duration >= 1 << 24 {
        return None;
    }
    let chunks = chunks(still)?;
    let (mut w, mut h, mut alpha) = (0u32, 0u32, false);
    let mut image: Vec<&Chunk<'_>> = Vec::new();
    let mut extra: Vec<&Chunk<'_>> = Vec::new();
    let (mut iccp, mut exif, mut xmp) = (None, None, None);
    for c in &chunks {
        match &c.fourcc {
            b"VP8X" => {
                let v = c.payload;
                if v.len() < 10 || v[0] & ANIMATION != 0 {
                    return None;
                }
                (w, h) = (u24(&v[4..7]) + 1, u24(&v[7..10]) + 1);
                alpha |= v[0] & ALPHA != 0;
            }
            b"ALPH" => {
                alpha = true;
                image.push(c);
            }
            b"VP8 " => {
                let v = c.payload;
                // Frame tag (3 bytes), start code, then 14-bit width/height.
                if w == 0 {
                    if v.len() < 10 || v[3..6] != [0x9d, 0x01, 0x2a] {
                        return None;
                    }
                    w = u32::from(u16::from_le_bytes([v[6], v[7]]) & 0x3fff);
                    h = u32::from(u16::from_le_bytes([v[8], v[9]]) & 0x3fff);
                }
                image.push(c);
            }
            b"VP8L" => {
                let v = c.payload;
                if v.len() < 5 || v[0] != 0x2f {
                    return None;
                }
                let bits = u32::from_le_bytes([v[1], v[2], v[3], v[4]]);
                if w == 0 {
                    w = (bits & 0x3fff) + 1;
                    h = ((bits >> 14) & 0x3fff) + 1;
                }
                alpha |= (bits >> 28) & 1 != 0;
                image.push(c);
            }
            b"ICCP" => iccp = Some(c.payload),
            b"EXIF" => exif = Some(c.payload),
            b"XMP " => xmp = Some(c.payload),
            b"ANIM" | b"ANMF" => return None,
            _ => extra.push(c),
        }
    }
    if image.is_empty() || w == 0 || h == 0 {
        return None;
    }

    let mut flags = ANIMATION;
    if iccp.is_some() {
        flags |= ICC;
    }
    if alpha {
        flags |= ALPHA;
    }
    if exif.is_some() {
        flags |= EXIF;
    }
    if xmp.is_some() {
        flags |= XMP;
    }
    let mut vp8x = alloc::vec![flags, 0, 0, 0];
    put_u24(&mut vp8x, w - 1);
    put_u24(&mut vp8x, h - 1);
    let mut anim = p.bgcolor.to_le_bytes().to_vec();
    anim.extend_from_slice(&p.loop_count.to_le_bytes());
    let mut anmf = Vec::new();
    put_u24(&mut anmf, 0); // x offset / 2
    put_u24(&mut anmf, 0); // y offset / 2
    put_u24(&mut anmf, w - 1);
    put_u24(&mut anmf, h - 1);
    put_u24(&mut anmf, p.duration);
    anmf.push(p.flags);
    for c in &image {
        put_chunk(&mut anmf, &c.fourcc, c.payload);
    }

    let mut body = Vec::new();
    put_chunk(&mut body, b"VP8X", &vp8x);
    if let Some(icc) = iccp {
        put_chunk(&mut body, b"ICCP", icc);
    }
    put_chunk(&mut body, b"ANIM", &anim);
    put_chunk(&mut body, b"ANMF", &anmf);
    if let Some(e) = exif {
        put_chunk(&mut body, b"EXIF", e);
    }
    if let Some(x) = xmp {
        put_chunk(&mut body, b"XMP ", x);
    }
    for c in &extra {
        put_chunk(&mut body, &c.fourcc, c.payload);
    }
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&u32::try_from(body.len() + 4).ok()?.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&body);
    Some(out)
}

fn u24(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16
}

fn put_u24(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes()[..3]);
}

fn put_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
}
