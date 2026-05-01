//! Fuzz target: arbitrary bytes through every static decode entry point.
//!
//! Exercises the libwebp decoder via every public top-level decode function:
//! decode_rgba / decode_rgb / decode_bgra / decode_bgr, the typed `decode<P>`
//! generic, and decode_yuv. Should never panic regardless of input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rgb::alt::{BGR8, BGRA8};
use rgb::{RGB8, RGBA8};

// Cap allocations: largest plausible decoded buffer we let through.
const MAX_PIXELS: u64 = 64 * 1024 * 1024; // 64 MP

fn pixels_ok(data: &[u8]) -> bool {
    match webpx::ImageInfo::from_webp(data) {
        Ok(info) => (info.width as u64) * (info.height as u64) <= MAX_PIXELS,
        Err(_) => false,
    }
}

fuzz_target!(|data: &[u8]| {
    // Header probe should always be safe.
    let _ = webpx::ImageInfo::from_webp(data);

    if !pixels_ok(data) {
        // Try the failure paths anyway — these should still not panic on
        // garbage data, but stop short of giant allocations on adversarial
        // header sizes the fuzzer may discover.
        let _ = webpx::decode_rgba(data);
        return;
    }

    let _ = webpx::decode_rgba(data);
    let _ = webpx::decode_rgb(data);
    let _ = webpx::decode_bgra(data);
    let _ = webpx::decode_bgr(data);

    // Typed-pixel generic path.
    let _ = webpx::decode::<RGBA8>(data);
    let _ = webpx::decode::<RGB8>(data);
    let _ = webpx::decode::<BGRA8>(data);
    let _ = webpx::decode::<BGR8>(data);

    // YUV planar decode.
    let _ = webpx::decode_yuv(data);
});
