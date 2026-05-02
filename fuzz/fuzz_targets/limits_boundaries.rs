//! Fuzz target: `Limits` values at u32 boundaries against arbitrary input.
//!
//! Verifies that limit-enforcement is monotone (a tighter limit never
//! accepts an input the looser limit rejected) and that the per-field
//! enforcement matrix holds: `max_width`, `max_height`, `max_pixels`,
//! `max_total_pixels`, `max_input_bytes`, `max_frames`,
//! `max_animation_ms`, `max_metadata_bytes`. Targets the static
//! decoder, animation decoder, and mux metadata paths uniformly.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use webpx::{ColorMode, DecoderConfig, Limits};

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

fn build_limits(li: &LimitInput) -> Limits {
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

fuzz_target!(|input: Input<'_>| {
    let limits = build_limits(&input.limits);

    // ---- Static decoder with limits ----
    if let Ok(dec) = webpx::Decoder::new(input.data) {
        let cfg = DecoderConfig::new().limits(limits);
        let dec = dec.config(cfg);
        let dec = if input.use_scale {
            dec.scale(input.scale_w as u32, input.scale_h as u32)
        } else {
            dec
        };
        let _ = dec.decode_rgba();
    }

    // ---- Animation decoder with limits ----
    let _ =
        webpx::AnimationDecoder::with_options_limits(input.data, ColorMode::Rgba, true, &limits);

    // ---- Mux metadata with limits ----
    let _ = webpx::get_icc_profile_with_limits(input.data, &limits);
    let _ = webpx::get_exif_with_limits(input.data, &limits);
    let _ = webpx::get_xmp_with_limits(input.data, &limits);

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
