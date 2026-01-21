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
//! Estimates are based on empirical measurements using heaptrack and may vary
//! by ±15% depending on image content, hardware, and configuration.
//!
//! # Measured Data (libwebp 1.5, gradient test images)
//!
//! ## Lossy Encoding (q85, all methods)
//!
//! | Size | Pixels | Peak Memory | Bytes/Pixel |
//! |------|--------|-------------|-------------|
//! | 128x128 | 16K | 0.32 MB | 20.2 |
//! | 256x256 | 65K | 0.95 MB | 15.1 |
//! | 512x512 | 262K | 3.58 MB | 14.3 |
//! | 1024x1024 | 1M | 14.01 MB | 14.0 |
//! | 2048x2048 | 4M | 55.06 MB | 13.8 |
//!
//! **Formula: `peak ≈ 150KB + pixels × 14 bytes`**
//! Method has <5% impact on lossy memory usage.
//!
//! ## Lossless Encoding - Method 0 (fastest)
//!
//! | Size | Pixels | Peak Memory | Bytes/Pixel |
//! |------|--------|-------------|-------------|
//! | 512x512 | 262K | 6.98 MB | 27.9 |
//! | 1024x1024 | 1M | 24.51 MB | 24.5 |
//! | 2048x2048 | 4M | 97.66 MB | 24.4 |
//!
//! **Formula: `peak ≈ 0.6MB + pixels × 24 bytes`**
//!
//! ## Lossless Encoding - Method 4-6 (higher quality)
//!
//! | Size | Pixels | Peak Memory | Bytes/Pixel |
//! |------|--------|-------------|-------------|
//! | 512x512 | 262K | 16.09 MB | 64.4 |
//! | 1024x1024 | 1M | 35.54 MB | 35.5 |
//! | 2048x2048 | 4M | 137.52 MB | 34.4 |
//!
//! **Formula: `peak ≈ 10MB + pixels × 32 bytes`**
//!
//! ## Method Impact on Memory
//!
//! - Lossy: Method has <5% impact on memory
//! - Lossless: Method 0 uses 30-45% LESS memory than method 4-6

use crate::config::{EncoderConfig, Preset};

// =============================================================================
// Lossy encoding constants
// Measured: Methods 0-2 vs 3-6 differ by ~3%, so we use a single baseline
// =============================================================================

/// Bytes per pixel for lossy encoding methods 0-2.
const LOSSY_M0_BYTES_PER_PIXEL: f64 = 13.4;

/// Fixed overhead for lossy encoding methods 0-2 (~115KB).
const LOSSY_M0_FIXED_OVERHEAD: u64 = 115_000;

/// Bytes per pixel for lossy encoding methods 3-6.
const LOSSY_M3_BYTES_PER_PIXEL: f64 = 13.7;

/// Fixed overhead for lossy encoding methods 3-6 (~220KB).
const LOSSY_M3_FIXED_OVERHEAD: u64 = 220_000;

// =============================================================================
// Lossless encoding constants - METHOD 0 (fastest, least memory)
// =============================================================================

/// Bytes per pixel for lossless method 0 encoding.
const LOSSLESS_M0_BYTES_PER_PIXEL: f64 = 24.0;

/// Fixed overhead for lossless method 0 encoding (~0.6MB).
const LOSSLESS_M0_FIXED_OVERHEAD: u64 = 600_000;

// =============================================================================
// Lossless encoding constants - METHODS 1-6 (higher quality tiers)
// At large sizes (1024+), methods 1-6 converge to similar memory usage.
// The main distinction is method 0 vs methods 1+.
// =============================================================================

/// Bytes per pixel for lossless methods 1-6 encoding.
/// Converges to ~34 bytes/pixel at large sizes.
const LOSSLESS_M1_BYTES_PER_PIXEL: f64 = 34.0;

/// Fixed overhead for lossless methods 1-6 encoding (~1.5MB).
/// Note: At smaller sizes (<512px), actual usage may be higher due to
/// non-linear hash table sizing effects.
const LOSSLESS_M1_FIXED_OVERHEAD: u64 = 1_500_000;

