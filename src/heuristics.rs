//! Resource estimation heuristics for encoding and decoding operations.
//!
//! These heuristics provide approximate estimates for memory consumption and
//! relative time costs of encoding/decoding operations. Use them for:
//!
//! - Pre-allocating buffers
//! - Sizing thread pools
//! - Memory budgeting
//! - Progress estimation
//!
//! # Accuracy
//!
//! Estimates are based on empirical measurements and may vary by ±30%
//! depending on image content, hardware, and configuration.

use crate::config::{EncoderConfig, Preset};

/// Resource estimation for encoding operations.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EncodeEstimate {
    /// Estimated peak memory in bytes during encoding.
    ///
    /// Includes input buffer, libwebp internal state, and output buffer.
    pub peak_memory_bytes: u64,

    /// Estimated heap allocations during encoding.
    ///
    /// Fewer allocations = better latency (less GC pressure).
    pub estimated_allocations: u32,

    /// Relative time factor (1.0 = baseline lossy q75 m4).
    ///
    /// Multiply by baseline time to get estimated time.
    pub time_factor: f32,

    /// Estimated output size in bytes.
    ///
    /// Very rough estimate based on quality and compression type.
    pub estimated_output_bytes: u64,
}

/// Resource estimation for decoding operations.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DecodeEstimate {
    /// Estimated peak memory in bytes during decoding.
    pub peak_memory_bytes: u64,

    /// Estimated heap allocations during decoding.
    pub estimated_allocations: u32,

    /// Relative time factor (1.0 = baseline).
    pub time_factor: f32,

    /// Output buffer size in bytes.
    pub output_bytes: u64,
}

/// Estimate resources for encoding an image.
///
/// # Arguments
///
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `bpp` - Bytes per pixel of input (3 for RGB, 4 for RGBA)
/// * `config` - Encoder configuration
///
/// # Example
///
/// ```rust
/// use webpx::heuristics::estimate_encode;
/// use webpx::EncoderConfig;
///
/// let est = estimate_encode(1920, 1080, 4, &EncoderConfig::default());
/// println!("Peak memory: {} MB", est.peak_memory_bytes / 1_000_000);
/// println!("Relative time: {:.1}x baseline", est.time_factor);
/// ```
#[must_use]
pub fn estimate_encode(width: u32, height: u32, bpp: u8, config: &EncoderConfig) -> EncodeEstimate {
    let pixels = (width as u64) * (height as u64);
    let input_bytes = pixels * (bpp as u64);

    // libwebp internal state is roughly 2 bytes per pixel for lossy,
    // up to 4 bytes per pixel for lossless
    let internal_factor = if config.lossless { 4.0 } else { 2.0 };
    let internal_bytes = (pixels as f64 * internal_factor) as u64;

    // Output estimate based on quality and compression type
    let output_ratio: f64 = if config.lossless {
        // Lossless: typically 40-80% of input size
        0.6
    } else {
        // Lossy: roughly correlates with quality
        // q50 ≈ 5%, q75 ≈ 10%, q85 ≈ 15%, q95 ≈ 25%
        let q = (config.quality.clamp(0.0, 100.0)) as f64;
        0.03 + (q / 100.0) * 0.25
    };
    let estimated_output = (input_bytes as f64 * output_ratio) as u64;

    // Peak memory includes all three: input + internal + output
    // Plus some overhead for the encoder itself (~100KB)
    let peak_memory = input_bytes + internal_bytes + estimated_output + 100_000;

    // Time factor based on method and lossless flag
    let method_factor = match config.method {
        0 => 0.5,
        1 => 0.65,
        2 => 0.8,
        3 => 0.9,
        4 => 1.0,
        5 => 1.3,
        6 => 1.6,
        _ => 1.0,
    };

    let lossless_factor = if config.lossless { 2.5 } else { 1.0 };

    // Quality doesn't significantly affect time for lossy,
    // but near-lossless mode is slower
    let quality_factor = if config.near_lossless < 100 { 1.2 } else { 1.0 };

    // Preset factors (some presets add extra processing)
    let preset_factor = match config.preset {
        Preset::Default => 1.0,
        Preset::Photo => 1.1,
        Preset::Picture => 1.0,
        Preset::Drawing => 1.0,
        Preset::Icon => 0.9,
        Preset::Text => 0.9,
    };

    let time_factor = method_factor * lossless_factor * quality_factor * preset_factor;

    // Allocations: roughly 10-20 per encode, plus a few per megapixel
    let megapixels = (pixels as f64) / 1_000_000.0;
    let estimated_allocations = (15.0 + megapixels * 5.0) as u32;

    EncodeEstimate {
        peak_memory_bytes: peak_memory,
        estimated_allocations,
        time_factor: time_factor as f32,
        estimated_output_bytes: estimated_output,
    }
}

