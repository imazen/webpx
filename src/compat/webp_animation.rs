//! Compatibility shim for the `webp-animation` crate (0.9.x).
//!
//! This module provides an API-compatible interface to ease migration
//! from the `webp-animation` crate to `webpx`.
//!
//! # Migration
//!
//! Replace your imports:
//! ```rust,ignore
//! // Before
//! use webp_animation::{Encoder, Decoder, ColorMode};
//!
//! // After
//! use webpx::compat::webp_animation::{Encoder, Decoder, ColorMode};
//! ```
//!
//! # Key Differences
//!
//! - Uses `finalize()` to match webp-animation API (webpx uses `finish()`)
//! - Uses tuple dimensions `(width, height)` like webp-animation
//! - Decoder implements `IntoIterator` for frame iteration

use alloc::vec::Vec;
use core::ops::Deref;

/// Color mode for decoded frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// RGB (3 bytes per pixel).
    Rgb,
    /// RGBA (4 bytes per pixel).
    #[default]
    Rgba,
    /// BGRA (4 bytes per pixel).
    Bgra,
    /// BGR (3 bytes per pixel).
    Bgr,
}

impl ColorMode {
    /// Bytes per pixel for this color mode.
    pub fn size(&self) -> usize {
        match self {
            ColorMode::Rgb | ColorMode::Bgr => 3,
            ColorMode::Rgba | ColorMode::Bgra => 4,
        }
    }
}

impl From<ColorMode> for crate::ColorMode {
    fn from(mode: ColorMode) -> Self {
        match mode {
            ColorMode::Rgb => crate::ColorMode::Rgb,
            ColorMode::Rgba => crate::ColorMode::Rgba,
            ColorMode::Bgra => crate::ColorMode::Bgra,
            ColorMode::Bgr => crate::ColorMode::Bgr,
        }
    }
}

/// Owned WebP data buffer.
#[derive(Debug)]
pub struct WebPData(Vec<u8>);

impl WebPData {
    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Get the length of the buffer.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Deref for WebPData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[u8]> for WebPData {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Error type compatible with webp-animation.
#[derive(Debug, PartialEq)]
pub enum Error {
    /// Encoder creation failed.
    EncoderCreateFailed,
    /// Frame add failed.
    EncoderAddFailed,
    /// Encoder assembly failed.
    EncoderAssmebleFailed,
    /// Decode failed.
    DecodeFailed,
    /// Buffer size mismatch.
    BufferSizeFailed(usize, usize),
    /// Timestamp ordering error.
    TimestampMustBeHigherThanPrevious(i32, i32),
    /// No frames added.
    NoFramesAdded,
    /// Dimensions must be positive.
    DimensionsMustbePositive,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::EncoderCreateFailed => write!(f, "Encoder creation failed"),
            Error::EncoderAddFailed => write!(f, "Frame add failed"),
            Error::EncoderAssmebleFailed => write!(f, "Encoder assembly failed"),
            Error::DecodeFailed => write!(f, "Decode failed"),
            Error::BufferSizeFailed(got, expected) => {
                write!(
                    f,
                    "Buffer size mismatch: got {}, expected {}",
                    got, expected
                )
            }
            Error::TimestampMustBeHigherThanPrevious(ts, prev) => {
                write!(f, "Timestamp {} must be higher than previous {}", ts, prev)
            }
            Error::NoFramesAdded => write!(f, "No frames added"),
            Error::DimensionsMustbePositive => write!(f, "Dimensions must be positive"),
        }
    }
}

/// Encoder options.
#[derive(Debug, Clone, Default)]
pub struct EncoderOptions {
    /// Minimum keyframe interval.
    pub kmin: i32,
    /// Maximum keyframe interval.
    pub kmax: i32,
    /// Encoding configuration.
    pub encoding_config: Option<EncodingConfig>,
}

/// Encoding configuration.
#[derive(Debug, Clone)]
pub struct EncodingConfig {
    /// Quality (0-100).
    pub quality: f32,
    /// Encoding type.
    pub encoding_type: EncodingType,
}

