//! `output_info`, `probe`-time streaming info and push-decode info must
//! describe exactly what decoding produces, under the native crop / scale /
//! flip options and every orientation hint.

#![cfg(feature = "zencodec")]

use std::borrow::Cow;
use webpx::DecoderConfig;
use webpx::zencodec::{WebpDecoderConfig, WebpEncoderConfig};
use zencodec::decode::{Decode as _, DecodeJob as _, DecodeRowSink, DecoderConfig as _, SinkError};
use zencodec::encode::{EncodeJob as _, Encoder as _, EncoderConfig as _};
use zencodec::{Metadata, MetadataPolicy, Orientation, OrientationHint};
use zenpixels::{PixelDescriptor, PixelSlice, PixelSliceMut};

const W: u32 = 8;
const H: u32 = 6;

fn encode(tag: Orientation, alpha: bool) -> Vec<u8> {
    let px: Vec<u8> = (0..W * H)
        .flat_map(|i| {
            [
                i as u8 * 5,
                200 - i as u8,
                i as u8 ^ 0x33,
                if alpha { 128 } else { 255 },
            ]
        })
        .collect();
    let slice = PixelSlice::new(&px, W, H, W as usize * 4, PixelDescriptor::RGBA8_SRGB).unwrap();
    WebpEncoderConfig::lossless()
        .job()
        .with_metadata_policy(
            Metadata::none().with_orientation(tag),
            MetadataPolicy::PreserveExact,
        )
        .encoder()
        .unwrap()
        .encode(slice)
        .unwrap()
        .into_vec()
}

fn configs() -> Vec<(&'static str, DecoderConfig)> {
    vec![
        ("default", DecoderConfig::new()),
        ("crop", DecoderConfig::new().crop(1, 2, 5, 3)),
        ("scale", DecoderConfig::new().scale(11, 4)),
        (
            "crop+scale",
            DecoderConfig::new().crop(2, 0, 4, 6).scale(3, 9),
        ),
        ("flip", DecoderConfig::new().flip(true)),
    ]
}

#[derive(Default)]
struct Sink {
    desc: Option<PixelDescriptor>,
    dims: (u32, u32),
    buf: Vec<u8>,
}

