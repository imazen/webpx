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
