//! Fuzz crash regression suite.
//!
//! Runs every file in `fuzz/regression/` through every public entry point that
//! has a fuzz target. Each seed file is a previously-found crash that has been
//! fixed; this test ensures none re-introduce a panic.
//!
//! Reproduces what the fuzz targets do, but as a regular `cargo test` — no
//! nightly toolchain required. Failures here mean a regression of a
//! previously-fixed bug.
//!
//! To add a new seed: drop the (preferably minimized) crash file into
//! `fuzz/regression/` with a `crash-<sha>` name, no other action required.

use std::fs;
use std::path::PathBuf;

#[cfg(feature = "decode")]
const MAX_PIXEL_BYTES: usize = 256 * 1024 * 1024;

fn regression_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fuzz/regression")
}

#[cfg(feature = "decode")]
fn run_decode_static(input: &[u8]) {
    let _ = webpx::ImageInfo::from_webp(input);
    let info = match webpx::ImageInfo::from_webp(input) {
        Ok(i) => i,
        Err(_) => return,
    };
    let total_bytes = (info.width as usize)
        .saturating_mul(info.height as usize)
        .saturating_mul(4);
    if total_bytes > MAX_PIXEL_BYTES {
        return;
    }
    let _ = webpx::decode_rgba(input);
    let _ = webpx::decode_rgb(input);
    let _ = webpx::decode_bgra(input);
    let _ = webpx::decode_bgr(input);
    let _ = webpx::decode_yuv(input);
}

#[cfg(feature = "decode")]
fn run_decoder_builder(input: &[u8]) {
    if let Ok(dec) = webpx::Decoder::new(input) {
        let _ = dec.decode_rgba();
    }
    if let Ok(dec) = webpx::Decoder::new(input) {
        let _ = dec.scale(32, 32).decode_rgba();
    }
    if let Ok(dec) = webpx::Decoder::new(input) {
        let _ = dec.crop(0, 0, 16, 16).decode_rgb();
    }
    // Edge cases the fuzzer reaches but a fixed harness misses: zero
    // scale and zero crop dimensions used to panic inside imgref instead
    // of returning Err. Make sure every seed exercises both.
    if let Ok(dec) = webpx::Decoder::new(input) {
        let _ = dec.scale(0, 0).decode_rgba();
    }
    if let Ok(dec) = webpx::Decoder::new(input) {
        let _ = dec.crop(0, 0, 0, 0).decode_rgba();
    }
}

#[cfg(all(feature = "decode", feature = "streaming"))]
fn run_streaming(input: &[u8]) {
    use webpx::{ColorMode, StreamingDecoder};
    if let Ok(mut dec) = StreamingDecoder::new(ColorMode::Rgba) {
        let _ = dec.append(input);
        let _ = dec.finish();
    }
}

#[cfg(all(feature = "decode", feature = "animation"))]
fn run_animation(input: &[u8]) {
    if let Ok(mut dec) = webpx::AnimationDecoder::new(input) {
        let mut count = 0;
        while let Ok(Some(_frame)) = dec.next_frame() {
            count += 1;
            if count >= 64 {
                break;
            }
        }
    }
}

#[cfg(feature = "icc")]
fn run_mux(input: &[u8]) {
    let _ = webpx::get_icc_profile(input);
    let _ = webpx::get_exif(input);
    let _ = webpx::get_xmp(input);
    let _ = webpx::remove_icc(input);
    let _ = webpx::remove_exif(input);
    let _ = webpx::remove_xmp(input);
    if let Ok(emb) = webpx::embed_icc(input, b"\x00\x00\x00\x00") {
        let _ = webpx::get_icc_profile(&emb);
    }
}

#[test]
fn fuzz_regression_seeds_do_not_panic() {
    let dir = regression_dir();
    let entries: Vec<_> = match fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .collect(),
        Err(_) => Vec::new(),
    };

    if entries.is_empty() {
        eprintln!(
            "no regression seeds in {} — populate from fuzz crashes as they're discovered",
            dir.display()
        );
        return;
    }

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unnamed>");
        let input = fs::read(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));

        #[cfg(feature = "decode")]
        run_decode_static(&input);
        #[cfg(feature = "decode")]
        run_decoder_builder(&input);
        #[cfg(all(feature = "decode", feature = "streaming"))]
        run_streaming(&input);
        #[cfg(all(feature = "decode", feature = "animation"))]
        run_animation(&input);
        #[cfg(feature = "icc")]
        run_mux(&input);

        eprintln!("ok: {name} ({} bytes)", input.len());
    }
}
