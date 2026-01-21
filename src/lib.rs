//! # webpx
//!
//! Complete WebP encoding and decoding with ICC profiles, streaming, and animation support.
//!
//! `webpx` provides safe Rust bindings to Google's libwebp library, offering:
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
pub use types::{BitstreamFormat, ColorMode, ImageInfo, WebPData, YuvPlanes, YuvPlanesRef};

// Re-export enough crate types for cooperative cancellation
pub use enough::{Stop, StopReason, Unstoppable};

// Re-export whereat types for error location tracking
pub use whereat::{at, at_crate, At, ResultAtExt};

#[cfg(feature = "decode")]
pub use decode::{
    decode, decode_append, decode_bgr, decode_bgr_into, decode_bgra, decode_bgra_into, decode_into,
    decode_rgb, decode_rgb_into, decode_rgba, decode_rgba_into, decode_to_img, decode_yuv, Decoder,
};
// DecodePixel trait is intentionally not exported - it's a sealed implementation detail.
// Users use concrete types (RGBA8, RGB8, etc.) with decode functions.

#[cfg(feature = "encode")]
pub use encode::Encoder;

#[cfg(feature = "icc")]
pub use mux::{
    embed_exif, embed_icc, embed_xmp, get_exif, get_icc_profile, get_xmp, remove_exif, remove_icc,
    remove_xmp,
};

#[cfg(feature = "streaming")]
pub use streaming::{DecodeStatus, StreamingDecoder, StreamingEncoder};

#[cfg(feature = "animation")]
pub use animation::{AnimationDecoder, AnimationEncoder, AnimationInfo, Frame};

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
