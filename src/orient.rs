//! Applying an EXIF orientation to decoded pixels.

use alloc::vec::Vec;
use zenpixels::Orientation;

/// `src` (`w`×`h` pixels of `bpp` bytes, rows `stride` bytes apart)
/// transformed by `o`, tightly packed, with its dimensions.
pub(crate) fn orient_pixels(
    src: &[u8],
    w: u32,
    h: u32,
    stride: usize,
    bpp: usize,
    o: Orientation,
) -> (Vec<u8>, u32, u32) {
    let (ow, oh) = o.output_dimensions(w, h);
    let mut out = alloc::vec![0u8; (ow as usize) * (oh as usize) * bpp];
    for sy in 0..h {
        let row = &src[sy as usize * stride..][..w as usize * bpp];
        for (sx, px) in (0..w).zip(row.chunks_exact(bpp)) {
            let (dx, dy) = o.forward_map(sx, sy, w, h);
            let d = (dy as usize * ow as usize + dx as usize) * bpp;
            out[d..d + bpp].copy_from_slice(px);
        }
    }
    (out, ow, oh)
}