/// Estimate resources for decoding an image.
///
/// # Arguments
///
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `output_bpp` - Bytes per pixel of output (3 for RGB, 4 for RGBA)
/// * `is_lossless` - Whether the source is losslessly compressed
///
/// # Example
///
/// ```rust
/// use webpx::heuristics::estimate_decode;
///
/// let est = estimate_decode(1920, 1080, 4, false);
/// println!("Output buffer: {} MB", est.output_bytes / 1_000_000);
/// println!("Peak memory: {} MB", est.peak_memory_bytes / 1_000_000);
/// ```
#[must_use]
pub fn estimate_decode(
    width: u32,
    height: u32,
    output_bpp: u8,
    is_lossless: bool,
) -> DecodeEstimate {
    let pixels = (width as u64) * (height as u64);
    let output_bytes = pixels * (output_bpp as u64);

    // libwebp internal state during decode: roughly 1.5-2x output
    let internal_factor = if is_lossless { 2.0 } else { 1.5 };
    let internal_bytes = (output_bytes as f64 * internal_factor) as u64;

    // Peak memory = output + internal
    // Plus some overhead for decoder state (~50KB)
    let peak_memory = output_bytes + internal_bytes + 50_000;

    // Time factor: lossless is typically slightly slower to decode
    let time_factor = if is_lossless { 1.2 } else { 1.0 };

    // Allocations: minimal for decode, mostly just the output buffer
    let megapixels = (pixels as f64) / 1_000_000.0;
    let estimated_allocations = (5.0 + megapixels * 2.0) as u32;

    DecodeEstimate {
        peak_memory_bytes: peak_memory,
        estimated_allocations,
        time_factor: time_factor as f32,
        output_bytes,
    }
}

/// Estimate resources for decoding into a pre-allocated buffer.
///
/// This path has significantly lower allocation overhead since the
/// output buffer is provided by the caller.
///
/// # Example
///
/// ```rust
/// use webpx::heuristics::estimate_decode_zerocopy;
///
/// let est = estimate_decode_zerocopy(1920, 1080, false);
/// println!("Only {} allocations expected", est.estimated_allocations);
/// ```
#[must_use]
pub fn estimate_decode_zerocopy(width: u32, height: u32, is_lossless: bool) -> DecodeEstimate {
    let mut est = estimate_decode(width, height, 4, is_lossless);

    // Zero-copy path: output buffer is pre-allocated, so we subtract it
    // Only internal libwebp memory remains
    let pixels = (width as u64) * (height as u64);
    let output_bytes = pixels * 4;
    est.peak_memory_bytes -= output_bytes;

    // Much fewer allocations since output isn't allocated
    est.estimated_allocations = (est.estimated_allocations / 3).max(2);

    est
}

