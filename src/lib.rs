//! # webpx
//!
//! Complete WebP encoding and decoding with ICC profiles, streaming, and animation support.
//!
//! This crate wraps libwebp via FFI to provide:
//! - Static and animated WebP encode/decode
//! - ICC profile embedding and extraction
//! - Streaming/incremental processing
//! - Content-aware optimization presets
//! - RGB, RGBA, and YUV plane support
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! // Simple encode
//! let rgba_data: &[u8] = &[/* RGBA pixel data */];
//! let webp_bytes = webpx::encode_rgba(rgba_data, 640, 480, 85.0)?;
//!
//! // Simple decode
//! let (pixels, width, height) = webpx::decode_rgba(&webp_bytes)?;
//! ```
//!
//! ## Builder API
//!
//! ```rust,ignore
//! use webpx::{Encoder, Preset};
//!
//! let rgba_data: &[u8] = &[/* RGBA pixel data */];
//! let icc_bytes: &[u8] = &[/* ICC profile data */];
//! let webp_bytes = webpx::Encoder::new(rgba_data, 640, 480)
//!     .preset(Preset::Photo)
//!     .quality(85.0)
//!     .icc_profile(icc_bytes)
//!     .encode()?;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

extern crate alloc;

mod config;
mod error;
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

// Re-exports
pub use config::{DecoderConfig, EncoderConfig, Preset};
pub use error::{Error, Result};
pub use types::{ColorMode, ImageInfo, YuvPlanes};

#[cfg(feature = "decode")]
pub use decode::{decode_rgb, decode_rgba, decode_yuv, Decoder};

#[cfg(feature = "encode")]
pub use encode::{encode_lossless, encode_rgb, encode_rgba, Encoder};

#[cfg(feature = "icc")]
pub use mux::{
    embed_exif, embed_icc, embed_xmp, get_exif, get_icc_profile, get_xmp, remove_exif, remove_icc,
    remove_xmp,
};

#[cfg(feature = "streaming")]
pub use streaming::{DecodeStatus, StreamingDecoder, StreamingEncoder};

#[cfg(feature = "animation")]
pub use animation::{AnimationDecoder, AnimationEncoder, Frame};

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
