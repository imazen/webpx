//! Resource limits for decode/encode operations.
//!
//! [`Limits`] defines caps on resource usage. [`LimitExceeded`] is returned
//! when a check fails. The shape matches `zencodec::ResourceLimits` so a
//! caller carrying a single resource policy across multiple imazen codecs
//! can lift the relevant fields into webpx's [`Limits`] without re-thinking
//! the units.
//!
//! Use the `check_*` methods for parse-time rejection (fastest — reject
//! before any pixel work):
//!
//! ```rust,no_run
//! use webpx::{Decoder, DecoderConfig, Limits};
//!
//! let limits = Limits::none()
//!     .with_max_pixels(64 * 1024 * 1024)            // 64 MP per frame
//!     .with_max_total_pixels(256 * 1024 * 1024)     // 256 MP across all frames
//!     .with_max_frames(1024)
//!     .with_max_metadata_bytes(4 * 1024 * 1024);    // 4 MB ICC/EXIF/XMP
//!
//! let webp_data: &[u8] = &[];
//! let img = Decoder::new(webp_data)?
//!     .config(DecoderConfig::new().limits(limits))
//!     .decode_rgba()?;
//! # Ok::<(), webpx::At<webpx::Error>>(())
//! ```

/// Resource limits for decode/encode operations.
///
/// All fields are optional; `None` means no webpx-side limit (libwebp's
/// intrinsic 16383×16383 cap still applies). Codecs enforce what they
/// can — not all limit types apply to every operation.
///
/// Field naming matches `zencodec::ResourceLimits` so cross-codec policy
/// objects map cleanly. Threading is intentionally not in this struct —
/// it's a performance knob, not a DoS budget; use
/// [`DecoderConfig::use_threads`](crate::DecoderConfig::use_threads).
///
/// # Enforcement matrix
///
/// "Auto" means webpx checks the field for you when you pass `Limits` to
/// the listed entry point. "Manual" means the field is part of `Limits`
/// for shape compatibility but webpx does not auto-check it on this path
/// — call the corresponding `check_*` method yourself.
///
/// | Field | `DecoderConfig::limits` | `AnimationDecoder::with_options_limits` | `mux::*_with_limits` | Encoder paths |
/// |---|---|---|---|---|
/// | `max_input_bytes` | Auto (pre-features) | Auto (pre-decoder) | Auto (pre-demux) | n/a |
/// | `max_width` / `max_height` / `max_pixels` | Auto (declared dims, post-scale) | Auto (canvas dims) | n/a | Manual via [`check_dimensions`](Self::check_dimensions) |
/// | `max_total_pixels` | Auto (still = w × h × 1) | Auto (w × h × frame_count) | n/a | n/a |
/// | `max_frames` | n/a | Auto (declared frame_count) | n/a | n/a |
/// | `max_animation_ms` | n/a | Auto in [`AnimationDecoder::decode_all`](crate::AnimationDecoder::decode_all) (cumulative timestamp) | n/a | Manual via [`check_animation_ms`](Self::check_animation_ms) |
/// | `max_metadata_bytes` | n/a | n/a | Auto (chunk size) | n/a |
/// | `max_output_bytes` | n/a | n/a | n/a | Manual via [`check_output_size`](Self::check_output_size) on the encoded `Vec` |
///
/// "Manual" fields are not lying about being available — they're real
/// caps you can apply with one line of caller code, just not yet wired
/// into the encoder builder paths. A future minor release will lift the
/// encoder caps to "Auto" without changing the public `Limits` shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// Maximum pixels in a single frame (`width × height`).
    ///
    /// **Per-frame** limit. For animations, each frame is checked
    /// independently. To bound the cumulative pixel count across all
    /// frames, use [`max_total_pixels`](Self::max_total_pixels) too.
    pub max_pixels: Option<u64>,
    /// Maximum pixels across **all frames** (`width × height × frame_count`).
    ///
    /// A 1000×1000 animation with 200 frames has 200 million total pixels.
    /// `max_pixels` would pass each 1 MP frame individually — this field
    /// catches the cumulative cost.
    pub max_total_pixels: Option<u64>,
    /// Maximum image width in pixels.
    pub max_width: Option<u32>,
    /// Maximum image height in pixels.
    pub max_height: Option<u32>,
    /// Maximum decode input size in bytes.
    pub max_input_bytes: Option<u64>,
    /// Maximum number of animation frames.
    pub max_frames: Option<u32>,
    /// Maximum total animation duration in milliseconds.
    pub max_animation_ms: Option<u64>,
    /// Maximum size of an ICCP / EXIF / XMP chunk returned from the
    /// demuxer.
    ///
    /// `None` means the webpx-internal hard cap (256 MiB) still applies.
    /// `Some(n)` rejects chunks larger than `n` (and is also bounded by
    /// the 256 MiB hard cap — a `Some` value larger than 256 MiB is
    /// effectively the hard cap).
    pub max_metadata_bytes: Option<u32>,
    /// Maximum encoded output size in bytes (encode operations).
    pub max_output_bytes: Option<u64>,
}