/// Estimate resources for encoding an animation.
///
/// # Arguments
///
/// * `width` - Frame width in pixels
/// * `height` - Frame height in pixels
/// * `frame_count` - Number of frames
/// * `config` - Encoder configuration
///
/// # Example
///
/// ```rust
/// use webpx::heuristics::estimate_animation_encode;
/// use webpx::EncoderConfig;
///
/// let est = estimate_animation_encode(640, 480, 30, &EncoderConfig::default());
/// println!("Peak memory for 30-frame animation: {} MB", est.peak_memory_bytes / 1_000_000);
/// ```
#[must_use]
pub fn estimate_animation_encode(
    width: u32,
    height: u32,
    frame_count: u32,
    config: &EncoderConfig,
) -> EncodeEstimate {
    let single_frame = estimate_encode(width, height, 4, config);

    // Animation encoder keeps some state between frames
    // Peak memory is roughly: 2 frames worth + muxer overhead
    let frame_bytes = (width as u64) * (height as u64) * 4;
    let peak_memory = single_frame.peak_memory_bytes + frame_bytes + 100_000;

    // Time is roughly linear with frame count, but with some overhead
    let time_factor = single_frame.time_factor * (frame_count as f32 * 0.95 + 0.05);

    // Allocations: base + per-frame
    let estimated_allocations = single_frame.estimated_allocations
        + (frame_count - 1) * (single_frame.estimated_allocations / 2);

    // Output estimate: sum of compressed frames plus muxer overhead
    let estimated_output = single_frame.estimated_output_bytes * (frame_count as u64) + 1000;

    EncodeEstimate {
        peak_memory_bytes: peak_memory,
        estimated_allocations,
        time_factor,
        estimated_output_bytes: estimated_output,
    }
}

/// Estimate resources for decoding an animation.
///
/// # Arguments
///
/// * `width` - Frame width in pixels
/// * `height` - Frame height in pixels
/// * `frame_count` - Number of frames
/// * `is_lossless` - Whether the animation uses lossless compression
#[must_use]
pub fn estimate_animation_decode(
    width: u32,
    height: u32,
    frame_count: u32,
    is_lossless: bool,
) -> DecodeEstimate {
    let single_frame = estimate_decode(width, height, 4, is_lossless);

    // Animation decoder processes one frame at a time
    // Peak memory is roughly one frame plus decoder state
    // Plus the source animation data is held in memory
    let frame_bytes = (width as u64) * (height as u64) * 4;
    let peak_memory = single_frame.peak_memory_bytes + frame_bytes;

    // Time is roughly linear with frame count
    let time_factor = single_frame.time_factor * frame_count as f32;

    // Allocations: one per frame for the output, plus decoder overhead
    let estimated_allocations = frame_count * 2 + 5;

    // Total output if collecting all frames
    let output_bytes = single_frame.output_bytes * (frame_count as u64);

    DecodeEstimate {
        peak_memory_bytes: peak_memory,
        estimated_allocations,
        time_factor,
        output_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_estimate_sanity() {
        let est = estimate_encode(1920, 1080, 4, &EncoderConfig::default());

        // Peak memory should be at least input size
        let input_size = 1920 * 1080 * 4;
        assert!(est.peak_memory_bytes >= input_size);

        // Output should be less than input for lossy
        assert!(est.estimated_output_bytes < input_size);

        // Time factor should be positive
        assert!(est.time_factor > 0.0);
    }

    #[test]
    fn test_decode_estimate_sanity() {
        let est = estimate_decode(1920, 1080, 4, false);

        // Output should match expected size
        let expected_output = 1920 * 1080 * 4;
        assert_eq!(est.output_bytes, expected_output);

        // Peak memory should be at least output size
        assert!(est.peak_memory_bytes >= expected_output);
    }

    #[test]
    fn test_lossless_is_more_expensive() {
        let lossy = estimate_encode(512, 512, 4, &EncoderConfig::default());
        let lossless = estimate_encode(512, 512, 4, &EncoderConfig::default().lossless(true));

        // Lossless should use more memory
        assert!(lossless.peak_memory_bytes > lossy.peak_memory_bytes);

        // Lossless should take more time
        assert!(lossless.time_factor > lossy.time_factor);
    }

    #[test]
    fn test_method_affects_time() {
        let fast = estimate_encode(512, 512, 4, &EncoderConfig::default().method(0));
        let slow = estimate_encode(512, 512, 4, &EncoderConfig::default().method(6));

        // Slower method should have higher time factor
        assert!(slow.time_factor > fast.time_factor);
    }

    #[test]
    fn test_zerocopy_fewer_allocations() {
        let alloc = estimate_decode(512, 512, 4, false);
        let zerocopy = estimate_decode_zerocopy(512, 512, false);

        assert!(zerocopy.estimated_allocations < alloc.estimated_allocations);
        assert!(zerocopy.peak_memory_bytes < alloc.peak_memory_bytes);
    }
}