impl Default for EncodingConfig {
    fn default() -> Self {
        Self {
            quality: 75.0,
            encoding_type: EncodingType::Lossy(LossyEncodingConfig::default()),
        }
    }
}

/// Encoding type.
#[derive(Debug, Clone)]
pub enum EncodingType {
    /// Lossy encoding.
    Lossy(LossyEncodingConfig),
    /// Lossless encoding.
    Lossless,
}

/// Lossy encoding configuration.
#[derive(Debug, Clone, Default)]
pub struct LossyEncodingConfig {
    /// Number of segments.
    pub segments: i32,
    /// Alpha compression.
    pub alpha_compression: bool,
}

/// Decoded animation frame.
#[derive(Debug, Clone)]
pub struct Frame {
    data: Vec<u8>,
    width: u32,
    height: u32,
    timestamp: i32,
    color_mode: ColorMode,
}

impl Frame {
    /// Get frame dimensions as tuple.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get frame pixel data.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Get frame timestamp in milliseconds.
    pub fn timestamp(&self) -> i32 {
        self.timestamp
    }

    /// Get the color mode.
    pub fn color_mode(&self) -> ColorMode {
        self.color_mode
    }
}

/// Decoder options.
#[derive(Debug, Clone, Default)]
pub struct DecoderOptions {
    /// Use multi-threaded decoding.
    pub use_threads: bool,
    /// Output color mode.
    pub color_mode: ColorMode,
}

/// Animation encoder (compatible with `webp_animation::Encoder`).
#[cfg(feature = "animation")]
pub struct Encoder {
    inner: crate::AnimationEncoder,
    previous_timestamp: i32,
    frame_count: u32,
}

#[cfg(feature = "animation")]
impl Encoder {
    /// Create a new encoder with dimensions.
    pub fn new(dimensions: (u32, u32)) -> Result<Self, Error> {
        Self::new_with_options(dimensions, EncoderOptions::default())
    }

    /// Create a new encoder with options.
    pub fn new_with_options(
        dimensions: (u32, u32),
        options: EncoderOptions,
    ) -> Result<Self, Error> {
        let (width, height) = dimensions;
        if width == 0 || height == 0 {
            return Err(Error::DimensionsMustbePositive);
        }

        let mut inner =
            crate::AnimationEncoder::new(width, height).map_err(|_| Error::EncoderCreateFailed)?;

        if let Some(config) = &options.encoding_config {
            inner.set_quality(config.quality);
            if matches!(config.encoding_type, EncodingType::Lossless) {
                inner.set_lossless(true);
            }
        }

        Ok(Self {
            inner,
            previous_timestamp: -1,
            frame_count: 0,
        })
    }

    /// Add a frame at the given timestamp.
    pub fn add_frame(&mut self, data: &[u8], timestamp_ms: i32) -> Result<(), Error> {
        if timestamp_ms <= self.previous_timestamp {
            return Err(Error::TimestampMustBeHigherThanPrevious(
                timestamp_ms,
                self.previous_timestamp,
            ));
        }

        self.inner
            .add_frame(data, timestamp_ms)
            .map_err(|_| Error::EncoderAddFailed)?;

        self.previous_timestamp = timestamp_ms;
        self.frame_count += 1;
        Ok(())
    }

    /// Finalize the animation and return WebP data.
    ///
    /// Note: This is named `finalize` to match webp-animation API.
    /// The underlying webpx API uses `finish`.
    pub fn finalize(self, end_timestamp_ms: i32) -> Result<WebPData, Error> {
        if self.frame_count == 0 {
            return Err(Error::NoFramesAdded);
        }

        let data = self
            .inner
            .finish(end_timestamp_ms)
            .map_err(|_| Error::EncoderAssmebleFailed)?;

        Ok(WebPData(data))
    }
}

