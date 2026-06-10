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

/// Decode budget for the `limits_boundaries` runner, matching the fuzz
/// target: arbitrary seed limits are intersected with this pixel cap so
/// a seed carrying loose-or-absent limits can't make the test itself
/// allocate gigabytes. If limit enforcement regresses, the
/// success-implies-budget asserts below fail loudly.
#[cfg(feature = "decode")]
const BUDGET_PIXELS: u64 = 16 * 1024 * 1024;

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

/// Mirror of `fuzz/fuzz_targets/limits_boundaries.rs`: seed files for
/// that target are `Arbitrary`-encoded `Input` values, not bare WebP
/// bitstreams, so they must be decoded with the same struct layout the
/// target used to produce them. Keep these structs byte-compatible with
/// the fuzz target's.
#[cfg(feature = "decode")]
mod limits_seed {
    use arbitrary::{Arbitrary, Unstructured};
    use webpx::Limits;

    #[derive(Arbitrary, Debug)]
    pub struct LimitInput {
        pub max_pixels: Option<u64>,
        pub max_total_pixels: Option<u64>,
        pub max_width: Option<u32>,
        pub max_height: Option<u32>,
        pub max_input_bytes: Option<u64>,
        pub max_frames: Option<u32>,
        pub max_animation_ms: Option<u64>,
        pub max_metadata_bytes: Option<u32>,
    }

    #[derive(Arbitrary, Debug)]
    pub struct Input<'a> {
        pub limits: LimitInput,
        pub use_scale: bool,
        pub scale_w: u16,
        pub scale_h: u16,
        pub data: &'a [u8],
    }

    pub fn parse(bytes: &[u8]) -> Option<Input<'_>> {
        Input::arbitrary_take_rest(Unstructured::new(bytes)).ok()
    }

    pub fn build_raw_limits(li: &LimitInput) -> Limits {
        let mut l = Limits::none();
        if let Some(v) = li.max_pixels {
            l = l.with_max_pixels(v);
        }
        if let Some(v) = li.max_total_pixels {
            l = l.with_max_total_pixels(v);
        }
        if let Some(v) = li.max_width {
            l = l.with_max_width(v);
        }
        if let Some(v) = li.max_height {
            l = l.with_max_height(v);
        }
        if let Some(v) = li.max_input_bytes {
            l = l.with_max_input_bytes(v);
        }
        if let Some(v) = li.max_frames {
            l = l.with_max_frames(v);
        }
        if let Some(v) = li.max_animation_ms {
            l = l.with_max_animation_ms(v);
        }
        if let Some(v) = li.max_metadata_bytes {
            l = l.with_max_metadata_bytes(v);
        }
        l
    }

    pub fn build_budgeted_limits(li: &LimitInput, budget: u64) -> Limits {
        build_raw_limits(li)
            .with_max_pixels(li.max_pixels.unwrap_or(u64::MAX).min(budget))
            .with_max_total_pixels(li.max_total_pixels.unwrap_or(u64::MAX).min(budget))
    }
}

#[cfg(feature = "decode")]
fn run_limits_boundaries(input: &[u8]) {
    use webpx::DecoderConfig;

    let Some(seed) = limits_seed::parse(input) else {
        return;
    };

    // Pure check math at the seed's raw (possibly extreme) limit values.
    let raw = limits_seed::build_raw_limits(&seed.limits);
    let w = u32::from(seed.scale_w);
    let h = u32::from(seed.scale_h);
    let _ = raw.check_dimensions(w, h);
    let _ = raw.check_still_image(w, h);
    let _ = raw.check_animation(w, h, seed.limits.max_frames.unwrap_or(1));
    let _ = raw.check_input_size(seed.data.len() as u64);
    let _ = raw.check_total_pixels(u64::from(w).saturating_mul(u64::from(h)));

    let limits = limits_seed::build_budgeted_limits(&seed.limits, BUDGET_PIXELS);

    // Static decoder path.
    if let Ok(dec) = webpx::Decoder::new(seed.data) {
        let cfg = DecoderConfig::new().limits(limits);
        let dec = dec.config(cfg);
        let dec = if seed.use_scale {
            dec.scale(u32::from(seed.scale_w), u32::from(seed.scale_h))
        } else {
            dec
        };
        if let Ok(img) = dec.decode_rgba() {
            let produced = (img.width() as u64) * (img.height() as u64);
            assert!(
                produced <= BUDGET_PIXELS,
                "decode produced {produced} pixels past the {BUDGET_PIXELS} budget"
            );
        }
    }

    // YUV path: bypasses decode_advanced, must still enforce limits and
    // reject crop/scale rather than silently ignoring them.
    if let Ok(dec) = webpx::Decoder::new(seed.data) {
        let cfg = DecoderConfig::new().limits(limits);
        let dec = dec.config(cfg);
        if seed.use_scale {
            assert!(
                dec.scale(u32::from(seed.scale_w), u32::from(seed.scale_h))
                    .decode_yuv()
                    .is_err(),
                "decode_yuv must reject scale configs"
            );
        } else if let Ok(planes) = dec.decode_yuv() {
            let produced = u64::from(planes.width) * u64::from(planes.height);
            assert!(
                produced <= BUDGET_PIXELS,
                "decode_yuv produced {produced} pixels past the {BUDGET_PIXELS} budget"
            );
        }
    }

    // Animation path with budgeted limits.
    #[cfg(feature = "animation")]
    {
        let _ = webpx::AnimationDecoder::with_options_limits(
            seed.data,
            webpx::ColorMode::Rgba,
            true,
            &limits,
        );
    }

    // Mux metadata paths with raw limits (chunk sizes are input-bounded).
    #[cfg(feature = "icc")]
    {
        let _ = webpx::get_icc_profile_with_limits(seed.data, &raw);
        let _ = webpx::get_exif_with_limits(seed.data, &raw);
        let _ = webpx::get_xmp_with_limits(seed.data, &raw);
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
        #[cfg(feature = "decode")]
        run_limits_boundaries(&input);
        #[cfg(all(feature = "decode", feature = "streaming"))]
        run_streaming(&input);
        #[cfg(all(feature = "decode", feature = "animation"))]
        run_animation(&input);
        #[cfg(feature = "icc")]
        run_mux(&input);

        eprintln!("ok: {name} ({} bytes)", input.len());
    }
}
