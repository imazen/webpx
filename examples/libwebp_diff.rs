//! Differential harness: the same decode/encode calls through webpx, built
//! against two libwebp versions. Prints one line per (input, operation) with a
//! hash of the output or the error, so two runs can be diffed.
//!
//! To compare against another libwebp: copy libwebp-sys's source, replace its
//! `vendor/src` and `vendor/sharpyuv` with the other libwebp's `src` and
//! `sharpyuv` (the headers' ABI versions must match the bindings), add
//! `[patch.crates-io] libwebp-sys = { path = ... }` to a copy of this repo,
//! build this example in both trees and `diff` the outputs. Used on
//! 2026-10-25 for libwebp 1.6.0 vs main (see CLAUDE.md "libwebp API Notes").
//!
//!   gen <png-list> <out-dir>   write webp variants of each PNG (run once)
//!   dec <webp-list>            decode each file under many option sets
//!   enc <png-list>             encode each PNG under many option sets

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use webpx::{AlphaFilter, ColorMode, Decoder, DecoderConfig, EncoderConfig, Limits};

fn h(b: &[u8]) -> String {
    let mut s = DefaultHasher::new();
    b.hash(&mut s);
    format!("{:016x}/{}", s.finish(), b.len())
}

fn res<T>(r: webpx::Result<T>, f: impl FnOnce(T) -> String) -> String {
    match r {
        Ok(v) => f(v),
        Err(e) => format!("ERR {}", e.error()),
    }
}

fn load_png(p: &str) -> Option<(Vec<u8>, u32, u32)> {
    let img = image::open(p).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    if w > 1200 || h > 1200 || w < 2 || h < 2 {
        return None;
    }
    Some((img.into_raw(), w, h))
}

