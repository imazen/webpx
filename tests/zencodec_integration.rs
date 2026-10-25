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
fn fidelity_targets_roundtrip() {
    use zencodec::encode::Fidelity;

    // Lossless wins, even from a lossy-configured start.
    let ll = WebpEncoderConfig::lossy().with_fidelity(Fidelity::Lossless);
    assert_eq!(ll.resolved_target_fidelity(), Some(Fidelity::Lossless));
    assert_eq!(
        <WebpEncoderConfig as zencodec::encode::EncoderConfig>::is_lossless(&ll),
        Some(true)
    );

    // Lossy quality even from a lossless start (the with_lossless(false) switch).
    let cq = WebpEncoderConfig::lossless().with_fidelity(Fidelity::codec_quality(70.0));
    assert_eq!(
        cq.resolved_target_fidelity(),
        Some(Fidelity::codec_quality(70.0))
    );
    assert_eq!(
        <WebpEncoderConfig as zencodec::encode::EncoderConfig>::is_lossless(&cq),
        Some(false)
    );

    // SSIM2 + butteraugli map through the measured inverse tables (omni fleet
    // sweep 2026-06-23), reported as codec_quality. Check the clamped extremes
    // (robust, no interpolation) and the mapping direction.
    use zencodec::encode::LossyTarget;
    let q_of = |c: &WebpEncoderConfig| match c.resolved_target_fidelity() {
        Some(Fidelity::Lossy(LossyTarget::CodecSpecificQuality(q))) => q,
        other => panic!("expected codec_quality, got {other:?}"),
    };
    // Excellent target → top of the dial; poor target → bottom.
    assert_eq!(
        q_of(&WebpEncoderConfig::lossy().with_fidelity(Fidelity::ssim2(95.0))),
        95.0
    );
    assert_eq!(
        q_of(&WebpEncoderConfig::lossy().with_fidelity(Fidelity::ssim2(50.0))),
        5.0
    );
    assert_eq!(
        q_of(&WebpEncoderConfig::lossy().with_fidelity(Fidelity::butteraugli(1.0))),
        95.0
    );
    assert_eq!(
        q_of(&WebpEncoderConfig::lossy().with_fidelity(Fidelity::butteraugli(10.0))),
        5.0
    );
    // Monotone interior: higher SSIM2 → higher quality; larger butteraugli
    // distance (worse) → lower quality.
    assert!(
        q_of(&WebpEncoderConfig::lossy().with_fidelity(Fidelity::ssim2(70.0)))
            < q_of(&WebpEncoderConfig::lossy().with_fidelity(Fidelity::ssim2(85.0)))
    );
    assert!(
        q_of(&WebpEncoderConfig::lossy().with_fidelity(Fidelity::butteraugli(5.0)))
            < q_of(&WebpEncoderConfig::lossy().with_fidelity(Fidelity::butteraugli(3.0)))
    );
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

#[cfg(feature = "animation")]
fn three_frame_animation(durations: [u32; 3], loop_count: Option<u32>) -> Vec<u8> {
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
    let mut enc = WebpEncoderConfig::lossless()
        .job()
        .with_loop_count(loop_count)
        .animation_frame_encoder()
        .expect("animation_frame_encoder ctor");
    for (frame, dur) in [&frame_a, &frame_b, &frame_c].into_iter().zip(durations) {
        let slice =
            PixelSlice::new(frame, 8, 8, 8 * 4, PixelDescriptor::RGBA8_SRGB).expect("frame slice");
        enc.push_frame(slice, dur, None).expect("push frame");
    }
    enc.finish(None).expect("anim finish").into_vec()
}

/// `AnimationFrame::frame_index` counts displayed frames and `duration_ms`
/// carries each frame's ANMF duration.
#[cfg(feature = "animation")]
#[test]
fn animation_frames_report_index_and_duration() {
    use zencodec::decode::AnimationFrameDecoder;

    let webp = three_frame_animation([40, 0, 130], None);
    let mut dec = WebpDecoderConfig::new()
        .job()
        .animation_frame_decoder(
            std::borrow::Cow::Borrowed(&webp),
            &[PixelDescriptor::RGBA8_SRGB],
        )
        .expect("animation_frame_decoder ctor");
    let mut seen = Vec::new();
    while let Some(frame) = dec.render_next_frame(None).expect("render frame") {
        seen.push((frame.frame_index(), frame.duration_ms()));
    }
    assert_eq!(seen, [(0, 40), (1, 0), (2, 130)]);
}

/// `with_loop_count` reaches the ANIM chunk as libwebp reads it; `None` keeps
/// WebP's default of 0 (loop forever).
#[cfg(feature = "animation")]
#[test]
fn animation_loop_count_reaches_libwebp() {
    use zencodec::decode::AnimationFrameDecoder;

    for (requested, stored) in [(None, 0), (Some(0), 0), (Some(3), 3), (Some(65535), 65535)] {
        let webp = three_frame_animation([40, 50, 60], requested);
        let lib = webpx::AnimationDecoder::new(&webp).expect("libwebp decoder");
        assert_eq!(lib.info().loop_count, stored, "requested {requested:?}");
        let dec = WebpDecoderConfig::new()
            .job()
            .animation_frame_decoder(
                std::borrow::Cow::Borrowed(&webp),
                &[PixelDescriptor::RGBA8_SRGB],
            )
            .expect("animation_frame_decoder ctor");
        assert_eq!(dec.loop_count(), Some(stored), "requested {requested:?}");
    }
}

/// A loop count that doesn't fit WebP's 16-bit field is an error, not a
/// silent clamp or wrap.
#[cfg(feature = "animation")]
#[test]
fn animation_loop_count_out_of_range_is_rejected() {
    let r = WebpEncoderConfig::lossless()
        .job()
        .with_loop_count(Some(65536))
        .animation_frame_encoder();
    assert!(r.is_err(), "65536 loops cannot be stored");
}

/// Animation support is advertised exactly when the `animation` feature is on.
#[test]
fn capabilities_advertise_animation() {
    let caps = <WebpEncoderConfig as zencodec::encode::EncoderConfig>::capabilities();
    assert_eq!(caps.animation(), cfg!(feature = "animation"));
    let dcaps = <WebpDecoderConfig as zencodec::decode::DecoderConfig>::capabilities();
    assert_eq!(dcaps.animation(), cfg!(feature = "animation"));
}

/// Probe and animation-decoder `ImageInfo` agree with libwebp's VP8X alpha
/// flag, for an opaque and a translucent animation.
#[cfg(feature = "animation")]
#[test]
fn animation_alpha_matches_probe() {
    use zencodec::decode::AnimationFrameDecoder;

    let opaque = three_frame_animation([40, 50, 60], None);
    let translucent = {
        let mut enc = webpx::AnimationEncoder::new(8, 8).unwrap();
        enc.set_lossless(true);
        let mut px = rgba_test_image(8, 8);
        px[3] = 128;
        enc.add_frame_rgba(&px, 0).unwrap();
        px[0] ^= 0xff;
        enc.add_frame_rgba(&px, 40).unwrap();
        enc.finish(80).unwrap()
    };
    for (webp, alpha) in [(opaque, false), (translucent, true)] {
        assert_eq!(webpx::ImageInfo::from_webp(&webp).unwrap().has_alpha, alpha);
        let probe = WebpDecoderConfig::new().job().probe(&webp).expect("probe");
        let dec = WebpDecoderConfig::new()
            .job()
            .animation_frame_decoder(
                std::borrow::Cow::Borrowed(&webp),
                &[PixelDescriptor::RGBA8_SRGB],
            )
            .expect("animation_frame_decoder ctor");
        assert_eq!(probe.has_alpha, alpha, "probe");
        assert_eq!(dec.info().has_alpha, alpha, "animation decoder");
    }
}

/// Frame durations pushed through the zencodec encoder reach the file exactly
/// even when their running total passes `u32::MAX` ms (libwebp's encoder takes
/// wrapping `int` timestamps).
#[cfg(feature = "animation")]
#[test]
fn animation_durations_exact_past_u32_total() {
    use zencodec::encode::AnimationFrameEncoder;

    const MAX_ANMF_MS: u32 = (1 << 24) - 1;
    let n = 257; // 257 × (2^24 - 1) ms > u32::MAX
    let mut enc = WebpEncoderConfig::lossless()
        .job()
        .animation_frame_encoder()
        .expect("animation_frame_encoder ctor");
    for i in 0..n {
        let mut px = rgba_test_image(8, 8);
        px[0] = i as u8;
        px[1] = (i >> 8) as u8;
        let slice =
            PixelSlice::new(&px, 8, 8, 8 * 4, PixelDescriptor::RGBA8_SRGB).expect("frame slice");
        enc.push_frame(slice, MAX_ANMF_MS, None)
            .expect("push frame");
    }
    let webp = enc.finish(None).expect("anim finish").into_vec();
    let frames = webpx::AnimationDecoder::with_options_limits(
        &webp,
        webpx::ColorMode::Rgba,
        false,
        &webpx::Limits::none(),
    )
    .expect("decoder")
    .decode_all()
    .expect("decode_all");
    let wrong: Vec<(usize, u32)> = frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f.duration_ms != MAX_ANMF_MS)
        .map(|(i, f)| (i, f.duration_ms))
        .collect();
    assert_eq!(frames.len(), n);
    assert!(wrong.is_empty(), "(frame, duration): {wrong:?}");
}

