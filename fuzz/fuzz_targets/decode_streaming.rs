//! Fuzz target: incremental / streaming decode.
//!
//! Splits the encoded bytes into arbitrary chunks and feeds them to
//! `StreamingDecoder::append`, exercising the libwebp incremental decoder
//! state machine. Repeats with `update` (overlapping buffer style) and the
//! `with_buffer` constructor so all three input modes get coverage.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use webpx::{ColorMode, DecodeStatus, StreamingDecoder};

const MAX_PIXELS: u64 = 16 * 1024 * 1024;

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    /// Chunk sizes to split the input into. Values of 0 are skipped.
    chunks: Vec<u16>,
    color_mode_choice: u8,
    use_buffer: bool,
    data: &'a [u8],
}

fn pick_mode(b: u8) -> ColorMode {
    match b % 5 {
        0 => ColorMode::Rgba,
        1 => ColorMode::Bgra,
        2 => ColorMode::Argb,
        3 => ColorMode::Rgb,
        _ => ColorMode::Bgr,
    }
}

fuzz_target!(|input: Input<'_>| {
    // Cheap header probe to avoid throwing huge garbage at the streaming
    // decoder for no benefit (it would just reject it after allocating).
    if let Ok(info) = webpx::ImageInfo::from_webp(input.data)
        && (info.width as u64) * (info.height as u64) > MAX_PIXELS
    {
        return;
    }

    let mode = pick_mode(input.color_mode_choice);

    // ---- Append mode (decoder allocates output) ----
    if let Ok(mut dec) = StreamingDecoder::new(mode) {
        let mut offset = 0usize;
        for &n in &input.chunks {
            let n = n as usize;
            if n == 0 {
                continue;
            }
            let end = offset.saturating_add(n).min(input.data.len());
            if offset >= end {
                break;
            }
            match dec.append(&input.data[offset..end]) {
                Ok(DecodeStatus::Complete) => break,
                Ok(_) => {} // NeedMoreData / Partial / future variants
                Err(_) => break,
            }
            offset = end;
            // Probe partial-output accessor every chunk.
            let _ = dec.get_partial();
            let _ = dec.dimensions();
            let _ = dec.decoded_rows();
        }
        // Flush remainder.
        if offset < input.data.len() {
            let _ = dec.append(&input.data[offset..]);
        }
        let _ = dec.finish();
    }

    // ---- with_buffer mode (caller-owned output) ----
    if input.use_buffer
        && (mode == ColorMode::Rgba
            || mode == ColorMode::Bgra
            || mode == ColorMode::Argb
            || mode == ColorMode::Rgb
            || mode == ColorMode::Bgr)
        && let Ok(info) = webpx::ImageInfo::from_webp(input.data)
        && info.width > 0
        && info.height > 0
    {
        let bpp = match mode {
            ColorMode::Rgba | ColorMode::Bgra | ColorMode::Argb => 4,
            ColorMode::Rgb | ColorMode::Bgr => 3,
            _ => return,
        };
        let stride = (info.width as usize).saturating_mul(bpp);
        let buf_size = stride.saturating_mul(info.height as usize);
        if buf_size == 0 || buf_size > 64 * 1024 * 1024 {
            return;
        }
        let mut buf = vec![0u8; buf_size];
        if let Ok(mut dec) = StreamingDecoder::with_buffer(&mut buf, stride, mode) {
            // One-shot append of the entire buffer.
            let _ = dec.append(input.data);
            let _ = dec.finish();
        }
    }
});
