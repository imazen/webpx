//! Fuzz target: animated WebP demux + per-frame decode.
//!
//! Exercises the WebPAnimDecoder path: parses ANMF/ANIM chunks, applies
//! the dispose/blend logic per frame, and decodes each frame. Also tries
//! `decode_all` and `reset` so the iterator-state-machine paths get hit.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use webpx::{AnimationDecoder, ColorMode};

const MAX_PIXELS: u64 = 4 * 1024 * 1024; // 4 MP per canvas
const MAX_FRAMES: u32 = 256;

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    color_mode_choice: u8,
    use_threads: bool,
    use_reset: bool,
    use_decode_all: bool,
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
    let mode = pick_mode(input.color_mode_choice);

    // Frame-by-frame.
    if let Ok(mut dec) = AnimationDecoder::with_options(input.data, mode, input.use_threads) {
        let info = dec.info().clone();
        if (info.width as u64) * (info.height as u64) > MAX_PIXELS {
            return;
        }
        if info.frame_count > MAX_FRAMES {
            return;
        }
        let mut count = 0u32;
        while let Ok(Some(_frame)) = dec.next_frame() {
            count = count.saturating_add(1);
            if count >= MAX_FRAMES {
                break;
            }
        }
        if input.use_reset {
            dec.reset();
            // Re-iterate after reset — must not panic.
            for _ in 0..MAX_FRAMES.min(info.frame_count) {
                match dec.next_frame() {
                    Ok(Some(_)) => {}
                    _ => break,
                }
            }
        }
    }

    // Batch.
    if input.use_decode_all
        && let Ok(mut dec) = AnimationDecoder::with_options(input.data, mode, false)
    {
        let info = dec.info().clone();
        if (info.width as u64) * (info.height as u64) <= MAX_PIXELS
            && info.frame_count <= MAX_FRAMES
        {
            let _ = dec.decode_all();
        }
    }
});
