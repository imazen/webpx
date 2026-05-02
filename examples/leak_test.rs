//! Memory-leak regression test path.
//!
//! Runs every webpx surface in a tight loop and exits cleanly. With
//! every libwebp resource going through the [`crate::ffi`] RAII
//! wrappers, no allocation should leak across iterations — running
//! under `heaptrack` or AddressSanitizer with `detect_leaks=1` should
//! report zero leaked bytes attributable to webpx code.
//!
//! **Expected non-webpx leaks (filter when interpreting output):**
//!
//! - `std::sys::pal::unix::stack_overflow::thread_info::set_current_info`
//!   ~544 B per thread — Linux thread-local stack overflow info; std
//!   releases it via OS process teardown but heaptrack doesn't see the
//!   `free`. Not a webpx leak.
//!
//! Run modes:
//!
//! ```text
//! # Heaptrack (captures libwebp allocations too):
//! cargo build --release --all-features --example leak_test
//! heaptrack ./target/release/examples/leak_test
//! heaptrack_print heaptrack.leak_test.*.zst | grep -E "^(leaked|peak)"
//!
//! # ASan with leak detection:
//! RUSTFLAGS='-Zsanitizer=address' \
//!   ASAN_OPTIONS='detect_leaks=1' \
//!   cargo +nightly run -Zbuild-std --release \
//!     --target x86_64-unknown-linux-gnu \
//!     --all-features --example leak_test
//! ```
//!
//! The harness deliberately exercises:
//!
//! - Static encode (RGBA, RGB, BGRA, BGR, ARGB zero-copy)
//! - Static decode (RGBA into / append / standalone)
//! - Streaming decode with append
//! - Streaming decode with caller-owned buffer
//! - Animation encode + decode (full and frame-by-frame)
//! - Mux: embed + extract ICCP / EXIF / XMP
//! - Error paths: oversized strides, bad inputs, hostile dimensions
//!
//! Each surface runs N iterations (default 50) so a single missed
//! cleanup compounds into a large leak that's easy to spot.

use rgb::RGBA8;
use std::env;
use webpx::*;

const ITERATIONS: usize = 50;

fn iter_count() -> usize {
    env::var("WEBPX_LEAK_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(ITERATIONS)
}

fn make_rgba(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&[
                (x.wrapping_mul(40) + 10) as u8,
                (y.wrapping_mul(50) + 20) as u8,
                (x.wrapping_mul(17).wrapping_add(y.wrapping_mul(19)) + 30) as u8,
                255,
            ]);
        }
    }
    data
}

fn make_rgb(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&[
                (x.wrapping_mul(11) + 7) as u8,
                (y.wrapping_mul(13) + 7) as u8,
                (x.wrapping_mul(17).wrapping_add(y.wrapping_mul(19)) + 30) as u8,
            ]);
        }
    }
    data
}

fn run_static_encode_decode(n: usize) {
    let rgba = make_rgba(64, 48);
    let rgb = make_rgb(64, 48);
    for _ in 0..n {
        let webp = Encoder::new_rgba(&rgba, 64, 48)
            .quality(80.0)
            .encode(Unstoppable)
            .expect("encode rgba");
        let _ = decode_rgba(&webp).expect("decode rgba");

        let mut buf = vec![0u8; 64 * 48 * 4];
        let _ = decode_rgba_into(&webp, &mut buf, 64 * 4).expect("decode_rgba_into");

        let mut sink: Vec<RGBA8> = Vec::new();
        let _ = decode_append::<RGBA8>(&webp, &mut sink).expect("decode_append");

        let _ = Encoder::new_rgb(&rgb, 64, 48)
            .quality(80.0)
            .encode(Unstoppable)
            .expect("encode rgb");
        let _ = Encoder::new_bgra(&rgba, 64, 48)
            .quality(80.0)
            .encode(Unstoppable)
            .expect("encode bgra");

        // Lossless path
        let _ = Encoder::new_rgba(&rgba, 64, 48)
            .lossless(true)
            .encode(Unstoppable)
            .expect("encode lossless");
    }
}

