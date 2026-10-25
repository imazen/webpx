//! EXIF orientation through the zencodec layer: what each `OrientationHint`
//! reports and outputs on every decode path, and what encoding writes.

#![cfg(feature = "zencodec")]

use std::borrow::Cow;
use webpx::zencodec::{WebpDecoderConfig, WebpEncoderConfig};
#[cfg(feature = "streaming")]
use zencodec::decode::StreamingDecode as _;
use zencodec::decode::{Decode as _, DecodeJob as _, DecodeRowSink, DecoderConfig as _, SinkError};
use zencodec::encode::{EncodeJob as _, Encoder as _, EncoderConfig as _};
use zencodec::{Metadata, MetadataPolicy, Orientation, OrientationHint};
use zenpixels::{PixelDescriptor, PixelSlice, PixelSliceMut};

const W: u32 = 5;
const H: u32 = 3;

/// `W`×`H` RGBA with a distinct value in every pixel.
fn image() -> Vec<u8> {
    (0..W * H)
        .flat_map(|i| [i as u8 * 10, 255 - i as u8, i as u8 ^ 0x55, 255])
        .collect()
}

/// `px` (`w`×`h` RGBA) as displayed under `o`.
fn render(px: &[u8], w: u32, h: u32, o: Orientation) -> (Vec<u8>, u32, u32) {
    let (ow, oh) = o.output_dimensions(w, h);
    let mut out = vec![0u8; px.len()];
    for sy in 0..h {
        for sx in 0..w {
            let (dx, dy) = o.forward_map(sx, sy, w, h);
            let s = ((sy * w + sx) * 4) as usize;
            let d = ((dy * ow + dx) * 4) as usize;
            out[d..d + 4].copy_from_slice(&px[s..s + 4]);
        }
    }
    (out, ow, oh)
}

fn encode(meta: Metadata) -> Vec<u8> {
    let px = image();
    let slice = PixelSlice::new(&px, W, H, W as usize * 4, PixelDescriptor::RGBA8_SRGB).unwrap();
    WebpEncoderConfig::lossless()
        .job()
        .with_metadata_policy(meta, MetadataPolicy::PreserveExact)
        .encoder()
        .unwrap()
        .encode(slice)
        .unwrap()
        .into_vec()
}

fn packed(ps: &PixelSlice<'_>) -> Vec<u8> {
    let rb = ps.width() as usize * ps.descriptor().bytes_per_pixel();
    (0..ps.rows())
        .flat_map(|y| ps.row(y)[..rb].to_vec())
        .collect()
}

fn job(hint: OrientationHint) -> webpx::zencodec::WebpDecodeJob {
    WebpDecoderConfig::new().job().with_orientation(hint)
}

#[derive(Default)]
struct Collect {
    w: u32,
    buf: Vec<u8>,
}

impl DecodeRowSink for Collect {
    fn begin(&mut self, w: u32, h: u32, d: PixelDescriptor) -> Result<(), SinkError> {
        self.w = w;
        self.buf = vec![0; (w * h) as usize * d.bytes_per_pixel()];
        Ok(())
    }
    fn provide_next_buffer(
        &mut self,
        y: u32,
        h: u32,
        w: u32,
        d: PixelDescriptor,
    ) -> Result<PixelSliceMut<'_>, SinkError> {
        let stride = w as usize * d.bytes_per_pixel();
        let range = y as usize * stride..(y + h) as usize * stride;
        Ok(PixelSliceMut::new(&mut self.buf[range], w, h, stride, d).unwrap())
    }
}

const HINTS: [OrientationHint; 4] = [
    OrientationHint::Preserve,
    OrientationHint::Correct,
    OrientationHint::CorrectAndTransform(Orientation::FlipH),
    OrientationHint::ExactTransform(Orientation::Rotate180),
];

/// The transform each hint bakes for a file tagged `tag`.
fn baked(hint: OrientationHint, tag: Orientation) -> Orientation {
    match hint {
        OrientationHint::Correct => tag,
        OrientationHint::CorrectAndTransform(t) => tag.then(t),
        OrientationHint::ExactTransform(t) => t,
        _ => Orientation::Identity,
    }
}

