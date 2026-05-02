//! Integration tests for the `zencodec` trait surface.
//!
//! Verifies the encoder + decoder paths round-trip cleanly through the
//! shared `zencodec` traits. The same call shape works against
//! `zenwebp::zencodec::*` — see `examples/zencodec_swap.rs`.

#![cfg(feature = "zencodec")]

use webpx::zencodec::{WebpDecoderConfig, WebpEncoderConfig};
use zencodec::ImageFormat;
use zencodec::decode::{Decode, DecodeJob as _, DecoderConfig as _};
use zencodec::encode::{EncodeJob as _, Encoder as _, EncoderConfig as _};
use zenpixels::{PixelDescriptor, PixelSlice};

fn rgba_test_image(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&[
                (x.wrapping_mul(31) + 7) as u8,
                (y.wrapping_mul(53) + 17) as u8,
                ((x ^ y).wrapping_mul(11)) as u8,
                255,
            ]);
        }
    }
    data
}

#[test]
fn encoder_roundtrip_via_zencodec_traits() {
    let pixels = rgba_test_image(32, 24);

    // Encode through zencodec trait surface.
    let cfg = WebpEncoderConfig::lossy().with_generic_quality(85.0);
    assert_eq!(
        <WebpEncoderConfig as zencodec::encode::EncoderConfig>::format(),
        ImageFormat::WebP
    );
    assert_eq!(
        <WebpEncoderConfig as zencodec::encode::EncoderConfig>::generic_quality(&cfg),
        Some(85.0)
    );

    let job = cfg.job();
    let encoder = job.encoder().expect("encoder construction");

    let slice =
        PixelSlice::new(&pixels, 32, 24, 32 * 4, PixelDescriptor::RGBA8_SRGB).expect("pixel slice");
    let out = encoder.encode(slice).expect("encode through zencodec");
    assert_eq!(out.format(), ImageFormat::WebP);
    let webp = out.into_vec();
    assert!(!webp.is_empty(), "encode produced empty output");

    // Decode through zencodec trait surface.
    let dcfg = WebpDecoderConfig::new();
    assert_eq!(
        <WebpDecoderConfig as zencodec::decode::DecoderConfig>::formats(),
        &[ImageFormat::WebP],
    );

    let djob = dcfg.job();
    let info = djob.probe(&webp).expect("probe");
    assert_eq!(info.width, 32);
    assert_eq!(info.height, 24);
    assert_eq!(info.format, ImageFormat::WebP);

    let decoder = djob
        .decoder(
            std::borrow::Cow::Borrowed(&webp),
            &[PixelDescriptor::RGBA8_SRGB],
        )
        .expect("decoder construction");
    let result = decoder.decode().expect("decode through zencodec");
    assert_eq!(result.width(), 32);
    assert_eq!(result.height(), 24);
    assert!(result.has_alpha());
}

#[test]
fn lossless_roundtrip_preserves_pixels() {
    let pixels = rgba_test_image(8, 8);

    let cfg = WebpEncoderConfig::lossless();
    assert_eq!(
        <WebpEncoderConfig as zencodec::encode::EncoderConfig>::is_lossless(&cfg),
        Some(true)
    );

    let job = cfg.job();
    let encoder = job.encoder().expect("encoder");
    let slice = PixelSlice::new(&pixels, 8, 8, 8 * 4, PixelDescriptor::RGBA8_SRGB).expect("slice");
    let webp = encoder.encode(slice).expect("encode").into_vec();

    let djob = WebpDecoderConfig::new().job();
    let decoder = djob
        .decoder(
            std::borrow::Cow::Borrowed(&webp),
            &[PixelDescriptor::RGBA8_SRGB],
        )
        .expect("decoder");
    let out = decoder.decode().expect("decode");
    let buf = out.into_buffer();
    let decoded = buf.as_slice().as_strided_bytes();

    // Lossless must preserve every byte.
    assert_eq!(decoded.len(), pixels.len());
    assert_eq!(decoded, pixels.as_slice());
}

#[test]
fn limits_propagate_through_zencodec_traits() {
    use zencodec::ResourceLimits;

    let pixels = rgba_test_image(64, 64); // 4096 pixels
    let cfg = WebpEncoderConfig::lossy();
    let webp = cfg
        .job()
        .encoder()
        .expect("encoder")
        .encode(PixelSlice::new(&pixels, 64, 64, 64 * 4, PixelDescriptor::RGBA8_SRGB).unwrap())
        .expect("encode")
        .into_vec();

    // Tight pixel cap on the decoder must reject.
    let limits = ResourceLimits::none().with_max_pixels(1024);
    let djob = WebpDecoderConfig::new().job().with_limits(limits);
    let decoder = djob
        .decoder(
            std::borrow::Cow::Borrowed(&webp),
            &[PixelDescriptor::RGBA8_SRGB],
        )
        .expect("decoder ctor");
    let r = decoder.decode();
    assert!(
        r.is_err(),
        "max_pixels=1024 must reject a 64x64 (4096-pixel) image"
    );
}

