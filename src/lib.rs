//! # webpx
//!
//! Complete WebP encoding and decoding with ICC profiles, streaming, and animation support.
//!
//! `webpx` provides ergonomic Rust bindings to Google's libwebp library
//! (FFI to the upstream C codebase).
//!
//! **For new projects, use [`zenwebp`](https://github.com/imazen/zenwebp)
//! instead.** `zenwebp` is a pure-Rust port of libwebp — equally or
//! more capable than webpx on every axis (full feature parity: lossy /
//! lossless, animation, alpha, ICC / EXIF / XMP, streaming, presets,
//! limits), with native `wasm32-unknown-unknown` support that webpx
//! cannot offer (libwebp requires emscripten), and
//! `#![forbid(unsafe_code)]` so the whole class of FFI / memory-safety
//! bug is structurally impossible.
//!
//! Performance and compression are essentially a wash: libwebp can be
//! up to ~35 % faster on specific photos but up to ~2.5× slower on
//! others; encoded-size difference is at most ~0.02 % — noise.
//!
//! The security argument is concrete: libwebp has a documented history
//! of high-severity vulnerabilities (most notably CVE-2023-4863, an
//! actively-exploited 0-click heap overflow patched out of band in
//! every major browser), and every libwebp wrapper that has been
//! audited — webpx included — has shipped soundness bugs (versions
//! 0.1.0–0.1.4 are yanked; 0.2.0 + 0.2.1 fixed multiple FFI issues
//! found across two parallel audits). `zenwebp`'s pure-safe-Rust
//! implementation does not have that exposure.
//!
//! `webpx` is maintained for users whose application already links
//! libwebp through another path (existing C / C++ code, system
//! package), who specifically need libwebp's MIPS DSP code paths, or
//! who have benchmarked their content and confirmed libwebp wins.
//!
//! webpx offers:
//!
//! - **Static Images**: Encode/decode RGB, RGBA, and YUV formats with lossy or lossless compression
//! - **Animations**: Create and decode animated WebP files frame-by-frame or in batch
//! - **Metadata**: Embed and extract ICC color profiles, EXIF, and XMP data
//! - **Streaming**: Incremental decoding for large files or network streams
//! - **Presets**: Content-aware optimization (Photo, Picture, Drawing, Icon, Text)
//! - **Advanced Config**: Full control over compression parameters, alpha handling, and more
//!
//! ## Quick Start
//!
//! ```rust
//! use webpx::{Encoder, Unstoppable};
//!
//! // Create a small 2x2 RGBA image (red, green, blue, white)
//! let rgba_data: Vec<u8> = vec![
//!     255, 0, 0, 255,    // red
//!     0, 255, 0, 255,    // green
//!     0, 0, 255, 255,    // blue
//!     255, 255, 255, 255 // white
//! ];
//!
//! // Encode to WebP (lossy, quality 85)
//! let webp_bytes = Encoder::new_rgba(&rgba_data, 2, 2)
//!     .quality(85.0)
//!     .encode(Unstoppable)?;
//!
//! // Decode back to RGBA
//! let (pixels, width, height) = webpx::decode_rgba(&webp_bytes)?;
//! assert_eq!((width, height), (2, 2));
//! # Ok::<(), webpx::At<webpx::Error>>(())
//! ```
//!
//! ## Decoding untrusted input
//!
//! Apply a [`Limits`] policy any time the bitstream comes from an
//! untrusted source. Without one, libwebp's intrinsic 16383×16383 cap
//! still applies but allocations up to ~1 GiB at 4 bpp are reachable.
//! With limits set, oversized inputs are rejected at parse time.
//!
//! ```rust,no_run
//! use webpx::{Decoder, DecoderConfig, Limits};
//!
//! let limits = Limits::none()
//!     .with_max_pixels(64 * 1024 * 1024)
//!     .with_max_total_pixels(256 * 1024 * 1024)
//!     .with_max_frames(1024)
//!     .with_max_input_bytes(64 * 1024 * 1024)
//!     .with_max_metadata_bytes(4 * 1024 * 1024);
//!
//! let webp_data: &[u8] = &[];
//! let img = Decoder::new(webp_data)?
//!     .config(DecoderConfig::new().limits(limits))
//!     .decode_rgba()?;
//! # Ok::<(), webpx::At<webpx::Error>>(())
//! ```
//!
//! See the [`Limits`] type for the per-field enforcement matrix and the
//! `decode_with_limits` example for end-to-end usage across the static
//! decoder, animation decoder, and metadata extraction paths.
//!
//! ## Builder API
//!
//! For more control, use the builder pattern:
//!
//! ```rust,no_run
//! use webpx::{Encoder, Preset, Unstoppable};
//!
//! let rgba_data: &[u8] = &[0u8; 640 * 480 * 4]; // placeholder
//! let webp_bytes = Encoder::new_rgba(rgba_data, 640, 480)
//!     .preset(Preset::Photo)  // Optimize for photos
//!     .quality(85.0)          // 0-100, higher = better quality
//!     .method(4)              // 0-6, higher = slower but smaller
//!     .encode(Unstoppable)?;
//! # Ok::<(), webpx::At<webpx::Error>>(())
//! ```
//!
//! ## Feature Flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `decode` | Yes | Decoding support |
//! | `encode` | Yes | Encoding support |
//! | `std` | Yes | Standard library (disable for no_std) |
//! | `animation` | No | Animated WebP support |
//! | `icc` | No | ICC/EXIF/XMP metadata |
//! | `streaming` | No | Incremental processing |
//!
//! ## no_std Support
//!
//! This crate supports `no_std` with `alloc`. Disable the `std` feature:
//!
//! ```toml
//! webpx = { version = "0.1", default-features = false, features = ["decode", "encode"] }
//! ```
//!
//! ## Compatibility Shims
//!
//! For easy migration from other WebP crates, see [`compat::webp`] and [`compat::webp_animation`].
//!
//! ## Error Handling
//!
//! All functions return `Result<T, At<Error>>` where [`At`] provides lightweight error
//! location tracking. See the [error module](crate::error) documentation for how to:
//! - Propagate traces with `.at()`
//! - Add your crate's info to traces with `at_crate!`
//! - Convert to plain errors with `.into_inner()`

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

