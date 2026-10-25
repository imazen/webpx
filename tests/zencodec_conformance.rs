//! `zencodec-testkit` conformance checks against the webpx zencodec layer.
//!
//! Every check the testkit offers runs here, including the opt-in error
//! envelope and truncation-series checks.

#![cfg(feature = "zencodec")]

use webpx::zencodec::{WebpDecoderConfig, WebpEncoderConfig};
use zencodec_testkit::{self as tk, TestImage};

fn still() -> TestImage {
    TestImage::rgba8_gradient(40, 24)
}

#[cfg(feature = "animation")]
fn frames() -> [TestImage; 3] {
    [0, 60, 120].map(|seed| TestImage::rgba8_gradient_seeded(24, 16, seed))
}

fn ok(res: tk::Conformance) {
    if let Err(f) = res {
        panic!("{f:?}");
    }
}

#[test]
fn lossless_pixel_roundtrip() {
    ok(tk::check_pixel_roundtrip(
        WebpEncoderConfig::lossless(),
        WebpDecoderConfig::new(),
        &still(),
    ));
}

#[test]
fn lossless_cross_path_pixel_equivalence() {
    ok(tk::check_cross_path_pixel_equivalence(
        WebpEncoderConfig::lossless(),
        WebpDecoderConfig::new(),
        &still(),
    ));
}

#[test]
fn metadata_no_leak() {
    ok(tk::check_metadata_no_leak(
        WebpEncoderConfig::lossless(),
        WebpDecoderConfig::new(),
        &still(),
    ));
}

/// A lossless config must give lossless animation frames: the animation
/// encoder must not let libwebp pick lossy per frame (`allow_mixed`).
#[cfg(feature = "animation")]
#[test]
fn lossless_animation_cross_path_equivalence() {
    ok(tk::check_animation_cross_path_equivalence(
        WebpEncoderConfig::lossless(),
        WebpDecoderConfig::new(),
        &frames(),
    ));
}

/// The streaming decoder works (buffered, one batch), so the capabilities
/// must say so; with the flag set, the cross-path check above also compares
/// the streaming path's pixels.
#[cfg(feature = "streaming")]
#[test]
fn decode_capabilities_advertise_streaming() {
    use zencodec::decode::DecoderConfig as _;
    assert!(WebpDecoderConfig::capabilities().streaming());
}

/// Every declared capability works, and every undeclared optional path fails
/// with an `UnsupportedOperation` the caller can find in the error chain.
#[test]
fn capability_honesty() {
    ok(tk::check_capability_honesty(
        WebpEncoderConfig::lossless(),
        WebpDecoderConfig::new(),
        &still(),
    ));
}

/// webpx errors carry a zencodec category and the codec name.
#[test]
fn errors_are_categorized() {
    use std::borrow::Cow;
    use zencodec::decode::{DecodeJob as _, DecoderConfig as _};
    use zencodec::{CategorizedError as _, ErrorCategory, ImageError};
    let err = match WebpDecoderConfig::new()
        .job()
        .decoder(Cow::Borrowed(b"RIFF\x10\0\0\0WEBPVP8 garbage!!"), &[])
        .and_then(zencodec::decode::Decode::decode)
    {
        Err(e) => e,
        Ok(_) => panic!("garbage decoded"),
    };
    assert_eq!(err.codec_name(), Some("webpx"));
    assert_eq!(err.category(), ErrorCategory::Image(ImageError::Malformed));
    // The native error is the envelope's detail (the CHANGELOG migration).
    let native = err
        .error()
        .detail()
        .and_then(|d| d.downcast_ref::<webpx::Error>());
    assert!(
        matches!(
            native,
            Some(webpx::Error::DecodeFailed(_) | webpx::Error::InvalidWebP)
        ),
        "{native:?}"
    );
}

/// Under every orientation-keeping metadata policy the displayed image
/// survives an encode/decode round trip, without loss or double rotation.
#[test]
fn orientation_roundtrip() {
    ok(tk::check_orientation_roundtrip(
        WebpEncoderConfig::lossless(),
        WebpDecoderConfig::new(),
        &still(),
    ));
}

/// Errors cross `dyn` dispatch as `At<CodecError>`: the category and the codec
/// name survive erasure.
#[test]
fn decode_error_envelope() {
    ok(tk::check_decode_error_envelope(
        WebpDecoderConfig::new(),
        b"RIFF\x10\0\0\0WEBPVP8 garbage!!",
    ));
}

/// Truncated lossless, lossy and animated files fail as image-data errors (never
/// internal, resource, I/O or caller faults) and never panic.
#[test]
fn decode_truncation_series() {
    use zencodec::encode::{EncodeJob as _, Encoder as _, EncoderConfig as _};
    let img = still();
    for enc in [WebpEncoderConfig::lossless(), WebpEncoderConfig::lossy()] {
        let bytes = enc.job().encoder().unwrap().encode(img.as_slice()).unwrap();
        ok(tk::check_decode_truncation_series(
            WebpDecoderConfig::new(),
            bytes.data(),
        ));
    }
    #[cfg(feature = "animation")]
    {
        use zencodec::encode::AnimationFrameEncoder as _;
        let mut enc = WebpEncoderConfig::lossless()
            .job()
            .animation_frame_encoder()
            .unwrap();
        for f in frames() {
            enc.push_frame(f.as_slice(), 50, None).unwrap();
        }
        let anim = enc.finish(None).unwrap();
        ok(tk::check_decode_truncation_series(
            WebpDecoderConfig::new(),
            anim.data(),
        ));
    }
}
