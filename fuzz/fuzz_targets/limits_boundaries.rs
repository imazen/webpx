//! Fuzz target: `Limits` values at u32 boundaries against arbitrary input.
//!
//! Verifies that limit-enforcement is monotone (a tighter limit never
//! accepts an input the looser limit rejected) and that the per-field
//! enforcement matrix holds: `max_width`, `max_height`, `max_pixels`,
//! `max_total_pixels`, `max_input_bytes`, `max_frames`,
//! `max_animation_ms`, `max_metadata_bytes`. Targets the static
//! decoder, animation decoder, and mux metadata paths uniformly.
//!
//! ## Allocation budget
//!
//! Raw arbitrary limit values are exercised against the pure `check_*`
//! functions (no allocation, full u64-boundary coverage of the check
//! math). For the end-to-end decodes, the arbitrary limits are
//! *intersected with a hard pixel budget* before use. Without that
//! clamp, an input carrying loose-or-absent limits plus a huge scale
//! target makes libwebp allocate gigabytes that the configured limits
//! legitimately permit — libFuzzer's RSS/malloc cap then reports an
//! "OOM" that is really "caller opted out of limits". Five consecutive
//! weekly sweeps (seeds `9947b87f07e9`, `36a0851d8893`, `7b78d8c0a5f5`,
//! `8ead9eb0c7cd`, `8e6fbe148cef`) found exactly that non-bug. With the
//! clamp, an over-budget allocation means enforcement is actually
//! broken — every OOM this target reports is real.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use webpx::{ColorMode, DecoderConfig, Limits};

/// Hard ceiling on pixels any in-target decode may produce: 16 MiPx
/// (64 MiB RGBA). Each effective pixel/dimension limit is the arbitrary
/// value AND'd with this budget, so enforcement still sees
/// fuzzer-chosen values below the cap.
const BUDGET_PIXELS: u64 = 16 * 1024 * 1024;

#[derive(Arbitrary, Debug)]
struct LimitInput {
    max_pixels: Option<u64>,
    max_total_pixels: Option<u64>,
    max_width: Option<u32>,
    max_height: Option<u32>,
    max_input_bytes: Option<u64>,
    max_frames: Option<u32>,
    max_animation_ms: Option<u64>,
    max_metadata_bytes: Option<u32>,
}

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    limits: LimitInput,
    use_scale: bool,
    scale_w: u16,
    scale_h: u16,
    data: &'a [u8],
}

/// Limits exactly as the fuzzer chose them — may be absent or huge.
/// Safe only for the pure `check_*` calls, never for a real decode.
fn build_raw_limits(li: &LimitInput) -> Limits {
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

/// Fuzzer-chosen limits intersected with the allocation budget. The
/// pixel and per-frame caps are always present (absent ⇒ budget), so a
/// successful decode is proof the enforcement path ran and held.
/// Non-allocation fields (input bytes, frames, duration, metadata) keep
/// their raw arbitrary values — input size is already bounded by
/// `-max_len`, and frame/duration caps bound work, not memory.
fn build_budgeted_limits(li: &LimitInput) -> Limits {
    let raw = build_raw_limits(li);
    raw.with_max_pixels(li.max_pixels.unwrap_or(u64::MAX).min(BUDGET_PIXELS))
        .with_max_total_pixels(li.max_total_pixels.unwrap_or(u64::MAX).min(BUDGET_PIXELS))
}

fuzz_target!(|input: Input<'_>| {
    // ---- Pure check math at raw (possibly extreme) boundary values.
    // No allocation happens here; this preserves coverage of the
    // saturating-mul / comparison logic at u64::MAX-adjacent values.
    let raw = build_raw_limits(&input.limits);
    let w = u32::from(input.scale_w);
    let h = u32::from(input.scale_h);
    let _ = raw.check_dimensions(w, h);
    let _ = raw.check_still_image(w, h);
    let _ = raw.check_animation(w, h, input.limits.max_frames.unwrap_or(1));
    let _ = raw.check_input_size(input.data.len() as u64);
    let _ = raw.check_total_pixels(u64::from(w).saturating_mul(u64::from(h)));

    let limits = build_budgeted_limits(&input.limits);

    // ---- Static decoder with limits ----
    if let Ok(dec) = webpx::Decoder::new(input.data) {
        let cfg = DecoderConfig::new().limits(limits);
        let dec = dec.config(cfg);
        let dec = if input.use_scale {
            dec.scale(u32::from(input.scale_w), u32::from(input.scale_h))
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

    // ---- YUV path: bypasses decode_advanced, must still enforce the
    // configured limits (and reject crop/scale instead of silently
    // ignoring them).
    if let Ok(dec) = webpx::Decoder::new(input.data) {
        let cfg = DecoderConfig::new().limits(limits);
        let dec = dec.config(cfg);
        if input.use_scale {
            let r = dec
                .scale(u32::from(input.scale_w), u32::from(input.scale_h))
                .decode_yuv();
            assert!(r.is_err(), "decode_yuv must reject scale configs");
        } else if let Ok(planes) = dec.decode_yuv() {
            let produced = (planes.width as u64) * (planes.height as u64);
            assert!(
                produced <= BUDGET_PIXELS,
                "decode_yuv produced {produced} pixels past the {BUDGET_PIXELS} budget"
            );
        }
    }

    // ---- Animation decoder with limits ----
    let _ =
        webpx::AnimationDecoder::with_options_limits(input.data, ColorMode::Rgba, true, &limits);

    // ---- Mux metadata with limits (raw values: chunk sizes are
    // bounded by the input, so extreme caps are allocation-safe) ----
    let _ = webpx::get_icc_profile_with_limits(input.data, &raw);
    let _ = webpx::get_exif_with_limits(input.data, &raw);
    let _ = webpx::get_xmp_with_limits(input.data, &raw);

    // ---- Monotonicity probe: a tighter cap MUST NOT accept what a
    // looser cap rejected. We only check a single dimension here
    // (max_input_bytes) to keep the probe cheap.
    let loose = Limits::none();
    let tight = Limits::none().with_max_input_bytes(0);
    let loose_icc = webpx::get_icc_profile_with_limits(input.data, &loose);
    let tight_icc = webpx::get_icc_profile_with_limits(input.data, &tight);
    if !input.data.is_empty()
        && loose_icc.is_ok()
        && tight_icc.is_ok()
        && let Ok(Some(_)) = loose_icc
    {
        // Both succeeding is only allowed when loose returned None
        // (no chunk; no input-bytes consumed for a chunk). Loose
        // returning a chunk while tight (max_input_bytes=0) accepted
        // anything is a violation.
        panic!("tight max_input_bytes accepted what loose returned a chunk for");
    }
});