/// Resource estimation for encoding operations.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EncodeEstimate {
    /// Estimated peak memory in bytes during encoding.
    ///
    /// Includes input buffer, libwebp internal state, and output buffer.
    /// Based on heaptrack measurements of actual libwebp allocations.
    pub peak_memory_bytes: u64,

    /// Estimated heap allocations during encoding.
    ///
    /// Fewer allocations = better latency (less GC pressure).
    pub estimated_allocations: u32,

    /// Relative time factor (1.0 = baseline lossy q85 m4).
    ///
    /// Multiply by baseline time to get estimated time.
    pub time_factor: f32,

    /// Estimated output size in bytes.
    ///
    /// Rough estimate based on quality and compression type.
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
/// Based on heaptrack measurements of libwebp memory usage.
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
/// println!("Peak memory: {:.1} MB", est.peak_memory_bytes as f64 / 1_000_000.0);
/// println!("Relative time: {:.1}x baseline", est.time_factor);
/// ```
#[must_use]
pub fn estimate_encode(width: u32, height: u32, bpp: u8, config: &EncoderConfig) -> EncodeEstimate {
    let pixels = (width as u64) * (height as u64);
    let input_bytes = pixels * (bpp as u64);

    // Peak memory based on empirical heaptrack measurements
    let peak_memory_bytes = if config.lossless {
        // Lossless memory varies significantly by method:
        // - Method 0: ~0.6MB + 24 bytes/pixel (fastest, ~40% less memory)
        // - Methods 1-6: ~1.5MB + 34 bytes/pixel (converge at large sizes)
        if config.method == 0 {
            LOSSLESS_M0_FIXED_OVERHEAD + (pixels as f64 * LOSSLESS_M0_BYTES_PER_PIXEL) as u64
        } else {
            LOSSLESS_M1_FIXED_OVERHEAD + (pixels as f64 * LOSSLESS_M1_BYTES_PER_PIXEL) as u64
        }
    } else {
        // Lossy memory is relatively stable across methods (~3% variation):
        // - Methods 0-2: ~115KB + 13.4 bytes/pixel
        // - Methods 3-6: ~220KB + 13.7 bytes/pixel
        if config.method <= 2 {
            LOSSY_M0_FIXED_OVERHEAD + (pixels as f64 * LOSSY_M0_BYTES_PER_PIXEL) as u64
        } else {
            LOSSY_M3_FIXED_OVERHEAD + (pixels as f64 * LOSSY_M3_BYTES_PER_PIXEL) as u64
        }
    };

    // Output estimate based on quality and compression type
    let output_ratio: f64 = if config.lossless {
        // Lossless: typically 20-80% of input size depending on content
        0.5
    } else {
        // Lossy: roughly correlates with quality
        // Measured: ~0.3% at q85 for gradient images, real photos ~5-15%
        let q = (config.quality.clamp(0.0, 100.0)) as f64;
        0.02 + (q / 100.0) * 0.18
    };
    let estimated_output = (input_bytes as f64 * output_ratio) as u64;

    // Time factor based on method and lossless flag
    // Measured relative times (method 4 = 1.0):
    // method 0: ~0.25x, method 6: ~1.1x
    let method_factor = match config.method {
        0 => 0.25,
        1 => 0.4,
        2 => 0.55,
        3 => 0.75,
        4 => 1.0,
        5 => 1.05,
        6 => 1.1,
        _ => 1.0,
    };

    // Lossless is ~5-10x slower than lossy
    let lossless_factor = if config.lossless { 6.0 } else { 1.0 };

    // Near-lossless mode is slower
    let quality_factor = if config.near_lossless < 100 { 1.3 } else { 1.0 };

    // Preset factors
    let preset_factor = match config.preset {
        Preset::Default => 1.0,
        Preset::Photo => 1.05,
        Preset::Picture => 1.0,
        Preset::Drawing => 1.0,
        Preset::Icon => 0.95,
        Preset::Text => 0.95,
    };

    let time_factor = method_factor * lossless_factor * quality_factor * preset_factor;

    // Allocations: measured ~20-30 per encode for most sizes
    let estimated_allocations = 25;

    EncodeEstimate {
        peak_memory_bytes,
        estimated_allocations,
        time_factor: time_factor as f32,
        estimated_output_bytes: estimated_output,
    }
}

