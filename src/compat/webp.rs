// Deprecation warnings from this module are intended for *external*
// callers; the internal impl blocks reference deprecated types of their
// own and would otherwise spam the build log.
#![allow(deprecated)]

//! Compatibility shim for the `webp` crate (0.3.x).
//!
//! This module provides an API-compatible interface to ease migration
//! from the `webp` crate to `webpx`.
//!
//! # Deprecated as of 0.2.1
//!
//! The shim swallows error variants ([`Encoder::encode`] returns an
//! empty `WebPMemory` on failure; [`Decoder::decode`] collapses every
//! error to `None`) and does not expose [`crate::Limits`] for
//! untrusted-input decoding.
//!
//! New code should:
//!
//! - **Migrate to [`zenwebp`](https://github.com/imazen/zenwebp)** — a
//!   pure-Rust WebP codec built with `#![forbid(unsafe_code)]`. Equally
//!   or more capable than `webpx` (full feature parity with libwebp,
//!   comparable and sometimes faster runtime performance), and with no
//!   exposure to libwebp's CVE pipeline (most recently CVE-2023-4863,
//!   the actively-exploited 0-click heap overflow) or the FFI
//!   soundness bugs every libwebp wrapper — including this one — has
//!   shipped. Recommended.
//! - Or use the main `webpx` API directly if you need the libwebp
//!   codebase specifically:
//!   - `Encoder::new_rgba(...).quality(q).encode(Unstoppable)` instead of
//!     [`Encoder::new`]
//!   - `Decoder::new(data)?.config(DecoderConfig::new().limits(...)).decode_rgba()`
//!     instead of [`Decoder::decode`]
//!
//! This module will be retained for at least one minor release; all
//! public items carry `#[deprecated]` attributes that surface as
//! compiler warnings on use.
//!
//! # Migration
//!
//! Replace your imports:
//! ```rust,ignore
//! // Before
//! use webp::{Encoder, Decoder, PixelLayout};
//!
//! // After
//! use webpx::compat::webp::{Encoder, Decoder, PixelLayout};
//! ```
//!
//! # Example
//!
//! ```rust
//! use webpx::compat::webp::{Encoder, Decoder, PixelLayout};
//!
//! // Encode
//! let rgba = vec![255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
//! let encoder = Encoder::new(&rgba, PixelLayout::Rgba, 2, 2);
//! let webp_data = encoder.encode(85.0);
//!
//! // Decode
//! let decoder = Decoder::new(&webp_data);
//! if let Some(image) = decoder.decode() {
//!     assert_eq!(image.width(), 2);
//!     assert_eq!(image.height(), 2);
//! }
//! ```

use alloc::vec::Vec;
use core::ops::Deref;

/// Pixel layout for raw image data.
#[deprecated(
    since = "0.2.1",
    note = "use `webpx::Encoder` / `webpx::Decoder` directly; see module docs"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelLayout {
    /// RGB (3 bytes per pixel).
    Rgb,
    /// RGBA (4 bytes per pixel).
    Rgba,
}

impl PixelLayout {
    /// Bytes per pixel for this layout.
    pub fn bytes_per_pixel(&self) -> u8 {
        match self {
            PixelLayout::Rgb => 3,
            PixelLayout::Rgba => 4,
        }
    }
}

/// Owned WebP memory buffer.
///
/// Provides `Deref<Target = [u8]>` for compatibility with `webp::WebPMemory`.
#[deprecated(
    since = "0.2.1",
    note = "use `webpx::WebPData` or `Vec<u8>` from the main API"
)]
#[derive(Debug)]
pub struct WebPMemory(Vec<u8>);

impl WebPMemory {
    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the length of the buffer.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Deref for WebPMemory {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[u8]> for WebPMemory {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Decoded WebP image.
#[deprecated(
    since = "0.2.1",
    note = "use `webpx::Decoder::new(data)?.decode_rgba()` etc."
)]
#[derive(Debug)]
pub struct WebPImage {
    data: Vec<u8>,
    layout: PixelLayout,
    width: u32,
    height: u32,
}

impl WebPImage {
    /// Get the pixel data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get the pixel layout.
    pub fn layout(&self) -> PixelLayout {
        self.layout
    }

    /// Get image width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get image height.
    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Bitstream features extracted from WebP data.
#[deprecated(since = "0.2.1", note = "use `webpx::ImageInfo::from_webp` directly")]
#[derive(Debug)]
pub struct BitstreamFeatures {
    width: u32,
    height: u32,
    has_alpha: bool,
    has_animation: bool,
}

impl BitstreamFeatures {
    /// Extract features from WebP data.
    pub fn new(data: &[u8]) -> Option<Self> {
        crate::ImageInfo::from_webp(data).ok().map(|info| Self {
            width: info.width,
            height: info.height,
            has_alpha: info.has_alpha,
            has_animation: info.has_animation,
        })
    }

