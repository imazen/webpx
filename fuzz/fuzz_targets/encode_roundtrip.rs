//! Fuzz target: encode pixels -> decode -> compare dimensions.
//!
//! Generates a small image from fuzzer-controlled bytes (raw RGBA), runs it
//! through every encode configuration knob (lossy/lossless/method/preset),
//! decodes the result, and asserts dimensions survived the round trip.
//! Also exercises the AnimationEncoder path.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use webpx::{AnimationEncoder, Encoder, Preset, Unstoppable};

const MAX_DIM: u32 = 256;

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    width: u8,
    height: u8,
    quality: u8,
    method: u8,
    lossless: bool,
    near_lossless: u8,
    alpha_quality: u8,
    exact: bool,
    sharp_yuv: bool,
    preset_choice: u8,
    do_animation: bool,
    pixels: &'a [u8],
}

fn pick_preset(b: u8) -> Preset {
    match b % 6 {
        0 => Preset::Default,
        1 => Preset::Photo,
        2 => Preset::Picture,
        3 => Preset::Drawing,
        4 => Preset::Icon,
        _ => Preset::Text,
    }
}

fuzz_target!(|input: Input<'_>| {
    // Force a non-zero image, capped to keep the fuzzer fast.
    let w = ((input.width as u32) % MAX_DIM).max(1);
    let h = ((input.height as u32) % MAX_DIM).max(1);
    let needed = (w as usize) * (h as usize) * 4;
    if input.pixels.len() < needed {
        return;
    }
    let pixels = &input.pixels[..needed];

    let quality = (input.quality as f32) % 101.0;
    let method = input.method % 7;

    let encoded = Encoder::new_rgba(pixels, w, h)
        .quality(quality)
        .method(method)
        .preset(pick_preset(input.preset_choice))
        .lossless(input.lossless)
        .near_lossless(input.near_lossless.min(100))
        .alpha_quality(input.alpha_quality.min(100))
        .exact(input.exact)
        .sharp_yuv(input.sharp_yuv)
        .encode(Unstoppable);

    let encoded = match encoded {
        Ok(b) if !b.is_empty() => b,
        _ => return,
    };

    // Decode the encoded output and check dimensions match.
    if let Ok((_, dw, dh)) = webpx::decode_rgba(&encoded) {
        assert_eq!(
            dw, w,
            "decoded width mismatch (lossless={})",
            input.lossless
        );
        assert_eq!(
            dh, h,
            "decoded height mismatch (lossless={})",
            input.lossless
        );
    } else {
        // Successful encode that fails to decode is a bug.
        panic!("encoded WebP failed to round-trip decode");
    }

    // ---- Encode through RGB / BGRA / BGR entry points (different pixel paths) ----
    let rgb_needed = (w as usize) * (h as usize) * 3;
    if input.pixels.len() >= rgb_needed {
        let rgb = &input.pixels[..rgb_needed];
        let _ = Encoder::new_rgb(rgb, w, h)
            .quality(quality)
            .method(method)
            .lossless(input.lossless)
            .encode(Unstoppable);
    }
    let _ = Encoder::new_bgra(pixels, w, h)
        .quality(quality)
        .lossless(input.lossless)
        .encode(Unstoppable);

    // ---- Animation: 2 frames ----
    if input.do_animation && input.pixels.len() >= needed * 2 {
        let frame0 = &input.pixels[..needed];
        let frame1 = &input.pixels[needed..needed * 2];
        if let Ok(mut anim) = AnimationEncoder::new(w, h) {
            anim.set_quality(quality);
            anim.set_lossless(input.lossless);
            if anim.add_frame_rgba(frame0, 0).is_ok()
                && anim.add_frame_rgba(frame1, 100).is_ok()
                && let Ok(out) = anim.finish(200)
            {
                // Re-decode as static (first frame) and as animation.
                let _ = webpx::decode_rgba(&out);
                if let Ok(mut dec) = webpx::AnimationDecoder::new(&out) {
                    while let Ok(Some(_)) = dec.next_frame() {}
                }
            }
        }
    }
});