/// The decoder advertises ICC/EXIF/XMP extraction; probe, still decode and
/// animation decode must put the file's chunks into `ImageInfo`.
#[cfg(feature = "icc")]
#[test]
fn decode_extracts_metadata_into_image_info() {
    let icc: &[u8] = b"not a real icc profile";
    let exif: &[u8] = b"Exif\0\0MM\0*";
    let xmp: &[u8] = b"<x:xmpmeta/>";
    let embed = |webp: Vec<u8>| {
        let webp = webpx::embed_icc(&webp, icc).unwrap();
        let webp = webpx::embed_exif(&webp, exif).unwrap();
        webpx::embed_xmp(&webp, xmp).unwrap()
    };
    let check = |info: &zencodec::ImageInfo, what: &str| {
        let m = info.metadata();
        assert_eq!(m.icc_profile.as_deref(), Some(icc), "{what}: icc");
        assert_eq!(m.exif.as_deref(), Some(exif), "{what}: exif");
        assert_eq!(m.xmp.as_deref(), Some(xmp), "{what}: xmp");
    };

    let px = rgba_test_image(8, 8);
    let still = embed(
        webpx::Encoder::new_rgba(&px, 8, 8)
            .lossless(true)
            .encode(webpx::Unstoppable)
            .unwrap(),
    );
    check(
        &WebpDecoderConfig::new().job().probe(&still).expect("probe"),
        "still probe",
    );
    let out = WebpDecoderConfig::new()
        .job()
        .decoder(
            std::borrow::Cow::Borrowed(&still),
            &[PixelDescriptor::RGBA8_SRGB],
        )
        .expect("decoder")
        .decode()
        .expect("decode");
    check(out.info(), "still decode");

    #[cfg(feature = "animation")]
    {
        use zencodec::decode::AnimationFrameDecoder;
        let anim = embed(three_frame_animation([40, 50, 60], None));
        check(
            &WebpDecoderConfig::new().job().probe(&anim).expect("probe"),
            "animation probe",
        );
        let dec = WebpDecoderConfig::new()
            .job()
            .animation_frame_decoder(
                std::borrow::Cow::Borrowed(&anim),
                &[PixelDescriptor::RGBA8_SRGB],
            )
            .expect("animation_frame_decoder ctor");
        check(dec.info(), "animation decoder");
    }
}