    /// Get image width.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get image height.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Check if the image has an alpha channel.
    pub fn has_alpha(&self) -> bool {
        self.has_alpha
    }

    /// Check if the image is animated.
    pub fn has_animation(&self) -> bool {
        self.has_animation
    }
}

/// WebP encoder (compatible with `webp::Encoder`).
#[deprecated(
    since = "0.2.1",
    note = "use `webpx::Encoder` directly; the main API surfaces errors instead of returning an empty buffer"
)]
#[cfg(feature = "encode")]
pub struct Encoder<'a> {
    image: &'a [u8],
    layout: PixelLayout,
    width: u32,
    height: u32,
}

#[cfg(feature = "encode")]
impl<'a> Encoder<'a> {
    /// Create a new encoder from raw image data.
    pub fn new(image: &'a [u8], layout: PixelLayout, width: u32, height: u32) -> Self {
        Self {
            image,
            layout,
            width,
            height,
        }
    }

    /// Create an encoder from RGB data.
    pub fn from_rgb(image: &'a [u8], width: u32, height: u32) -> Self {
        Self::new(image, PixelLayout::Rgb, width, height)
    }

    /// Create an encoder from RGBA data.
    pub fn from_rgba(image: &'a [u8], width: u32, height: u32) -> Self {
        Self::new(image, PixelLayout::Rgba, width, height)
    }

    /// Encode with the given quality (0-100).
    pub fn encode(&self, quality: f32) -> WebPMemory {
        self.encode_simple(false, quality)
            .unwrap_or_else(|_| WebPMemory(Vec::new()))
    }

    /// Encode losslessly.
    pub fn encode_lossless(&self) -> WebPMemory {
        self.encode_simple(true, 75.0)
            .unwrap_or_else(|_| WebPMemory(Vec::new()))
    }

    /// Encode with simple options.
    pub fn encode_simple(&self, lossless: bool, quality: f32) -> crate::Result<WebPMemory> {
        use crate::Unstoppable;

        let config = crate::EncoderConfig::new()
            .quality(quality)
            .lossless(lossless);

        let data = match self.layout {
            PixelLayout::Rgba => {
                config.encode_rgba(self.image, self.width, self.height, Unstoppable)?
            }
            PixelLayout::Rgb => {
                config.encode_rgb(self.image, self.width, self.height, Unstoppable)?
            }
        };

        Ok(WebPMemory(data))
    }
}

/// WebP decoder (compatible with `webp::Decoder`).
#[deprecated(
    since = "0.2.1",
    note = "use `webpx::Decoder::new(data)?` directly; the main API exposes Limits and full error variants"
)]
#[cfg(feature = "decode")]
pub struct Decoder<'a> {
    data: &'a [u8],
}

#[cfg(feature = "decode")]
impl<'a> Decoder<'a> {
    /// Create a new decoder from WebP data.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Decode the WebP data.
    ///
    /// Returns `None` if decoding fails or the image is animated.
    pub fn decode(&self) -> Option<WebPImage> {
        let features = BitstreamFeatures::new(self.data)?;

        // webp crate doesn't support animation
        if features.has_animation() {
            return None;
        }

        let (data, width, height) = if features.has_alpha() {
            crate::decode_rgba(self.data).ok()?
        } else {
            crate::decode_rgb(self.data).ok()?
        };

        let layout = if features.has_alpha() {
            PixelLayout::Rgba
        } else {
            PixelLayout::Rgb
        };

        Some(WebPImage {
            data,
            layout,
            width,
            height,
        })
    }
}

#[cfg(all(test, feature = "decode", feature = "encode"))]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        let encoder = Encoder::from_rgba(&rgba, 2, 2);
        let webp = encoder.encode_lossless();

        assert!(!webp.is_empty());

        let decoder = Decoder::new(&webp);
        let image = decoder.decode().expect("decode failed");
        assert_eq!(image.width(), 2);
        assert_eq!(image.height(), 2);
        // Note: The decoded layout depends on whether libwebp reports has_alpha
        // Lossless with all-opaque alpha may not set the flag
        assert!(image.layout() == PixelLayout::Rgba || image.layout() == PixelLayout::Rgb);
    }

    #[test]
    fn test_bitstream_features() {
        let rgba = vec![0u8; 4 * 4 * 4];
        let encoder = Encoder::from_rgba(&rgba, 4, 4);
        let webp = encoder.encode(85.0);

        let features = BitstreamFeatures::new(&webp).expect("features");
        assert_eq!(features.width(), 4);
        assert_eq!(features.height(), 4);
        assert!(!features.has_animation());
    }
}
