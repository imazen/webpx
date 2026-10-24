//! `zencodec-testkit` conformance checks against the webpx zencodec layer.
//!
//! Only the checks webpx passes are run here. The ones it does not pass yet
//! (orientation round-trip, capability honesty for the encode paths, the
//! `At<CodecError>` error envelope and truncation categories) need public-API
//! changes; see CHANGELOG and CLAUDE.md "Known Issues".

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
