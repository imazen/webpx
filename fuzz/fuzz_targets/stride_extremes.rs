//! Fuzz target: stride parameters at `i32::MAX` boundaries.
//!
//! The 0.2.1 release closed a class of OOB issues where caller-supplied
//! `u32` / `usize` strides cast to `i32` for libwebp without an upper
//! bound check — strides `>= 2^31` wrapped to negative `i32`, and
//! libwebp's row-pointer arithmetic walked backwards through process
//! memory. This target exercises every encoder / decoder / streaming
//! entry point that takes a stride parameter, with stride values
//! drawn from the i32::MAX neighbourhood plus a fuzzer-controlled
//! arbitrary value.

#![no_main]

extern crate alloc;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use webpx::{ColorMode, Encoder, StreamingDecoder, Unstoppable};

const MAX_DIM: u32 = 256; // small for tractability
const MAX_PIXELS: u64 = 1024 * 1024;

#[derive(Arbitrary, Debug, Clone, Copy)]
enum StrideChoice {
    /// Tight (== width × bpp).
    Tight,
    /// Tight + 1 padding byte.
    TightPlus1,
    /// `i32::MAX as u32`.
    I32Max,
    /// `i32::MAX as u32 + 1` (the smallest stride that wraps negative).
    I32MaxPlus1,
    /// `u32::MAX`.
    U32Max,
    /// Arbitrary.
    Arbitrary(u32),
}

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    width: u8,
    height: u8,
    rgba_stride: StrideChoice,
    argb_stride: StrideChoice,
    streaming_stride: StrideChoice,
    decode_into_stride: StrideChoice,
    pixels: &'a [u8],
    encoded: &'a [u8],
}

fn resolve(choice: StrideChoice, tight: u32) -> u32 {
    match choice {
        StrideChoice::Tight => tight,
        StrideChoice::TightPlus1 => tight.saturating_add(1),
        StrideChoice::I32Max => i32::MAX as u32,
        StrideChoice::I32MaxPlus1 => (i32::MAX as u32).saturating_add(1),
        StrideChoice::U32Max => u32::MAX,
        StrideChoice::Arbitrary(v) => v,
    }
}

fuzz_target!(|input: Input<'_>| {
    let w = ((input.width as u32) % MAX_DIM).max(1);
    let h = ((input.height as u32) % MAX_DIM).max(1);
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return;
    }
    let needed_rgba = (w as usize).saturating_mul(h as usize).saturating_mul(4);
    if input.pixels.len() < needed_rgba {
        return;
    }

    // ---- RGBA encoder stride ----
    {
        let s = resolve(input.rgba_stride, w.saturating_mul(4));
        let _ = Encoder::new_rgba_stride(&input.pixels[..needed_rgba], w, h, s)
            .quality(50.0)
            .encode(Unstoppable);
    }

    // ---- ARGB encoder stride (pixel-stride, not byte-stride) ----
    {
        let s = resolve(input.argb_stride, w);
        // Build an aligned u32 slice — &[u8] is not necessarily 4-byte
        // aligned, so reinterpreting via from_raw_parts is UB.
        let argb_words = input.pixels.len() / 4;
        let mut argb: alloc::vec::Vec<u32> = alloc::vec::Vec::with_capacity(argb_words);
        for chunk in input.pixels.chunks_exact(4).take(argb_words) {
            argb.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        if argb.len() >= (s as usize).saturating_mul(h as usize) {
            let _ = Encoder::new_argb_stride(&argb, w, h, s)
                .quality(50.0)
                .encode(Unstoppable);
        }
    }

    // ---- StreamingDecoder::with_buffer stride ----
    {
        let s = resolve(input.streaming_stride, w.saturating_mul(4)) as usize;
        // Cap buffer size defensively.
        let buf_size = (w as usize).saturating_mul(h as usize).saturating_mul(4);
        if buf_size <= 16 * 1024 * 1024 {
            let mut buf = vec![0u8; buf_size];
            let _ = StreamingDecoder::with_buffer(&mut buf, s, ColorMode::Rgba);
        }
    }

    // ---- decode_*_into stride ----
    {
        let s = resolve(input.decode_into_stride, w.saturating_mul(4));
        let buf_size = (s as usize).saturating_mul(h as usize);
        if buf_size <= 16 * 1024 * 1024 {
            let mut buf = vec![0u8; buf_size];
            let _ = webpx::decode_rgba_into(input.encoded, &mut buf, s);
            let _ = webpx::decode_bgra_into(input.encoded, &mut buf, s);
        }
    }
});