#[test]
fn every_decode_path_honors_every_hint() {
    let src = image();
    for tag in [
        Orientation::Identity,
        Orientation::Rotate90,
        Orientation::Transverse,
    ] {
        let webp = encode(Metadata::none().with_orientation(tag));
        for hint in HINTS {
            let t = baked(hint, tag);
            let (want, ww, wh) = render(&src, W, H, t);
            let reported = if hint == OrientationHint::Preserve {
                tag
            } else {
                Orientation::Identity
            };
            let ctx = format!("tag {tag:?} hint {hint:?}");

            let probe = job(hint).probe(&webp).unwrap();
            assert_eq!(
                (probe.width, probe.height, probe.orientation),
                (ww, wh, reported),
                "{ctx} probe"
            );
            let oi = job(hint).output_info(&webp).unwrap();
            assert_eq!((oi.width, oi.height), (ww, wh), "{ctx} output_info");
            assert_eq!(oi.orientation_applied, t, "{ctx} output_info");

            let out = job(hint)
                .decoder(Cow::Borrowed(&webp), &[PixelDescriptor::RGBA8_SRGB])
                .unwrap()
                .decode()
                .unwrap();
            assert_eq!(
                (out.info().width, out.info().height, out.info().orientation),
                (ww, wh, reported),
                "{ctx} decode info"
            );
            let buf = out.into_buffer();
            assert_eq!(packed(&buf.as_slice()), want, "{ctx} decode pixels");

            #[cfg(feature = "streaming")]
            {
                let mut sd = job(hint)
                    .streaming_decoder(Cow::Borrowed(&webp), &[PixelDescriptor::RGBA8_SRGB])
                    .unwrap();
                assert_eq!(
                    (sd.info().width, sd.info().height),
                    (ww, wh),
                    "{ctx} streaming info"
                );
                let (_, batch) = sd.next_batch().unwrap().unwrap();
                assert_eq!(packed(&batch), want, "{ctx} streaming pixels");
            }

            let mut sink = Collect::default();
            let pi = job(hint)
                .push_decoder(
                    Cow::Borrowed(&webp),
                    &mut sink,
                    &[PixelDescriptor::RGBA8_SRGB],
                )
                .unwrap();
            assert_eq!((pi.width, pi.height), (ww, wh), "{ctx} push info");
            assert_eq!((sink.w, &sink.buf), (ww, &want), "{ctx} push pixels");
        }
    }
}

/// After baking, the EXIF blob's tag says Identity too, so a consumer that
/// reads the blob instead of `ImageInfo::orientation` can't rotate twice.
#[test]
fn baking_rewrites_the_exif_tag() {
    let webp = encode(Metadata::none().with_orientation(Orientation::Rotate270));
    let tag = |hint| {
        let info = job(hint).probe(&webp).unwrap();
        zencodec::helpers::parse_exif_orientation(info.embedded_metadata.exif.as_deref().unwrap())
    };
    assert_eq!(tag(OrientationHint::Preserve), Some(Orientation::Rotate270));
    assert_eq!(tag(OrientationHint::Correct), Some(Orientation::Identity));
}

#[test]
fn encode_writes_orientation_and_keeps_other_exif() {
    // A caller blob without an orientation tag gains one; its other tags stay.
    let mut ex = zencodec::exif::Exif::new(zencodec::exif::TextEncoding::Ascii);
    ex.set_copyright("(C) test");
    let blob = ex.to_bytes();
    let webp = encode(
        Metadata::none()
            .with_exif(blob)
            .with_orientation(Orientation::FlipV),
    );
    let exif = webpx::get_exif(&webp).unwrap().unwrap();
    let parsed = zencodec::exif::Exif::parse(&exif).unwrap();
    assert_eq!(parsed.orientation(), Some(Orientation::FlipV));
    assert_eq!(parsed.copyright().as_deref(), Some("(C) test"));
    assert_eq!(
        webpx::ImageInfo::from_webp(&webp).unwrap().orientation,
        Some(Orientation::FlipV)
    );
    // Identity with no blob writes no EXIF chunk at all.
    assert_eq!(webpx::get_exif(&encode(Metadata::none())).unwrap(), None);
}

#[cfg(feature = "animation")]
#[test]
fn animation_frames_honor_the_hint() {
    use zencodec::decode::AnimationFrameDecoder as _;
    use zencodec::encode::AnimationFrameEncoder as _;
    let a = image();
    let b: Vec<u8> = a.iter().map(|v| v.wrapping_add(77) | 1).collect();
    let mut enc = WebpEncoderConfig::lossless()
        .job()
        .with_metadata_policy(
            Metadata::none().with_orientation(Orientation::Rotate90),
            MetadataPolicy::PreserveExact,
        )
        .animation_frame_encoder()
        .unwrap();
    for px in [&a, &b] {
        let s = PixelSlice::new(px, W, H, W as usize * 4, PixelDescriptor::RGBA8_SRGB).unwrap();
        enc.push_frame(s, 100, None).unwrap();
    }
    let webp = enc.finish(None).unwrap().into_vec();
    for hint in HINTS {
        let t = baked(hint, Orientation::Rotate90);
        let mut dec = job(hint)
            .animation_frame_decoder(Cow::Borrowed(&webp), &[PixelDescriptor::RGBA8_SRGB])
            .unwrap();
        let (ww, wh) = t.output_dimensions(W, H);
        assert_eq!((dec.info().width, dec.info().height), (ww, wh), "{hint:?}");
        for px in [&a, &b] {
            let frame = dec.render_next_frame(None).unwrap().unwrap();
            assert_eq!(packed(frame.pixels()), render(px, W, H, t).0, "{hint:?}");
        }
    }
}