/// Estimate resources for decoding an image.
///
/// Decode memory is primarily the output buffer plus libwebp internal state.
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
/// println!("Output buffer: {:.1} MB", est.output_bytes as f64 / 1_000_000.0);
/// println!("Peak memory: {:.1} MB", est.peak_memory_bytes as f64 / 1_000_000.0);
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

    // Decode memory is typically less than encode
    // Using method 1+ constants as baseline since decode doesn't know encode method
    let peak_memory_bytes = if is_lossless {
        // Lossless decode needs hash tables for backward references
        // Roughly half the encode cost
        LOSSLESS_M1_FIXED_OVERHEAD / 2
            + (pixels as f64 * LOSSLESS_M1_BYTES_PER_PIXEL / 2.0) as u64
    } else {
        // Lossy decode is lighter than encode
        LOSSY_M3_FIXED_OVERHEAD + (pixels as f64 * LOSSY_M3_BYTES_PER_PIXEL / 2.0) as u64
    };

    // Time factor: decode is generally faster than encode
    // Lossless decode is slightly slower
    let time_factor = if is_lossless { 0.3 } else { 0.2 };

    // Allocations: minimal for decode
    let estimated_allocations = 10;

    DecodeEstimate {
        peak_memory_bytes,
        estimated_allocations,
        time_factor,
        output_bytes,
    }
}

/// Estimate resources for decoding into a pre-allocated buffer.
///
/// This path avoids allocating the output buffer, reducing peak memory
/// by the output size.
///
/// # Example
///
/// ```rust
/// use webpx::heuristics::estimate_decode_zerocopy;
///
/// let est = estimate_decode_zerocopy(1920, 1080, false);
/// println!("Peak memory (zero-copy): {:.1} MB", est.peak_memory_bytes as f64 / 1_000_000.0);
/// ```
#[must_use]
pub fn estimate_decode_zerocopy(width: u32, height: u32, is_lossless: bool) -> DecodeEstimate {
    let mut est = estimate_decode(width, height, 4, is_lossless);

    // Zero-copy path: output buffer is pre-allocated by caller
    // Subtract output from peak (it's not allocated during decode)
    est.peak_memory_bytes = est.peak_memory_bytes.saturating_sub(est.output_bytes);

    // Fewer allocations since output isn't allocated
    est.estimated_allocations = 5;

    est
}

/// Estimate resources for encoding an animation.
///
/// Animation encoding processes frames sequentially, so peak memory is
/// approximately one frame's worth plus encoder state.
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
/// println!("Peak memory for 30-frame animation: {:.1} MB",
///     est.peak_memory_bytes as f64 / 1_000_000.0);
/// ```
#[must_use]
pub fn estimate_animation_encode(
    width: u32,
    height: u32,
    frame_count: u32,
    config: &EncoderConfig,
) -> EncodeEstimate {
    let single_frame = estimate_encode(width, height, 4, config);

    // Animation encoder keeps previous frame for delta encoding
    // Peak is roughly 1.5x single frame
    let frame_bytes = (width as u64) * (height as u64) * 4;
    let peak_memory = single_frame.peak_memory_bytes + frame_bytes / 2 + 200_000;

    // Time is linear with frame count
    let time_factor = single_frame.time_factor * frame_count as f32;

    // Allocations: base + per-frame
    let estimated_allocations = single_frame.estimated_allocations + (frame_count - 1) * 5;

    // Output: sum of compressed frames
    let estimated_output = single_frame.estimated_output_bytes * (frame_count as u64);

    EncodeEstimate {
        peak_memory_bytes: peak_memory,
        estimated_allocations,
        time_factor,
        estimated_output_bytes: estimated_output,
    }
}

