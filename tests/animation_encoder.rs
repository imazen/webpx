//! `AnimationEncoder` options: lossless means lossless, loop-count bounds,
//! empty animations.

#![cfg(all(feature = "animation", feature = "encode", feature = "decode"))]

use webpx::{AnimationDecoder, AnimationEncoder, ColorMode};

const W: u32 = 24;
const H: u32 = 16;

/// Smooth RGBA gradient: lossy VP8 is much smaller than lossless for it, so
/// libwebp's `allow_mixed` would pick lossy.
fn frame(seed: usize) -> Vec<u8> {
    let mut px = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H as usize {
        for x in 0..W as usize {
            px.extend_from_slice(&[
                (x * 7 + y * 3 + seed) as u8,
                (x * 3 + y * 11 + seed * 2) as u8,
                ((x ^ y) + seed) as u8,
                255 - (x + y + seed) as u8,
            ]);
        }
    }
    px
}

fn decode_frames(webp: &[u8]) -> Vec<Vec<u8>> {
    AnimationDecoder::with_options(webp, ColorMode::Rgba, false)
        .unwrap()
        .decode_all()
        .unwrap()
        .into_iter()
        .map(|f| f.data)
        .collect()
}

fn encode(mut enc: AnimationEncoder, seeds: &[usize]) -> Vec<u8> {
    for (i, &s) in seeds.iter().enumerate() {
        enc.add_frame_rgba(&frame(s), i as i32 * 100).unwrap();
    }
    enc.finish(seeds.len() as i32 * 100).unwrap()
}

#[test]
fn set_lossless_frames_decode_exactly() {
    let seeds = [0, 60, 120];
    let mut enc = AnimationEncoder::new(W, H).unwrap();
    enc.set_lossless(true);
    let got = decode_frames(&encode(enc, &seeds));
    let want: Vec<Vec<u8>> = seeds.iter().map(|&s| frame(s)).collect();
    assert!(got == want, "lossless animation frames changed");
}

#[test]
fn allow_mixed_never_makes_lossless_frames_lossy() {
    let seeds = [0, 60, 120];
    let mut enc = AnimationEncoder::with_options(W, H, true, 0).unwrap();
    enc.set_lossless(true);
    let got = decode_frames(&encode(enc, &seeds));
    let want: Vec<Vec<u8>> = seeds.iter().map(|&s| frame(s)).collect();
    assert!(got == want, "lossless animation frames changed");
}

#[test]
fn allow_mixed_rejects_lossless_after_lossy_first_frame() {
    let mut enc = AnimationEncoder::with_options(W, H, true, 0).unwrap();
    enc.add_frame_rgba(&frame(0), 0).unwrap();
    enc.set_lossless(true);
    assert!(enc.add_frame_rgba(&frame(60), 100).is_err());
    // Without allow_mixed, switching per frame is fine.
    let mut enc = AnimationEncoder::new(W, H).unwrap();
    enc.add_frame_rgba(&frame(0), 0).unwrap();
    enc.set_lossless(true);
    enc.add_frame_rgba(&frame(60), 100).unwrap();
    let frames = decode_frames(&enc.finish(200).unwrap());
    assert_eq!(frames[1], frame(60));
}

#[test]
fn loop_count_must_fit_sixteen_bits() {
    assert!(AnimationEncoder::with_options(W, H, false, 65536).is_err());
    let webp = encode(
        AnimationEncoder::with_options(W, H, false, 65535).unwrap(),
        &[0, 60],
    );
    let dec = AnimationDecoder::new(&webp).unwrap();
    assert_eq!(dec.info().loop_count, 65535);
}

#[test]
fn finish_without_frames_is_an_error() {
    assert!(AnimationEncoder::new(W, H).unwrap().finish(100).is_err());
}