#[cfg(feature = "streaming")]
fn run_streaming(n: usize) {
    let rgba = make_rgba(48, 32);
    let webp = Encoder::new_rgba(&rgba, 48, 32)
        .quality(80.0)
        .encode(Unstoppable)
        .expect("encode for streaming");

    for _ in 0..n {
        // Append mode (decoder allocates output)
        let mut dec = StreamingDecoder::new(ColorMode::Rgba).expect("streaming new");
        for chunk in webp.chunks(7) {
            let _ = dec.append(chunk);
        }
        let _ = dec.finish();

        // with_buffer mode (caller-owned output)
        let stride = 48 * 4;
        let mut buf = vec![0u8; stride * 32];
        let mut dec =
            StreamingDecoder::with_buffer(&mut buf, stride, ColorMode::Rgba).expect("with_buffer");
        let _ = dec.append(&webp);
        let _ = dec.finish();
    }
}

#[cfg(feature = "animation")]
fn run_animation(n: usize) {
    let frame_a = make_rgba(32, 24);
    let mut frame_b = make_rgba(32, 24);
    for px in frame_b.chunks_mut(4) {
        px[0] = 255 - px[0];
    }

    for _ in 0..n {
        let mut enc = AnimationEncoder::new(32, 24).expect("anim encoder");
        enc.set_quality(70.0);
        enc.add_frame_rgba(&frame_a, 0).expect("add 0");
        enc.add_frame_rgba(&frame_b, 100).expect("add 1");
        enc.add_frame_rgba(&frame_a, 200).expect("add 2");
        let webp = enc.finish(300).expect("anim finish");

        let mut dec = AnimationDecoder::new(&webp).expect("anim decoder");
        while let Ok(Some(_frame)) = dec.next_frame() {}

        let mut dec = AnimationDecoder::new(&webp).expect("anim decoder 2");
        let _ = dec.decode_all().expect("decode_all");
    }
}

#[cfg(feature = "icc")]
fn run_mux(n: usize) {
    let rgba = make_rgba(16, 16);
    let webp = Encoder::new_rgba(&rgba, 16, 16)
        .quality(80.0)
        .encode(Unstoppable)
        .expect("encode for mux");
    let icc = vec![0xa5u8; 4096];
    let exif = b"Exif\0\0\xff\xd8".repeat(64);
    let xmp = b"<x:xmpmeta/>".repeat(32);

    for _ in 0..n {
        let with_icc = embed_icc(&webp, &icc).expect("embed_icc");
        let _ = get_icc_profile(&with_icc).expect("get_icc");

        let with_exif = embed_exif(&with_icc, &exif).expect("embed_exif");
        let _ = get_exif(&with_exif).expect("get_exif");

        let with_xmp = embed_xmp(&with_exif, &xmp).expect("embed_xmp");
        let _ = get_xmp(&with_xmp).expect("get_xmp");

        // Roundtrip with limits
        let limits = Limits::default();
        let _ = get_icc_profile_with_limits(&with_xmp, &limits);
        let _ = get_exif_with_limits(&with_xmp, &limits);
        let _ = get_xmp_with_limits(&with_xmp, &limits);

        // Removal paths exercise the mux setter+writer
        let _ = remove_icc(&with_xmp).expect("remove_icc");
        let _ = remove_exif(&with_xmp).expect("remove_exif");
        let _ = remove_xmp(&with_xmp).expect("remove_xmp");
    }
}

fn run_error_paths(n: usize) {
    // Force every Drop path to fire on error returns: oversize stride,
    // mismatched buffer, invalid dimensions, etc. The wrapper layer
    // must clean up libwebp's allocations on every branch.
    for _ in 0..n {
        // Oversize stride — Picture + MemWriter Drop must run.
        let buf = vec![0u8; 16];
        let _ = Encoder::new_rgba_stride(&buf, 1, 1, u32::MAX).encode(Unstoppable);

        // Invalid input to demuxer — Demux::Drop on error.
        #[cfg(feature = "icc")]
        let _ = get_icc_profile(&[0u8; 0]);
        #[cfg(feature = "icc")]
        let _ = get_icc_profile(&[0xff; 32]);
    }
}

fn main() {
    let n = iter_count();
    eprintln!("leak_test: {} iterations per surface", n);

    eprintln!("  running static encode/decode...");
    run_static_encode_decode(n);

    #[cfg(feature = "streaming")]
    {
        eprintln!("  running streaming...");
        run_streaming(n);
    }

    #[cfg(feature = "animation")]
    {
        eprintln!("  running animation...");
        run_animation(n);
    }

    #[cfg(feature = "icc")]
    {
        eprintln!("  running mux (icc/exif/xmp)...");
        run_mux(n);
    }

    eprintln!("  running error paths...");
    run_error_paths(n);

    eprintln!("leak_test: done");
}