/// `Decode::decode` reports the file's alpha flag, like `probe`, whatever
/// pixel layout the caller asked for.
#[test]
fn still_decode_alpha_matches_probe() {
    let opaque = rgba_test_image(8, 8);
    let mut translucent = opaque.clone();
    translucent[3] = 128;
    for (px, alpha) in [(opaque, false), (translucent, true)] {
        let webp = webpx::Encoder::new_rgba(&px, 8, 8)
            .lossless(true)
            .encode(webpx::Unstoppable)
            .unwrap();
        let probe = WebpDecoderConfig::new().job().probe(&webp).expect("probe");
        assert_eq!(probe.has_alpha, alpha, "probe");
        for desc in [PixelDescriptor::RGBA8_SRGB, PixelDescriptor::RGB8_SRGB] {
            let out = WebpDecoderConfig::new()
                .job()
                .decoder(std::borrow::Cow::Borrowed(&webp), &[desc])
                .expect("decoder")
                .decode()
                .expect("decode");
            assert_eq!(out.info().has_alpha, alpha, "decode to {desc:?}");
            assert_eq!((out.info().width, out.info().height), (8, 8));
        }
    }
}

/// A one-frame zencodec animation is written as a still, as libwebp and
/// zenwebp do, and keeps its metadata and orientation.
#[cfg(feature = "animation")]
#[test]
fn one_frame_animation_is_written_as_a_still() {
    use zencodec::encode::AnimationFrameEncoder;
    use zencodec::{ImageSequence, Metadata, MetadataPolicy};

    let px = rgba_test_image(8, 8);
    let mut enc = WebpEncoderConfig::lossless()
        .job()
        .with_loop_count(Some(2))
        .with_metadata_policy(
            Metadata::none()
                .with_xmp(b"<x/>".to_vec())
                .with_orientation(zencodec::Orientation::Rotate180),
            MetadataPolicy::PreserveExact,
        )
        .animation_frame_encoder()
        .unwrap();
    let slice = PixelSlice::new(&px, 8, 8, 8 * 4, PixelDescriptor::RGBA8_SRGB).unwrap();
    enc.push_frame(slice, 120, None).unwrap();
    let webp = enc.finish(None).unwrap().into_vec();

    let info = WebpDecoderConfig::new().job().probe(&webp).unwrap();
    assert!(!matches!(info.sequence, ImageSequence::Animation { .. }));
    assert_eq!(info.orientation, zencodec::Orientation::Rotate180);
    assert_eq!(info.embedded_metadata.xmp.as_deref(), Some(&b"<x/>"[..]));
}

