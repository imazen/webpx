//! Fuzz target: caller-provided output buffers.
//!
//! Exercises the `decode_*_into` family with strides and buffer sizes
//! derived from the fuzzer. Probes for OOB writes, stride miscalculations,
//! and integer overflow when computing `stride * height`.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use rgb::RGBA8;

const MAX_PIXELS: u64 = 16 * 1024 * 1024; // 16 MP — tighter for buffer paths

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    /// Stride padding, in pixels, beyond the natural width.
    stride_pad: u8,
    /// Whether to over-allocate the output buffer beyond what's needed.
    overalloc: bool,
    /// Whether to deliberately under-allocate (should produce a clean error).
    underalloc: bool,
    /// Encoded bytes.
    data: &'a [u8],
}

fuzz_target!(|input: Input<'_>| {
    let info = match webpx::ImageInfo::from_webp(input.data) {
        Ok(i) => i,
        Err(_) => return,
    };

    let pixels = (info.width as u64) * (info.height as u64);
    if pixels == 0 || pixels > MAX_PIXELS {
        return;
    }

    let stride_pad = input.stride_pad as u32;
    let stride_pixels = info.width.saturating_add(stride_pad);

    // ---- Byte-stride form (RGBA): stride is in bytes, must be >= w*4 ----
    {
        let stride_bytes = stride_pixels.saturating_mul(4);
        let needed = (stride_bytes as usize).saturating_mul(info.height as usize);

        let buf_len = if input.underalloc {
            needed.saturating_sub(1)
        } else if input.overalloc {
            needed.saturating_add(1024)
        } else {
            needed
        };

        // Cap buffer allocation defensively.
        if buf_len <= 256 * 1024 * 1024 {
            let mut buf = vec![0u8; buf_len];
            let _ = webpx::decode_rgba_into(input.data, &mut buf, stride_bytes);
            let _ = webpx::decode_rgb_into(input.data, &mut buf, stride_bytes);
            let _ = webpx::decode_bgra_into(input.data, &mut buf, stride_bytes);
            let _ = webpx::decode_bgr_into(input.data, &mut buf, stride_bytes);
        }
    }

    // ---- Pixel-stride form (typed): stride is in pixels of P ----
    {
        let needed_pix = (stride_pixels as usize).saturating_mul(info.height as usize);
        let buf_len_pix = if input.underalloc {
            needed_pix.saturating_sub(1)
        } else if input.overalloc {
            needed_pix.saturating_add(64)
        } else {
            needed_pix
        };

        if buf_len_pix <= 64 * 1024 * 1024 {
            let mut buf: Vec<RGBA8> = vec![RGBA8::default(); buf_len_pix];
            let _ = webpx::decode_into::<RGBA8>(input.data, &mut buf, stride_pixels);
        }
    }

    // ---- Append into existing Vec ----
    {
        let mut sink: Vec<RGBA8> = Vec::new();
        let _ = webpx::decode_append::<RGBA8>(input.data, &mut sink);
    }
});
