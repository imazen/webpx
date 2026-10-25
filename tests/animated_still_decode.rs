//! Still-image decodes of an animated file return its first composited frame
//! (as zenwebp's do); YUV decodes refuse it, as libwebp does.

#![cfg(all(feature = "decode", feature = "animation"))]

use rgb::RGBA8;
use webpx::{Decoder, DecoderConfig};

fn anim() -> Vec<u8> {
    std::fs::read("tests/fixtures/animated.webp").unwrap()
}

/// The oracle: libwebp's animation decoder, first frame, RGBA.
fn first() -> (Vec<u8>, u32, u32) {
    let mut d = webpx::AnimationDecoder::new(&anim()).unwrap();
    let f = d.next_frame().unwrap().unwrap();
    (f.data, f.width, f.height)
}

fn rgb_of(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
}
fn swap(rgba: &[u8], bpp: usize) -> Vec<u8> {
    rgba.chunks(4)
        .flat_map(|p| {
            let mut q = vec![p[2], p[1], p[0], p[3]];
            q.truncate(bpp);
            q
        })
        .collect()
}

#[test]
fn every_rgb_path_returns_the_first_frame() {
    let a = anim();
    let (px, w, h) = first();
    assert_eq!(webpx::decode_rgba(&a).unwrap(), (px.clone(), w, h));
    assert_eq!(webpx::decode_rgb(&a).unwrap(), (rgb_of(&px), w, h));
    assert_eq!(webpx::decode_bgra(&a).unwrap(), (swap(&px, 4), w, h));
    assert_eq!(webpx::decode_bgr(&a).unwrap(), (swap(&px, 3), w, h));
    let typed: (Vec<RGBA8>, u32, u32) = webpx::decode(&a).unwrap();
    assert_eq!(
        typed
            .0
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect::<Vec<_>>(),
        px
    );
    let mut appended: Vec<RGBA8> = vec![RGBA8::new(1, 2, 3, 4)];
    webpx::decode_append(&a, &mut appended).unwrap();
    assert_eq!(appended.len(), 1 + (w * h) as usize);
    // _into with a padded stride.
    let stride = w * 4 + 8;
    let mut buf = vec![0u8; (stride * h) as usize];
    assert_eq!(
        webpx::decode_rgba_into(&a, &mut buf, stride).unwrap(),
        (w, h)
    );
    for y in 0..h as usize {
        assert_eq!(
            &buf[y * stride as usize..][..(w * 4) as usize],
            &px[y * (w * 4) as usize..][..(w * 4) as usize]
        );
    }
    let mut typed_buf = vec![RGBA8::default(); (w * h) as usize];
    webpx::decode_into(&a, &mut typed_buf, w).unwrap();
    assert_eq!(
        typed_buf
            .iter()
            .flat_map(|p| [p.r, p.g, p.b, p.a])
            .collect::<Vec<_>>(),
        px
    );
    // Builder (advanced path) and its typed output.
    let raw = Decoder::new(&a)
        .unwrap()
        .config(DecoderConfig::new())
        .decode_rgba_raw()
        .unwrap();
    assert_eq!(raw, (px.clone(), w, h));
}

#[test]
fn unsupported_options_and_yuv_are_refused() {
    let a = anim();
    let d = |c: DecoderConfig| Decoder::new(&a).unwrap().config(c).decode_rgba_raw();
    assert!(d(DecoderConfig::new().crop(0, 0, 4, 4)).is_err());
    assert!(d(DecoderConfig::new().scale(8, 8)).is_err());
    assert!(d(DecoderConfig::new().flip(true)).is_err());
    assert!(webpx::decode_yuv(&a).is_err());
}

/// The incremental decoder buffers an animated stream and returns its first
/// frame from `finish`, fed whole or a few bytes at a time.
#[cfg(feature = "streaming")]
#[test]
fn streaming_decoder_returns_the_first_frame() {
    let a = anim();
    let (px, w, h) = first();
    for chunk in [a.len(), 7, 1] {
        let mut d = webpx::StreamingDecoder::new(webpx::ColorMode::Rgba).unwrap();
        for c in a.chunks(chunk) {
            assert_eq!(d.append(c).unwrap(), webpx::DecodeStatus::NeedMoreData);
        }
        assert_eq!(d.dimensions(), Some((w, h)));
        assert_eq!(d.finish().unwrap(), (px.clone(), w, h), "chunk {chunk}");
    }
    let mut d = webpx::StreamingDecoder::new(webpx::ColorMode::Rgb).unwrap();
    d.append(&a).unwrap();
    assert_eq!(d.finish().unwrap(), (rgb_of(&px), w, h));
    // A caller-owned output buffer can't take an animation.
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let mut d =
        webpx::StreamingDecoder::with_buffer(&mut buf, (w * 4) as usize, webpx::ColorMode::Rgba)
            .unwrap();
    assert!(d.append(&a).is_err());
    // Stills still decode incrementally, a byte at a time.
    let still = std::fs::read("tests/fixtures/lossy_rgb.webp").unwrap();
    let mut d = webpx::StreamingDecoder::new(webpx::ColorMode::Rgba).unwrap();
    for c in still.chunks(1) {
        d.append(c).unwrap();
    }
    assert_eq!(d.finish().unwrap(), webpx::decode_rgba(&still).unwrap());
}
