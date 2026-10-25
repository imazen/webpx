//! `DecoderConfig::apply_orientation`: decoded pixels come back the way the
//! EXIF orientation says to display them.

#![cfg(all(feature = "decode", feature = "encode", feature = "icc"))]

use webpx::{Decoder, DecoderConfig, Orientation, Unstoppable};

const W: u32 = 6;
const H: u32 = 4;

fn tiff(value: u16) -> Vec<u8> {
    let mut t = b"II*\0\x08\0\0\0\x01\0".to_vec();
    t.extend_from_slice(&0x0112u16.to_le_bytes());
    t.extend_from_slice(&3u16.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(&value.to_le_bytes());
    t.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    t
}

fn file(tag: u16) -> (Vec<u8>, Vec<u8>) {
    let px: Vec<u8> = (0..W * H)
        .flat_map(|i| [i as u8 * 9, 255 - i as u8, 3, 255])
        .collect();
    let webp = webpx::Encoder::new_rgba(&px, W, H)
        .lossless(true)
        .encode(Unstoppable)
        .unwrap();
    (webpx::embed_exif(&webp, &tiff(tag)).unwrap(), px)
}

fn render(px: &[u8], w: u32, h: u32, o: Orientation) -> (Vec<u8>, u32, u32) {
    let (ow, oh) = o.output_dimensions(w, h);
    let mut out = vec![0; px.len()];
    for sy in 0..h {
        for sx in 0..w {
            let (dx, dy) = o.forward_map(sx, sy, w, h);
            let (s, d) = (((sy * w + sx) * 4) as usize, ((dy * ow + dx) * 4) as usize);
            out[d..d + 4].copy_from_slice(&px[s..s + 4]);
        }
    }
    (out, ow, oh)
}

#[test]
fn apply_orientation_returns_display_pixels() {
    for tag in 1..=8u16 {
        let o = Orientation::from_exif(tag as u8).unwrap();
        let (webp, px) = file(tag);
        let dec = |cfg: DecoderConfig| {
            Decoder::new(&webp)
                .unwrap()
                .config(cfg)
                .decode_rgba_raw()
                .unwrap()
        };
        // Default: stored order.
        assert_eq!(dec(DecoderConfig::new()), (px.clone(), W, H), "tag {tag}");
        assert_eq!(
            dec(DecoderConfig::new().apply_orientation(true)),
            render(&px, W, H, o),
            "tag {tag}"
        );
        // flip first, then the transform: same displayed image.
        assert_eq!(
            dec(DecoderConfig::new().flip(true).apply_orientation(true)),
            render(&px, W, H, o),
            "tag {tag} flip"
        );
        // RGB and typed paths agree with RGBA.
        let (rgb, rw, rh) = Decoder::new(&webp)
            .unwrap()
            .config(DecoderConfig::new().apply_orientation(true))
            .decode_rgb_raw()
            .unwrap();
        let want = render(&px, W, H, o);
        let want_rgb: Vec<u8> = want.0.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        assert_eq!((rgb, rw, rh), (want_rgb, want.1, want.2), "tag {tag} rgb");
    }
}

#[test]
fn simple_paths_reject_apply_orientation() {
    let (webp, _) = file(6);
    let d = || {
        Decoder::new(&webp)
            .unwrap()
            .config(DecoderConfig::new().apply_orientation(true))
    };
    let mut buf = vec![0u8; (W * H * 4) as usize];
    assert!(d().decode_rgba_into(&mut buf, W * 4).is_err());
    assert!(d().decode_yuv().is_err());
}

#[cfg(feature = "zencodec")]
#[test]
fn zencodec_treats_it_as_correct_without_double_rotation() {
    use std::borrow::Cow;
    use zencodec::decode::{Decode as _, DecodeJob as _, DecoderConfig as _};
    let (webp, px) = file(6);
    let out = webpx::zencodec::WebpDecoderConfig::from_native(
        DecoderConfig::new().apply_orientation(true),
    )
    .job()
    .decoder(
        Cow::Borrowed(&webp),
        &[zenpixels::PixelDescriptor::RGBA8_SRGB],
    )
    .unwrap()
    .decode()
    .unwrap();
    assert_eq!(out.info().orientation, Orientation::Identity);
    let buf = out.into_buffer();
    let s = buf.as_slice();
    let got: Vec<u8> = (0..s.rows())
        .flat_map(|y| s.row(y)[..s.width() as usize * 4].to_vec())
        .collect();
    assert_eq!(
        (got, s.width(), s.rows()),
        render(&px, W, H, Orientation::Rotate90)
    );
}