extern crate alloc;

// Define crate info for whereat traces (enables GitHub links)
whereat::define_at_crate_info!();

mod config;
pub mod error;
mod ffi;
mod limits;
mod types;

#[cfg(feature = "decode")]
mod decode;

#[cfg(feature = "encode")]
mod encode;

#[cfg(feature = "icc")]
mod mux;

#[cfg(feature = "streaming")]
mod streaming;

#[cfg(feature = "animation")]
mod animation;

pub mod heuristics;

pub mod compat;

// Re-exports
pub use config::{AlphaFilter, DecoderConfig, EncodeStats, EncoderConfig, ImageHint, Preset};
pub use error::{DecodingError, EncodingError, Error, MuxError, Result};
pub use limits::{LimitExceeded, Limits};
pub use types::{BitstreamFormat, ColorMode, ImageInfo, WebPData, YuvPlanes, YuvPlanesRef};

// Re-export enough crate types for cooperative cancellation
pub use enough::{Stop, StopReason, Unstoppable};

// Re-export whereat types for error location tracking
pub use whereat::{At, ResultAtExt, at, at_crate};

#[cfg(feature = "decode")]
pub use decode::{
    Decoder, decode, decode_append, decode_bgr, decode_bgr_into, decode_bgra, decode_bgra_into,
    decode_into, decode_rgb, decode_rgb_into, decode_rgba, decode_rgba_into, decode_to_img,
    decode_yuv,
};
// DecodePixel trait is intentionally not exported - it's a sealed implementation detail.
// Users use concrete types (RGBA8, RGB8, etc.) with decode functions.

#[cfg(feature = "encode")]
pub use encode::Encoder;

#[cfg(feature = "icc")]
pub use mux::{
    embed_exif, embed_icc, embed_xmp, get_exif, get_exif_with_limits, get_icc_profile,
    get_icc_profile_with_limits, get_xmp, get_xmp_with_limits, remove_exif, remove_icc, remove_xmp,
};

#[cfg(feature = "streaming")]
pub use streaming::DecodeStatus;
#[cfg(all(feature = "streaming", feature = "decode"))]
pub use streaming::StreamingDecoder;
#[cfg(all(feature = "streaming", feature = "encode"))]
pub use streaming::StreamingEncoder;

#[cfg(feature = "animation")]
pub use animation::AnimationDecoder;
#[cfg(all(feature = "animation", feature = "encode"))]
pub use animation::AnimationEncoder;
#[cfg(feature = "animation")]
pub use animation::{AnimationInfo, Frame};

/// Library version information.
pub fn version() -> (u32, u32, u32) {
    let v = unsafe { libwebp_sys::WebPGetDecoderVersion() } as u32;
    ((v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let (major, minor, patch) = version();
        assert!(
            major >= 1,
            "Expected libwebp 1.x, got {}.{}.{}",
            major,
            minor,
            patch
        );
    }
}