/// `with_canvas_size` fixes the frame size: matching frames encode, others
/// are rejected at `push_frame`, and an impossible canvas is rejected up front.
#[cfg(feature = "animation")]
#[test]
fn animation_canvas_size_is_enforced() {
    use zencodec::encode::AnimationFrameEncoder;
    let px = rgba_test_image(8, 8);
    let slice = || PixelSlice::new(&px, 8, 8, 8 * 4, PixelDescriptor::RGBA8_SRGB).unwrap();
    let job = |w, h| {
        WebpEncoderConfig::lossless()
            .job()
            .with_canvas_size(w, h)
            .animation_frame_encoder()
    };
    let mut ok = job(8, 8).unwrap();
    ok.push_frame(slice(), 50, None).unwrap();
    ok.push_frame(slice(), 50, None).unwrap();
    let webp = ok.finish(None).unwrap().into_vec();
    let info = webpx::ImageInfo::from_webp(&webp).unwrap();
    assert_eq!((info.width, info.height), (8, 8));

    let mut wrong = job(16, 8).unwrap();
    assert!(wrong.push_frame(slice(), 50, None).is_err());
    assert!(job(0, 8).is_err());
    assert!(job(8, 16384).is_err());
}

/// `with_start_frame_index(n)` yields frame n first (composited over the
/// earlier frames), numbered n, with its own duration.
#[cfg(feature = "animation")]
#[test]
fn animation_start_frame_index_skips_earlier_frames() {
    use std::borrow::Cow;
    use zencodec::decode::AnimationFrameDecoder;
    let anim = three_frame_animation([40, 0, 130], None);
    let frames = |start| {
        let mut dec = WebpDecoderConfig::new()
            .job()
            .with_start_frame_index(start)
            .animation_frame_decoder(Cow::Borrowed(&anim), &[])
            .unwrap();
        let mut out = Vec::new();
        while let Some(f) = dec.render_next_frame(None).unwrap() {
            let p = f.pixels();
            let rb = p.width() as usize * 4;
            let px: Vec<u8> = (0..p.rows())
                .flat_map(|y| p.row(y)[..rb].to_vec())
                .collect();
            out.push((f.frame_index(), f.duration_ms(), px));
        }
        out
    };
    let all = frames(0);
    assert_eq!(all.len(), 3);
    assert_eq!(frames(1), all[1..].to_vec());
    assert_eq!(frames(2), all[2..].to_vec());
    assert!(frames(3).is_empty());
    assert!(frames(7).is_empty());
}