/// Animation decoder (compatible with `webp_animation::Decoder`).
#[cfg(feature = "animation")]
pub struct Decoder<'a> {
    data: &'a [u8],
    options: DecoderOptions,
}

#[cfg(feature = "animation")]
impl<'a> Decoder<'a> {
    /// Create a new decoder from WebP data.
    pub fn new(data: &'a [u8]) -> Result<Self, Error> {
        Self::new_with_options(data, DecoderOptions::default())
    }

    /// Create a new decoder with options.
    pub fn new_with_options(data: &'a [u8], options: DecoderOptions) -> Result<Self, Error> {
        if data.is_empty() {
            return Err(Error::DecodeFailed);
        }
        Ok(Self { data, options })
    }

    /// Decode all frames into a vector.
    pub fn decode(&self) -> Result<Vec<Frame>, Error> {
        let mut decoder = crate::AnimationDecoder::with_options(
            self.data,
            self.options.color_mode.into(),
            self.options.use_threads,
        )
        .map_err(|_| Error::DecodeFailed)?;

        let mut frames = Vec::new();

        while let Some(frame) = decoder.next_frame().map_err(|_| Error::DecodeFailed)? {
            frames.push(Frame {
                data: frame.data,
                width: frame.width,
                height: frame.height,
                timestamp: frame.timestamp_ms,
                color_mode: self.options.color_mode,
            });
        }

        Ok(frames)
    }
}

#[cfg(feature = "animation")]
impl<'a> IntoIterator for Decoder<'a> {
    type Item = Frame;
    type IntoIter = DecoderIterator;

    fn into_iter(self) -> Self::IntoIter {
        let inner = crate::AnimationDecoder::with_options(
            self.data,
            self.options.color_mode.into(),
            self.options.use_threads,
        )
        .ok();

        DecoderIterator {
            inner,
            color_mode: self.options.color_mode,
        }
    }
}

/// Iterator over animation frames.
#[cfg(feature = "animation")]
pub struct DecoderIterator {
    inner: Option<crate::AnimationDecoder>,
    color_mode: ColorMode,
}

#[cfg(feature = "animation")]
impl Iterator for DecoderIterator {
    type Item = Frame;

    fn next(&mut self) -> Option<Self::Item> {
        let decoder = self.inner.as_mut()?;
        let frame = decoder.next_frame().ok()??;

        Some(Frame {
            data: frame.data,
            width: frame.width,
            height: frame.height,
            timestamp: frame.timestamp_ms,
            color_mode: self.color_mode,
        })
    }
}

/// Prelude for common imports.
pub mod prelude {
    pub use super::ColorMode;
    #[cfg(feature = "animation")]
    pub use super::{Decoder, DecoderOptions, Encoder, EncoderOptions};
    #[cfg(feature = "animation")]
    pub use super::{EncodingConfig, EncodingType, LossyEncodingConfig};
}

#[cfg(all(test, feature = "animation"))]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_animation() {
        let frame1 = vec![255u8; 4 * 4 * 4]; // 4x4 white
        let frame2 = vec![0u8; 4 * 4 * 4]; // 4x4 black

        let mut encoder = Encoder::new((4, 4)).expect("create encoder");
        encoder.add_frame(&frame1, 0).expect("add frame 1");
        encoder.add_frame(&frame2, 100).expect("add frame 2");

        let webp = encoder.finalize(200).expect("finalize");
        assert!(!webp.is_empty());

        let decoder = Decoder::new(&webp).expect("create decoder");
        let frames: Vec<_> = decoder.into_iter().collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].dimensions(), (4, 4));
    }

    #[test]
    fn test_timestamp_ordering() {
        let frame = vec![0u8; 4 * 4 * 4];
        let mut encoder = Encoder::new((4, 4)).expect("create encoder");

        encoder.add_frame(&frame, 100).expect("add frame");
        let result = encoder.add_frame(&frame, 50); // timestamp goes backwards
        assert!(matches!(
            result,
            Err(Error::TimestampMustBeHigherThanPrevious(50, 100))
        ));
    }
}