/// Estimate resources for decoding an animation.
///
/// Animation decoding processes one frame at a time.
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

    // Animation decoder holds previous frame for blending
    let frame_bytes = (width as u64) * (height as u64) * 4;
    let peak_memory = single_frame.peak_memory_bytes + frame_bytes;

    // Time is linear with frame count
    let time_factor = single_frame.time_factor * frame_count as f32;

    // Allocations: per-frame output
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
    fn test_lossy_encode_formula_m4() {
        // Test against measured data points (heaptrack, libwebp 1.5)
        // 1024x1024 method 4: measured 14.01 MB
        let est = estimate_encode(1024, 1024, 4, &EncoderConfig::default());
        let measured = 14_010_000u64;
        let error = (est.peak_memory_bytes as f64 - measured as f64).abs() / measured as f64;
        assert!(
            error < 0.10,
            "Lossy 1024x1024 m4: estimated {} vs measured {}, error {:.1}%",
            est.peak_memory_bytes,
            measured,
            error * 100.0
        );
    }

    #[test]
    fn test_lossy_encode_formula_m0() {
        // 1024x1024 method 0: measured 13.52 MB
        let est = estimate_encode(1024, 1024, 4, &EncoderConfig::default().method(0));
        let measured = 13_520_000u64;
        let error = (est.peak_memory_bytes as f64 - measured as f64).abs() / measured as f64;
        assert!(
            error < 0.10,
            "Lossy 1024x1024 m0: estimated {} vs measured {}, error {:.1}%",
            est.peak_memory_bytes,
            measured,
            error * 100.0
        );
    }

    #[test]
    fn test_lossless_encode_formula_m4() {
        // 1024x1024 method 4: measured 35.54 MB
        let est = estimate_encode(
            1024,
            1024,
            4,
            &EncoderConfig::default().lossless(true).method(4),
        );
        let measured = 35_540_000u64;
        let error = (est.peak_memory_bytes as f64 - measured as f64).abs() / measured as f64;
        assert!(
            error < 0.10,
            "Lossless 1024x1024 m4: estimated {} vs measured {}, error {:.1}%",
            est.peak_memory_bytes,
            measured,
            error * 100.0
        );
    }

    #[test]
    fn test_lossless_encode_formula_m0() {
        // 1024x1024 method 0: measured 24.51 MB
        let est = estimate_encode(
            1024,
            1024,
            4,
            &EncoderConfig::default().lossless(true).method(0),
        );
        let measured = 24_510_000u64;
        let error = (est.peak_memory_bytes as f64 - measured as f64).abs() / measured as f64;
        assert!(
            error < 0.10,
            "Lossless 1024x1024 m0: estimated {} vs measured {}, error {:.1}%",
            est.peak_memory_bytes,
            measured,
            error * 100.0
        );
    }

    #[test]
    fn test_lossless_m0_uses_less_memory() {
        let m0 = estimate_encode(
            1024,
            1024,
            4,
            &EncoderConfig::default().lossless(true).method(0),
        );
        let m4 = estimate_encode(
            1024,
            1024,
            4,
            &EncoderConfig::default().lossless(true).method(4),
        );

        // Method 0 should use ~30-45% less memory than method 4
        let ratio = m0.peak_memory_bytes as f64 / m4.peak_memory_bytes as f64;
        assert!(
            ratio < 0.75,
            "Expected m0 < 75% of m4, got ratio {}",
            ratio
        );
    }

    #[test]
    fn test_lossless_more_memory() {
        let lossy = estimate_encode(512, 512, 4, &EncoderConfig::default());
        let lossless = estimate_encode(512, 512, 4, &EncoderConfig::default().lossless(true));

        // Lossless should use more memory than lossy
        assert!(lossless.peak_memory_bytes > lossy.peak_memory_bytes * 2);
    }

    #[test]
    fn test_scaling() {
        // Memory should scale roughly linearly with pixel count
        let small = estimate_encode(512, 512, 4, &EncoderConfig::default());
        let large = estimate_encode(1024, 1024, 4, &EncoderConfig::default());

        // 4x pixels should give ~4x memory (within 50% for fixed overhead)
        let ratio = large.peak_memory_bytes as f64 / small.peak_memory_bytes as f64;
        assert!(ratio > 3.0 && ratio < 5.0, "Ratio was {}", ratio);
    }

    #[test]
    fn test_decode_less_than_encode() {
        let encode = estimate_encode(1024, 1024, 4, &EncoderConfig::default());
        let decode = estimate_decode(1024, 1024, 4, false);

        // Decode should use less memory than encode
        assert!(decode.peak_memory_bytes < encode.peak_memory_bytes);
    }

    #[test]
    fn test_zerocopy_saves_memory() {
        let normal = estimate_decode(1024, 1024, 4, false);
        let zerocopy = estimate_decode_zerocopy(1024, 1024, false);

        // Zero-copy should save the output buffer size
        let output_size = 1024 * 1024 * 4;
        assert!(normal.peak_memory_bytes - zerocopy.peak_memory_bytes >= output_size / 2);
    }
}