impl Limits {
    /// No webpx-side limits — only libwebp's intrinsic caps apply.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Set [`max_pixels`](Self::max_pixels).
    #[must_use]
    pub fn with_max_pixels(mut self, max: u64) -> Self {
        self.max_pixels = Some(max);
        self
    }

    /// Set [`max_total_pixels`](Self::max_total_pixels).
    #[must_use]
    pub fn with_max_total_pixels(mut self, max: u64) -> Self {
        self.max_total_pixels = Some(max);
        self
    }

    /// Set [`max_width`](Self::max_width).
    #[must_use]
    pub fn with_max_width(mut self, max: u32) -> Self {
        self.max_width = Some(max);
        self
    }

    /// Set [`max_height`](Self::max_height).
    #[must_use]
    pub fn with_max_height(mut self, max: u32) -> Self {
        self.max_height = Some(max);
        self
    }

    /// Set [`max_input_bytes`](Self::max_input_bytes).
    #[must_use]
    pub fn with_max_input_bytes(mut self, max: u64) -> Self {
        self.max_input_bytes = Some(max);
        self
    }

    /// Set [`max_frames`](Self::max_frames).
    #[must_use]
    pub fn with_max_frames(mut self, max: u32) -> Self {
        self.max_frames = Some(max);
        self
    }

    /// Set [`max_animation_ms`](Self::max_animation_ms).
    #[must_use]
    pub fn with_max_animation_ms(mut self, max: u64) -> Self {
        self.max_animation_ms = Some(max);
        self
    }

    /// Set [`max_metadata_bytes`](Self::max_metadata_bytes).
    #[must_use]
    pub fn with_max_metadata_bytes(mut self, max: u32) -> Self {
        self.max_metadata_bytes = Some(max);
        self
    }

    /// Set [`max_output_bytes`](Self::max_output_bytes).
    #[must_use]
    pub fn with_max_output_bytes(mut self, max: u64) -> Self {
        self.max_output_bytes = Some(max);
        self
    }

    /// Whether any limits are set.
    #[must_use]
    pub fn has_any(&self) -> bool {
        self.max_pixels.is_some()
            || self.max_total_pixels.is_some()
            || self.max_width.is_some()
            || self.max_height.is_some()
            || self.max_input_bytes.is_some()
            || self.max_frames.is_some()
            || self.max_animation_ms.is_some()
            || self.max_metadata_bytes.is_some()
            || self.max_output_bytes.is_some()
    }

    // --- Validation methods ---

    /// Check `width × height` against `max_width`, `max_height`, `max_pixels`.
    pub fn check_dimensions(&self, width: u32, height: u32) -> Result<(), LimitExceeded> {
        if let Some(max) = self.max_width
            && width > max
        {
            return Err(LimitExceeded::Width { actual: width, max });
        }
        if let Some(max) = self.max_height
            && height > max
        {
            return Err(LimitExceeded::Height {
                actual: height,
                max,
            });
        }
        if let Some(max) = self.max_pixels {
            let pixels = u64::from(width).saturating_mul(u64::from(height));
            if pixels > max {
                return Err(LimitExceeded::Pixels {
                    actual: pixels,
                    max,
                });
            }
        }
        Ok(())
    }

    /// Check input data size against `max_input_bytes`.
    pub fn check_input_size(&self, bytes: u64) -> Result<(), LimitExceeded> {
        if let Some(max) = self.max_input_bytes
            && bytes > max
        {
            return Err(LimitExceeded::InputSize { actual: bytes, max });
        }
        Ok(())
    }

    /// Check encoded output size against `max_output_bytes`.
    pub fn check_output_size(&self, bytes: u64) -> Result<(), LimitExceeded> {
        if let Some(max) = self.max_output_bytes
            && bytes > max
        {
            return Err(LimitExceeded::OutputSize { actual: bytes, max });
        }
        Ok(())
    }