fn variants() -> Vec<(&'static str, EncoderConfig)> {
    let lossy = || EncoderConfig::new().quality(75.0);
    vec![
        ("q75", lossy()),
        ("q75-afnone", lossy().alpha_filter(AlphaFilter::None)),
        ("q75-afbest", lossy().alpha_filter(AlphaFilter::Best)),
        ("q75-aq50", lossy().alpha_quality(50)),
        (
            "q75-aq10-afbest",
            lossy().alpha_quality(10).alpha_filter(AlphaFilter::Best),
        ),
        (
            "q40-m6-seg1",
            EncoderConfig::new().quality(40.0).method(6).segments(1),
        ),
        (
            "q90-m4-pass6",
            EncoderConfig::new().quality(90.0).method(4).pass(6),
        ),
        ("ll", EncoderConfig::new_lossless()),
        ("ll-z9", EncoderConfig::new_lossless_level(9)),
        ("nearll60", EncoderConfig::new_lossless().near_lossless(60)),
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let list = std::fs::read_to_string(&args[2]).unwrap();
    let files: Vec<&str> = list.lines().filter(|l| !l.is_empty()).collect();
    match args[1].as_str() {
        "gen" => {
            let out = &args[3];
            for f in &files {
                let Some((px, w, hh)) = load_png(f) else {
                    continue;
                };
                let stem = std::path::Path::new(f)
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                for (name, cfg) in variants() {
                    if let Ok(webp) = cfg.encode_rgba(&px, w, hh, webpx::Unstoppable) {
                        std::fs::write(format!("{out}/{stem}.{name}.webp"), webp).unwrap();
                    }
                }
            }
        }
        "enc" => {
            for f in &files {
                let Some((px, w, hh)) = load_png(f) else {
                    continue;
                };
                for (name, cfg) in variants() {
                    println!(
                        "{f}\tenc:{name}\t{}",
                        res(cfg.encode_rgba(&px, w, hh, webpx::Unstoppable), |v| h(&v))
                    );
                }
                for sh in [0usize, 1, 2, 3] {
                    // RGBA input that starts at a non-4-byte-aligned address.
                    let mut buf = vec![0u8; px.len() + sh];
                    buf[sh..].copy_from_slice(&px);
                    let r = EncoderConfig::new_lossless().encode_rgba(
                        &buf[sh..],
                        w,
                        hh,
                        webpx::Unstoppable,
                    );
                    println!("{f}\tenc:ll-offset{sh}\t{}", res(r, |v| h(&v)));
                }
                let nan = EncoderConfig::new().quality(f32::NAN).encode_rgba(
                    &px,
                    w,
                    hh,
                    webpx::Unstoppable,
                );
                println!("{f}\tenc:nan\t{}", res(nan, |v| h(&v)));
            }
        }
        "dec" => {
            for f in &files {
                let data = std::fs::read(f).unwrap();
                let Ok(info) = webpx::ImageInfo::from_webp(&data) else {
                    println!("{f}\tinfo\tERR");
                    continue;
                };
                if info.has_animation {
                    let r = webpx::AnimationDecoder::with_options_limits(
                        &data,
                        ColorMode::Rgba,
                        false,
                        &Limits::none(),
                    )
                    .and_then(|mut d| d.decode_all());
                    println!(
                        "{f}\tanim\t{}",
                        res(r, |fr| {
                            let mut all = Vec::new();
                            for x in fr {
                                all.extend_from_slice(&x.data);
                                all.extend_from_slice(&x.duration_ms.to_le_bytes());
                            }
                            h(&all)
                        })
                    );
                    continue;
                }
                let (w, hh) = (info.width, info.height);
                let mut cfgs: Vec<(String, DecoderConfig)> = vec![
                    ("plain".into(), DecoderConfig::new()),
                    (
                        "nofancy".into(),
                        DecoderConfig::new().no_fancy_upsampling(true),
                    ),
                    ("bypass".into(), DecoderConfig::new().bypass_filtering(true)),
                    ("flip".into(), DecoderConfig::new().flip(true)),
                    (
                        "adither100".into(),
                        DecoderConfig::new().alpha_dithering(100),
                    ),
                ];
                if w >= 4 && hh >= 4 {
                    for (x, y, cw, ch) in [
                        (0, 1, w, hh - 1),
                        (1, 1, w / 2, hh / 2),
                        (3, 3, w - 3, hh - 3),
                        (w / 3, hh / 3, w / 3, hh / 3),
                        (0, hh / 2, w, hh - hh / 2),
                    ] {
                        if cw == 0 || ch == 0 {
                            continue;
                        }
                        cfgs.push((
                            format!("crop{x},{y},{cw},{ch}"),
                            DecoderConfig::new().crop(x, y, cw, ch),
                        ));
                    }
                    for (sw, sh) in [
                        (w / 2, hh / 2),
                        ((w / 3).max(1), (hh / 7).max(1)),
                        (1, 1),
                        ((w * 2).min(4000), (hh * 2).min(4000)),
                        (w, (hh / 3).max(1)),
                    ] {
                        cfgs.push((
                            format!("scale{sw}x{sh}"),
                            DecoderConfig::new().scale(sw, sh),
                        ));
                    }
                    cfgs.push((
                        "crop+scale".into(),
                        DecoderConfig::new()
                            .crop(1, 1, w / 2, hh / 2)
                            .scale((w / 4).max(1), (hh / 4).max(1)),
                    ));
                    cfgs.push((
                        "crop+scaleup".into(),
                        DecoderConfig::new().crop(1, 3, w / 2, hh / 2).scale(w, hh),
                    ));
                }
                for (name, cfg) in cfgs {
                    let cfg = cfg.limits(Limits::none());
                    for (mode, r) in [
                        (
                            "rgba",
                            Decoder::new(&data)
                                .map(|d| d.config(cfg.clone()))
                                .and_then(|d| d.decode_rgba_raw()),
                        ),
                        (
                            "rgb",
                            Decoder::new(&data)
                                .map(|d| d.config(cfg.clone()))
                                .and_then(|d| d.decode_rgb_raw()),
                        ),
                    ] {
                        println!(
                            "{f}\t{name}:{mode}\t{}",
                            res(r, |(v, ow, oh)| format!("{ow}x{oh} {}", h(&v)))
                        );
                    }
                }
                let y = Decoder::new(&data)
                    .map(|d| d.config(DecoderConfig::new().limits(Limits::none())))
                    .and_then(|d| d.decode_yuv());
                println!(
                    "{f}\tyuv\t{}",
                    res(y, |p| {
                        let mut a = p.y.clone();
                        a.extend_from_slice(&p.u);
                        a.extend_from_slice(&p.v);
                        h(&a)
                    })
                );
            }
        }
        _ => panic!("mode"),
    }
}