impl DecodeRowSink for Sink {
    fn begin(&mut self, w: u32, h: u32, d: PixelDescriptor) -> Result<(), SinkError> {
        self.desc = Some(d);
        self.dims = (w, h);
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

#[test]
fn reported_geometry_matches_the_decoded_buffer() {
    for alpha in [false, true] {
        let webp = encode(Orientation::Rotate90, alpha);
        for (name, native) in configs() {
            for hint in [OrientationHint::Preserve, OrientationHint::Correct] {
                let ctx = format!("{name} alpha={alpha} {hint:?}");
                let job = || {
                    WebpDecoderConfig::from_native(native.clone())
                        .job()
                        .with_orientation(hint)
                };
                let out = job()
                    .decoder(Cow::Borrowed(&webp), &[])
                    .unwrap()
                    .decode()
                    .unwrap();
                let (dw, dh) = (out.info().width, out.info().height);
                let buf = out.into_buffer();
                assert_eq!((buf.width(), buf.height()), (dw, dh), "{ctx} decode info");

                let oi = job().output_info(&webp).unwrap();
                assert_eq!((oi.width, oi.height), (dw, dh), "{ctx} output_info dims");
                assert_eq!(
                    oi.native_format,
                    buf.descriptor(),
                    "{ctx} output_info format"
                );

                #[cfg(feature = "streaming")]
                {
                    use zencodec::decode::StreamingDecode as _;
                    let mut sd = job().streaming_decoder(Cow::Borrowed(&webp), &[]).unwrap();
                    assert_eq!(
                        (sd.info().width, sd.info().height),
                        (dw, dh),
                        "{ctx} streaming info"
                    );
                    let (_, batch) = sd.next_batch().unwrap().unwrap();
                    assert_eq!(
                        (batch.width(), batch.rows()),
                        (dw, dh),
                        "{ctx} streaming batch"
                    );
                }

                let mut sink = Sink::default();
                let pi = job()
                    .push_decoder(
                        Cow::Borrowed(&webp),
                        &mut sink,
                        &[PixelDescriptor::BGRA8_SRGB],
                    )
                    .unwrap();
                assert_eq!(Some(pi.native_format), sink.desc, "{ctx} push format");
                assert_eq!((pi.width, pi.height), sink.dims, "{ctx} push dims");
            }
        }
    }
}

/// With the native `flip` option the buffer is already upside down; the
/// reported orientation (Preserve) and the baked result (Correct) account for
/// it, so the displayed image is the same as without `flip`.
#[test]
fn flip_option_composes_with_exif_orientation() {
    let webp = encode(Orientation::Rotate90, false);
    let decode = |native: DecoderConfig, hint| {
        let out = WebpDecoderConfig::from_native(native)
            .job()
            .with_orientation(hint)
            .decoder(Cow::Borrowed(&webp), &[PixelDescriptor::RGBA8_SRGB])
            .unwrap()
            .decode()
            .unwrap();
        let o = out.info().orientation;
        let b = out.into_buffer();
        let s = b.as_slice();
        let px: Vec<u8> = (0..s.rows())
            .flat_map(|y| s.row(y)[..s.width() as usize * 4].to_vec())
            .collect();
        (px, s.width(), s.rows(), o)
    };
    let display = |(px, w, h, o): (Vec<u8>, u32, u32, Orientation)| {
        let (ow, oh) = o.output_dimensions(w, h);
        let mut out = vec![0u8; px.len()];
        for sy in 0..h {
            for sx in 0..w {
                let (dx, dy) = o.forward_map(sx, sy, w, h);
                let (s, d) = (((sy * w + sx) * 4) as usize, ((dy * ow + dx) * 4) as usize);
                out[d..d + 4].copy_from_slice(&px[s..s + 4]);
            }
        }
        (out, ow, oh)
    };
    let want = display(decode(DecoderConfig::new(), OrientationHint::Preserve));
    let flipped = decode(DecoderConfig::new().flip(true), OrientationHint::Preserve);
    assert_eq!(flipped.3, Orientation::FlipV.then(Orientation::Rotate90));
    assert_eq!(display(flipped), want);
    let corrected = decode(DecoderConfig::new().flip(true), OrientationHint::Correct);
    assert_eq!(corrected.3, Orientation::Identity);
    assert_eq!(display(corrected), want);
}

/// With no format preference a still decodes to its native layout (RGB8 when
/// opaque, RGBA8 with alpha, as zenwebp does) on every path; an explicit
/// preference wins.
#[test]
fn default_format_follows_the_alpha_flag() {
    for alpha in [false, true] {
        let webp = encode(Orientation::Identity, alpha);
        let native = if alpha {
            PixelDescriptor::RGBA8_SRGB
        } else {
            PixelDescriptor::RGB8_SRGB
        };
        let job = || WebpDecoderConfig::new().job();
        let decoded = |preferred: &[PixelDescriptor]| {
            job()
                .decoder(Cow::Borrowed(&webp), preferred)
                .unwrap()
                .decode()
                .unwrap()
                .into_buffer()
                .descriptor()
        };
        assert_eq!(decoded(&[]), native, "alpha={alpha}");
        assert_eq!(job().output_info(&webp).unwrap().native_format, native);
        let mut sink = Sink::default();
        job()
            .push_decoder(Cow::Borrowed(&webp), &mut sink, &[])
            .unwrap();
        assert_eq!(sink.desc, Some(native));
        #[cfg(feature = "streaming")]
        {
            use zencodec::decode::StreamingDecode as _;
            let mut sd = job().streaming_decoder(Cow::Borrowed(&webp), &[]).unwrap();
            assert_eq!(sd.next_batch().unwrap().unwrap().1.descriptor(), native);
        }
        assert_eq!(
            decoded(&[PixelDescriptor::RGBA8_SRGB]),
            PixelDescriptor::RGBA8_SRGB
        );
        assert_eq!(
            decoded(&[PixelDescriptor::BGRA8_SRGB]),
            PixelDescriptor::BGRA8_SRGB
        );
    }
}