/// Animation frames use the whole encoder config, not just quality and
/// lossless: changing only the method changes the encoded animation.
#[cfg(feature = "animation")]
#[test]
fn animation_frames_use_the_full_encoder_config() {
    use zencodec::encode::AnimationFrameEncoder;
    let frames: Vec<Vec<u8>> = (0..3u32)
        .map(|s| {
            (0..32 * 32)
                .flat_map(|i| {
                    let (x, y) = (i % 32, i / 32);
                    [
                        (x * 7 + s * 40) as u8,
                        (y * 5) as u8,
                        ((x ^ y) * 3) as u8,
                        255,
                    ]
                })
                .collect()
        })
        .collect();
    let encode = |method| {
        let mut enc = WebpEncoderConfig::lossy()
            .with_method(method)
            .job()
            .animation_frame_encoder()
            .unwrap();
        for f in &frames {
            let s = PixelSlice::new(f, 32, 32, 32 * 4, PixelDescriptor::RGBA8_SRGB).unwrap();
            enc.push_frame(s, 100, None).unwrap();
        }
        enc.finish(None).unwrap().into_vec()
    };
    assert_ne!(
        encode(0),
        encode(6),
        "method had no effect on animation frames"
    );
}

/// zencodec's still decode of an animation returns its first composited
/// frame (as zenwebp does) on every still path; libwebp's own still decoder
/// refuses animations.
#[cfg(feature = "animation")]
#[test]
fn still_decode_of_an_animation_is_its_first_frame() {
    use std::borrow::Cow;
    let anim = three_frame_animation([40, 0, 130], None);
    let first = webpx::AnimationDecoder::new(&anim)
        .unwrap()
        .decode_all()
        .unwrap()
        .remove(0)
        .data;
    let rgba = PixelDescriptor::RGBA8_SRGB;
    let packed = |s: PixelSlice<'_>| -> Vec<u8> {
        let rb = s.width() as usize * s.descriptor().bytes_per_pixel();
        (0..s.rows())
            .flat_map(|y| s.row(y)[..rb].to_vec())
            .collect()
    };
    let out = WebpDecoderConfig::new()
        .job()
        .decoder(Cow::Borrowed(&anim), &[rgba])
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!(packed(out.into_buffer().as_slice()), first);
    #[cfg(feature = "streaming")]
    {
        use zencodec::decode::StreamingDecode as _;
        let mut sd = WebpDecoderConfig::new()
            .job()
            .streaming_decoder(Cow::Borrowed(&anim), &[rgba])
            .unwrap();
        assert_eq!(packed(sd.next_batch().unwrap().unwrap().1), first);
    }
    // RGB drops alpha only.
    let rgb = WebpDecoderConfig::new()
        .job()
        .decoder(Cow::Borrowed(&anim), &[PixelDescriptor::RGB8_SRGB])
        .unwrap()
        .decode()
        .unwrap();
    let want: Vec<u8> = first.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    assert_eq!(packed(rgb.into_buffer().as_slice()), want);
    // Still-decoder options the animation decoder lacks are refused.
    let cropped = WebpDecoderConfig::from_native(webpx::DecoderConfig::new().crop(0, 0, 4, 4))
        .job()
        .decoder(Cow::Borrowed(&anim), &[rgba])
        .unwrap()
        .decode();
    assert!(cropped.is_err());
    // The native API keeps libwebp's refusal.
    assert!(webpx::decode_rgba(&anim).is_err());
}