    /// Check frame count against `max_frames`.
    pub fn check_frames(&self, count: u32) -> Result<(), LimitExceeded> {
        if let Some(max) = self.max_frames
            && count > max
        {
            return Err(LimitExceeded::Frames { actual: count, max });
        }
        Ok(())
    }

    /// Check animation duration against `max_animation_ms`.
    pub fn check_animation_ms(&self, ms: u64) -> Result<(), LimitExceeded> {
        if let Some(max) = self.max_animation_ms
            && ms > max
        {
            return Err(LimitExceeded::Duration { actual: ms, max });
        }
        Ok(())
    }

    /// Check total pixels across all frames against `max_total_pixels`.
    pub fn check_total_pixels(&self, total: u64) -> Result<(), LimitExceeded> {
        if let Some(max) = self.max_total_pixels
            && total > max
        {
            return Err(LimitExceeded::TotalPixels { actual: total, max });
        }
        Ok(())
    }

    /// Check a metadata chunk (ICCP / EXIF / XMP) byte count against
    /// `max_metadata_bytes`.
    pub fn check_metadata_bytes(&self, bytes: u32) -> Result<(), LimitExceeded> {
        if let Some(max) = self.max_metadata_bytes
            && bytes > max
        {
            return Err(LimitExceeded::MetadataSize { actual: bytes, max });
        }
        Ok(())
    }

    /// Check a still image's `(width, height)` plus a frame count of 1
    /// against all dimension and pixel-budget limits in one call.
    pub fn check_still_image(&self, width: u32, height: u32) -> Result<(), LimitExceeded> {
        self.check_dimensions(width, height)?;
        if let Some(max) = self.max_total_pixels {
            let total = u64::from(width).saturating_mul(u64::from(height));
            if total > max {
                return Err(LimitExceeded::TotalPixels { actual: total, max });
            }
        }
        Ok(())
    }

    /// Check an animated image's `(width, height, frame_count)` against
    /// `max_width`, `max_height`, `max_pixels`, `max_frames`, and
    /// `max_total_pixels` in one call.
    pub fn check_animation(
        &self,
        width: u32,
        height: u32,
        frame_count: u32,
    ) -> Result<(), LimitExceeded> {
        self.check_dimensions(width, height)?;
        self.check_frames(frame_count)?;
        if let Some(max) = self.max_total_pixels {
            let total = u64::from(width)
                .saturating_mul(u64::from(height))
                .saturating_mul(u64::from(frame_count));
            if total > max {
                return Err(LimitExceeded::TotalPixels { actual: total, max });
            }
        }
        Ok(())
    }
}

/// A resource limit was exceeded.
///
/// Each variant carries the actual value and the limit that was exceeded
/// so the message can be useful. Implements [`core::error::Error`] so
/// callers can wrap or propagate it; webpx's own [`Error`](crate::Error)
/// already converts via the `?` operator (see `Error::LimitExceeded`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LimitExceeded {
    /// Image width exceeded `max_width`.
    Width {
        /// Actual width.
        actual: u32,
        /// Maximum allowed.
        max: u32,
    },
    /// Image height exceeded `max_height`.
    Height {
        /// Actual height.
        actual: u32,
        /// Maximum allowed.
        max: u32,
    },
    /// Pixel count exceeded `max_pixels`.
    Pixels {
        /// Actual pixel count.
        actual: u64,
        /// Maximum allowed.
        max: u64,
    },
    /// Total pixels across all frames exceeded `max_total_pixels`.
    TotalPixels {
        /// Actual total pixel count (`width × height × frames`).
        actual: u64,
        /// Maximum allowed.
        max: u64,
    },
    /// Input data size exceeded `max_input_bytes`.
    InputSize {
        /// Actual input size in bytes.
        actual: u64,
        /// Maximum allowed.
        max: u64,
    },
    /// Encoded output exceeded `max_output_bytes`.
    OutputSize {
        /// Actual or estimated output size in bytes.
        actual: u64,
        /// Maximum allowed.
        max: u64,
    },
    /// Frame count exceeded `max_frames`.
    Frames {
        /// Actual frame count.
        actual: u32,
        /// Maximum allowed.
        max: u32,
    },
    /// Animation duration exceeded `max_animation_ms`.
    Duration {
        /// Actual duration in milliseconds.
        actual: u64,
        /// Maximum allowed.
        max: u64,
    },
    /// Metadata chunk size exceeded `max_metadata_bytes`.
    MetadataSize {
        /// Actual chunk size in bytes.
        actual: u32,
        /// Maximum allowed.
        max: u32,
    },
}