#[test]
fn capabilities_advertise_webp_support() {
    let caps = <WebpEncoderConfig as zencodec::encode::EncoderConfig>::capabilities();
    assert!(caps.lossy(), "advertise lossy support");
    assert!(caps.lossless(), "advertise lossless support");
    assert!(caps.icc(), "advertise ICC embedding");
    assert!(caps.native_alpha(), "advertise native alpha");
    assert_eq!(caps.effort_range(), Some([0i32, 6]));

    let dcaps = <WebpDecoderConfig as zencodec::decode::DecoderConfig>::capabilities();
    assert!(dcaps.icc());
    assert!(dcaps.enforces_max_pixels());
}

#[cfg(feature = "animation")]
#[test]
fn animation_roundtrip_via_zencodec_traits() {
    use zencodec::decode::AnimationFrameDecoder;
    use zencodec::encode::AnimationFrameEncoder;

    let frame_a = rgba_test_image(8, 8);
    let mut frame_b = frame_a.clone();
    for px in frame_b.chunks_mut(4) {
        px[0] = 255 - px[0];
    }
    let mut frame_c = frame_a.clone();
    for px in frame_c.chunks_mut(4) {
        px[1] = 255 - px[1];
    }

    let cfg = WebpEncoderConfig::lossy().with_generic_quality(70.0);
    let mut anim_enc = cfg
        .job()
        .with_canvas_size(8, 8)
        .animation_frame_encoder()
        .expect("animation_frame_encoder ctor");

    for (frame, dur) in [(&frame_a, 100u32), (&frame_b, 100), (&frame_c, 150)] {
        let slice =
            PixelSlice::new(frame, 8, 8, 8 * 4, PixelDescriptor::RGBA8_SRGB).expect("frame slice");
        anim_enc.push_frame(slice, dur, None).expect("push frame");
    }
    let webp = anim_enc.finish(None).expect("anim finish").into_vec();
    assert!(!webp.is_empty());

    // Decode through the trait surface.
    let dcfg = WebpDecoderConfig::new();
    let djob = dcfg.job();
    let info = djob.probe(&webp).expect("probe");
    assert_eq!(info.width, 8);
    assert_eq!(info.height, 8);
    assert!(info.is_animation());

    let mut dec = WebpDecoderConfig::new()
        .job()
        .animation_frame_decoder(
            std::borrow::Cow::Borrowed(&webp),
            &[PixelDescriptor::RGBA8_SRGB],
        )
        .expect("animation_frame_decoder ctor");
    assert_eq!(dec.frame_count(), Some(3));

    let mut frames_seen = 0;
    while let Some(frame) = dec.render_next_frame(None).expect("render frame") {
        assert_eq!(frame.pixels().width(), 8);
        assert_eq!(frame.pixels().rows(), 8);
        frames_seen += 1;
    }
    assert_eq!(frames_seen, 3);
}

#[cfg(feature = "streaming")]
#[test]
fn streaming_decode_via_zencodec_traits() {
    use zencodec::decode::StreamingDecode;

    let pixels = rgba_test_image(16, 16);
    let cfg = WebpEncoderConfig::lossless();
    let webp = cfg
        .job()
        .encoder()
        .expect("encoder")
        .encode(PixelSlice::new(&pixels, 16, 16, 16 * 4, PixelDescriptor::RGBA8_SRGB).unwrap())
        .expect("encode")
        .into_vec();

    let mut sdec = WebpDecoderConfig::new()
        .job()
        .streaming_decoder(
            std::borrow::Cow::Borrowed(&webp),
            &[PixelDescriptor::RGBA8_SRGB],
        )
        .expect("streaming decoder");
    assert_eq!(sdec.info().width, 16);
    assert_eq!(sdec.info().height, 16);

    let mut total_rows = 0u32;
    while let Some((row_offset, slice)) = sdec.next_batch().expect("next_batch") {
        assert_eq!(row_offset, total_rows);
        assert_eq!(slice.width(), 16);
        // Lossless: bytes must match the source for this strip.
        let strip_rows = slice.rows();
        let stride = slice.stride();
        let bytes = slice.as_strided_bytes();
        for y in 0..strip_rows as usize {
            let want_y = total_rows as usize + y;
            let want = &pixels[want_y * 16 * 4..(want_y + 1) * 16 * 4];
            let got = &bytes[y * stride..y * stride + 16 * 4];
            assert_eq!(got, want, "row {} mismatch", want_y);
        }
        total_rows += strip_rows;
    }
    assert_eq!(total_rows, 16);
}

#[test]
fn limits_round_trip_between_webpx_and_zencodec() {
    // webpx::Limits ↔ zencodec::ResourceLimits should round-trip every
    // shared field.
    use webpx::Limits;
    use zencodec::ResourceLimits;

    let original = Limits::default();
    let zen: ResourceLimits = original.into();
    assert_eq!(zen.max_pixels, original.max_pixels);
    assert_eq!(zen.max_total_pixels, original.max_total_pixels);
    assert_eq!(zen.max_width, original.max_width);
    assert_eq!(zen.max_height, original.max_height);
    assert_eq!(zen.max_input_bytes, original.max_input_bytes);
    assert_eq!(zen.max_frames, original.max_frames);
    assert_eq!(zen.max_animation_ms, original.max_animation_ms);
    assert_eq!(zen.max_output_bytes, original.max_output_bytes);

    let back: Limits = zen.into();
    assert_eq!(back.max_pixels, original.max_pixels);
    assert_eq!(back.max_width, original.max_width);
    // max_metadata_bytes is webpx-only, drops on the round trip.
    assert_eq!(back.max_metadata_bytes, None);
}