#[test]
fn background_color_is_stored() {
    for frames in [&[0usize][..], &[0, 60]] {
        let mut enc = AnimationEncoder::new(W, H).unwrap();
        enc.set_background_color(0x8011_2233).unwrap();
        let webp = encode(enc, frames);
        let dec = AnimationDecoder::new(&webp).unwrap();
        assert_eq!(dec.info().bgcolor, 0x8011_2233, "{} frame(s)", frames.len());
    }
}

#[test]
fn options_are_rejected_after_the_first_frame() {
    let mut enc = AnimationEncoder::new(W, H).unwrap();
    enc.add_frame_rgba(&frame(0), 0).unwrap();
    assert!(enc.set_background_color(0).is_err());
    assert!(enc.set_keyframe_interval(0, 1).is_err());
}

#[test]
fn keyframe_interval_is_validated() {
    let set = |min, max| {
        AnimationEncoder::new(W, H)
            .unwrap()
            .set_keyframe_interval(min, max)
    };
    for (min, max) in [(0, 0), (0, 1), (2, 3), (6, 10), (9, 17), (26, 50)] {
        assert!(set(min, max).is_ok(), "({min}, {max})");
    }
    // libwebp would silently change each of these.
    for (min, max) in [(1, 0), (1, 1), (3, 3), (4, 3), (5, 10), (19, 50), (40, 80)] {
        assert!(set(min, max).is_err(), "({min}, {max})");
    }
}

/// Every frame a keyframe costs bytes over the default (no keyframes after
/// the first) for frames that differ only a little.
#[test]
fn keyframe_interval_reaches_libwebp() {
    // The same frame with a 2x2 square moving across it.
    let frames: Vec<Vec<u8>> = (0..8u32)
        .map(|i| {
            let mut px = frame(0);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = (((2 + dy) * W + 2 * i + dx) * 4) as usize;
                px[p..p + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
            px
        })
        .collect();
    let size = |kf: Option<(u32, u32)>| {
        let mut enc = AnimationEncoder::new(W, H).unwrap();
        enc.set_lossless(true);
        if let Some((min, max)) = kf {
            enc.set_keyframe_interval(min, max).unwrap();
        }
        for (i, f) in frames.iter().enumerate() {
            enc.add_frame_rgba(f, i as i32 * 100).unwrap();
        }
        enc.finish(800).unwrap().len()
    };
    let default = size(None);
    assert_eq!(size(Some((0, 0))), default);
    assert!(
        size(Some((0, 1))) > default,
        "all-keyframe {} vs default {default}",
        size(Some((0, 1)))
    );
}

#[test]
fn set_config_applies_every_setting() {
    let seeds = [0, 60, 120];
    let with_method = |m| {
        let mut enc = AnimationEncoder::new(W, H).unwrap();
        enc.set_config(webpx::EncoderConfig::new().quality(80.0).method(m));
        encode(enc, &seeds)
    };
    assert_ne!(with_method(0), with_method(6));
}

/// The webp-animation shim passes `kmin`/`kmax` to libwebp as that crate
/// does; they were ignored.
#[test]
#[allow(deprecated)]
fn compat_keyframe_options_reach_libwebp() {
    use webpx::compat::webp_animation::{Encoder, EncoderOptions};
    let frames: Vec<Vec<u8>> = (0..8u32)
        .map(|i| {
            let mut px = frame(0);
            let p = ((3 * W + 2 * i) * 4) as usize;
            px[p..p + 4].copy_from_slice(&[255, 0, 0, 255]);
            px
        })
        .collect();
    let size = |kmin, kmax| {
        let options = EncoderOptions {
            kmin,
            kmax,
            ..Default::default()
        };
        let mut enc = Encoder::new_with_options((W, H), options).unwrap();
        for (i, f) in frames.iter().enumerate() {
            enc.add_frame(f, i as i32 * 100).unwrap();
        }
        enc.finalize(800).unwrap().len()
    };
    assert!(size(0, 1) > size(0, 0));
}
