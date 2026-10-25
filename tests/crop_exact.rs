//! A crop decode returns exactly the pixels of the same region of a full
//! decode, for lossy and lossless files, at odd and even origins, with and
//! without `flip`.

#![cfg(feature = "decode")]

use webpx::{Decoder, DecoderConfig, Limits};

fn decode(data: &[u8], cfg: DecoderConfig, rgb: bool) -> (Vec<u8>, u32, u32) {
    let d = Decoder::new(data)
        .unwrap()
        .config(cfg.limits(Limits::none()));
    if rgb {
        d.decode_rgb_raw()
    } else {
        d.decode_rgba_raw()
    }
    .unwrap()
}

fn files() -> Vec<(String, Vec<u8>)> {
    let mut out = vec![];
    for f in ["lossy_alpha.webp", "lossy_rgb.webp", "lossless_2color.webp"] {
        out.push((
            f.into(),
            std::fs::read(format!("tests/fixtures/{f}")).unwrap(),
        ));
    }
    #[cfg(feature = "encode")]
    for (w, h) in [(37u32, 29u32), (16, 16), (5, 3)] {
        let px: Vec<u8> = (0..w * h)
            .flat_map(|i| {
                let (x, y) = (i % w, i / w);
                [
                    (x * 9) as u8,
                    (y * 13) as u8,
                    ((x ^ y) * 21) as u8,
                    (255 - x * 3) as u8,
                ]
            })
            .collect();
        for lossless in [false, true] {
            let webp = webpx::EncoderConfig::new()
                .quality(70.0)
                .lossless(lossless)
                .encode_rgba(&px, w, h, webpx::Unstoppable)
                .unwrap();
            out.push((format!("{w}x{h} lossless={lossless}"), webp));
        }
    }
    out
}

#[test]
fn crops_match_the_full_decode() {
    let mut checked = 0;
    for (name, data) in files() {
        let info = webpx::ImageInfo::from_webp(&data).unwrap();
        let (w, h) = (info.width, info.height);
        for flip in [false, true] {
            for rgb in [false, true] {
                let bpp = if rgb { 3 } else { 4 };
                let (full, ..) = decode(&data, DecoderConfig::new().flip(flip), rgb);
                let mut rects = vec![
                    (0, 0, w, h),
                    (1, 1, w - 1, h - 1),
                    (0, 0, 1, 1),
                    (w - 1, h - 1, 1, 1),
                ];
                for (x, y) in [
                    (1, 0),
                    (0, 1),
                    (2, 2),
                    (3, 1),
                    (w / 2, h / 3),
                    ((w / 3) | 1, (h / 2) | 1),
                ] {
                    if x < w && y < h {
                        rects.push((x, y, (w - x).min(7), (h - y).min(5)));
                        rects.push((x, y, w - x, h - y));
                    }
                }
                for (x, y, cw, ch) in rects {
                    let (c, ow, oh) = decode(
                        &data,
                        DecoderConfig::new().flip(flip).crop(x, y, cw, ch),
                        rgb,
                    );
                    assert_eq!((ow, oh), (cw, ch), "{name} {x},{y},{cw},{ch}");
                    for yy in 0..ch {
                        // With flip the full decode is upside down too.
                        let fy = if flip { h - 1 - (y + yy) } else { y + yy };
                        let cy = if flip { ch - 1 - yy } else { yy };
                        let a = &full
                            [((fy * w + x) * bpp) as usize..((fy * w + x + cw) * bpp) as usize];
                        let b = &c[(cy * cw * bpp) as usize..((cy + 1) * cw * bpp) as usize];
                        assert_eq!(
                            a, b,
                            "{name} flip={flip} rgb={rgb} crop {x},{y},{cw},{ch} row {yy}"
                        );
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 300, "{checked}");
}

/// libwebp rounds a lossy crop origin to even before scaling, which can't be
/// undone after scaling, so that combination is an error rather than a
/// shifted image. Even origins still scale.
#[test]
fn odd_lossy_crop_with_scaling_is_rejected() {
    let data = std::fs::read("tests/fixtures/lossy_rgb.webp").unwrap();
    let d = |x, y| {
        Decoder::new(&data)
            .unwrap()
            .config(DecoderConfig::new().crop(x, y, 40, 40).scale(20, 20))
            .decode_rgba_raw()
    };
    assert!(d(1, 0).is_err());
    assert!(d(0, 3).is_err());
    assert_eq!(d(2, 4).unwrap().1, 20);
}