impl core::fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Width { actual, max } => write!(f, "width {actual} exceeds limit {max}"),
            Self::Height { actual, max } => write!(f, "height {actual} exceeds limit {max}"),
            Self::Pixels { actual, max } => write!(f, "pixel count {actual} exceeds limit {max}"),
            Self::TotalPixels { actual, max } => {
                write!(f, "total pixels {actual} exceeds limit {max}")
            }
            Self::InputSize { actual, max } => {
                write!(f, "input size {actual} bytes exceeds limit {max}")
            }
            Self::OutputSize { actual, max } => {
                write!(f, "output size {actual} bytes exceeds limit {max}")
            }
            Self::Frames { actual, max } => write!(f, "frame count {actual} exceeds limit {max}"),
            Self::Duration { actual, max } => {
                write!(f, "duration {actual}ms exceeds limit {max}ms")
            }
            Self::MetadataSize { actual, max } => {
                write!(f, "metadata chunk size {actual} bytes exceeds limit {max}")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for LimitExceeded {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_no_limits() {
        assert!(!Limits::none().has_any());
    }

    #[test]
    fn builder_sets_fields() {
        let l = Limits::none()
            .with_max_pixels(1_000_000)
            .with_max_total_pixels(10_000_000)
            .with_max_metadata_bytes(4 * 1024 * 1024);
        assert!(l.has_any());
        assert_eq!(l.max_pixels, Some(1_000_000));
        assert_eq!(l.max_total_pixels, Some(10_000_000));
        assert_eq!(l.max_metadata_bytes, Some(4 * 1024 * 1024));
    }

    #[test]
    fn check_dimensions_pass_and_fail() {
        let l = Limits::none()
            .with_max_width(1920)
            .with_max_height(1080)
            .with_max_pixels(2_073_600);
        assert!(l.check_dimensions(1920, 1080).is_ok());
        assert!(matches!(
            l.check_dimensions(1921, 1080),
            Err(LimitExceeded::Width { .. })
        ));
        assert!(matches!(
            l.check_dimensions(1920, 1081),
            Err(LimitExceeded::Height { .. })
        ));
    }

    #[test]
    fn check_animation_total_pixels_catches_cumulative() {
        // 1000×1000 × 200 frames = 200M; per-frame 1M passes, total fails.
        let l = Limits::none()
            .with_max_pixels(2_000_000)
            .with_max_total_pixels(100_000_000);
        let err = l.check_animation(1000, 1000, 200).unwrap_err();
        assert_eq!(
            err,
            LimitExceeded::TotalPixels {
                actual: 200_000_000,
                max: 100_000_000
            }
        );
    }

    #[test]
    fn check_still_image_includes_total_pixels() {
        let l = Limits::none().with_max_total_pixels(1_000_000);
        // 1001×1000 = 1_001_000 > 1_000_000
        let err = l.check_still_image(1001, 1000).unwrap_err();
        assert_eq!(
            err,
            LimitExceeded::TotalPixels {
                actual: 1_001_000,
                max: 1_000_000
            }
        );
    }

    #[test]
    fn check_metadata_bytes() {
        let l = Limits::none().with_max_metadata_bytes(4096);
        assert!(l.check_metadata_bytes(2048).is_ok());
        assert!(matches!(
            l.check_metadata_bytes(8192),
            Err(LimitExceeded::MetadataSize { .. })
        ));
    }

    #[test]
    fn check_dimensions_no_limits_always_passes() {
        let l = Limits::none();
        assert!(l.check_dimensions(u32::MAX, u32::MAX).is_ok());
    }

    #[test]
    fn limit_exceeded_display() {
        use alloc::format;
        let err = LimitExceeded::Width {
            actual: 5000,
            max: 4096,
        };
        assert_eq!(format!("{err}"), "width 5000 exceeds limit 4096");
        let err = LimitExceeded::TotalPixels {
            actual: 200_000_000,
            max: 100_000_000,
        };
        assert_eq!(
            format!("{err}"),
            "total pixels 200000000 exceeds limit 100000000"
        );
    }
}
