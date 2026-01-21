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
//! Estimates are based on empirical measurements using heaptrack with gradient
//! test images. **Gradient images represent the best case for memory usage.**
//!
//! For worst-case estimation (high-entropy content like noise or complex photos):
//! - **Lossy**: Multiply estimate by 2.5x
//! - **Lossless**: Multiply estimate by 1.5x
//!
//! RGB vs RGBA input has no measurable impact on memory usage (libwebp converts
//! internally).
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

// =============================================================================
// Decoding constants (measured with heaptrack, libwebp 1.5)
// Decode memory is nearly identical for lossy and lossless sources.
// The primary cost is: output buffer + internal decode buffers.
// =============================================================================

/// Bytes per pixel for decoding (internal buffers).
/// This is roughly: YUV decode buffer + color conversion workspace.
const DECODE_BYTES_PER_PIXEL: f64 = 15.0;

/// Fixed overhead for lossy decoding (~76KB).
const DECODE_LOSSY_FIXED_OVERHEAD: u64 = 76_000;

/// Fixed overhead for lossless decoding (~133KB).
/// Slightly higher than lossy due to VP8L-specific data structures.
const DECODE_LOSSLESS_FIXED_OVERHEAD: u64 = 133_000;

/// Resource estimation for encoding operations.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EncodeEstimate {
    /// Minimum expected peak memory (best case: solid color, simple gradient).
    ///
    /// Based on heaptrack measurements with solid color images.
    pub peak_memory_bytes_min: u64,

    /// Typical peak memory (average case: natural photos, moderate complexity).
    ///
    /// Based on heaptrack measurements with gradient images, which represent
    /// typical photographic content reasonably well.
    pub peak_memory_bytes: u64,

    /// Maximum expected peak memory (worst case: noise, high-entropy content).
    ///
    /// Based on heaptrack measurements with random noise images.
    /// Real photos rarely hit this extreme.
    pub peak_memory_bytes_max: u64,

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
///
/// Based on heaptrack measurements of libwebp 1.5 memory usage.
///
/// Key finding: Decode memory is nearly identical for lossy and lossless
/// sources (~15 bytes/pixel total). Content type has minimal impact (~5%).
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DecodeEstimate {
    /// Minimum expected peak memory (best case: simple images).
    ///
    /// Decode memory varies only ~5% with content type, so min ≈ typical.
    pub peak_memory_bytes_min: u64,

    /// Typical peak memory in bytes during decoding.
    ///
    /// Based on heaptrack measurements with gradient test images.
    pub peak_memory_bytes: u64,

    /// Maximum expected peak memory (worst case: noise images).
    ///
    /// Decode memory is relatively stable; max is only ~5% above typical.
    pub peak_memory_bytes_max: u64,

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

    // Content-dependent memory multipliers (measured with heaptrack):
    //
    // LOSSY (gradient is baseline):
    //   - Gradient: 1.0x (baseline - smooth transitions, best case)
    //   - Real photos: 1.04x - 1.27x (average ~1.2x from CLIC2025 test set)
    //   - Noise: 2.25x (high entropy, worst case)
    //
    // LOSSLESS (gradient is baseline):
    //   - Solid: 0.6x (trivial to compress, minimal hash tables)
    //   - Gradient: 1.0x (baseline)
    //   - Real photos: ~1.2x estimated
    //   - Noise: 1.4x (maximum hash table growth)
    let (min_mult, typ_mult, max_mult) = if config.lossless {
        (0.6, 1.2, 1.5) // solid, typical photo, noise
    } else {
        (1.0, 1.2, 2.25) // gradient, typical photo, noise
    };

    let peak_memory_bytes_min = (peak_memory_bytes as f64 * min_mult) as u64;
    // Adjust typical estimate for real-world photos (gradient baseline × 1.2)
    let peak_memory_bytes_typ = (peak_memory_bytes as f64 * typ_mult) as u64;
    let peak_memory_bytes_max = (peak_memory_bytes as f64 * max_mult) as u64;

    EncodeEstimate {
        peak_memory_bytes_min,
        peak_memory_bytes: peak_memory_bytes_typ,
        peak_memory_bytes_max,
        estimated_allocations,
        time_factor: time_factor as f32,
        estimated_output_bytes: estimated_output,
    }
}

/// Estimate resources for decoding an image.
///
/// Based on heaptrack measurements of libwebp 1.5.
///
/// # Key findings:
/// - Decode memory is nearly identical for lossy and lossless (~15 bytes/pixel)
/// - Content type has minimal impact (~5% variation)
/// - Output buffer dominates: ~4 bytes/pixel for RGBA output
/// - Internal buffers: ~11 bytes/pixel
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

    // Measured decode memory formula: overhead + pixels × 15 bytes
    // Lossy and lossless have nearly identical bytes/pixel, only overhead differs
    let fixed_overhead = if is_lossless {
        DECODE_LOSSLESS_FIXED_OVERHEAD
    } else {
        DECODE_LOSSY_FIXED_OVERHEAD
    };

    let peak_memory_bytes = fixed_overhead + (pixels as f64 * DECODE_BYTES_PER_PIXEL) as u64;

    // Content-dependent multipliers (measured):
    // - Min (gradient/solid): 1.0x (baseline)
    // - Typical (photos): 1.0x (decode is very stable)
    // - Max (noise): 1.05x (only ~5% variation)
    let peak_memory_bytes_min = peak_memory_bytes;
    let peak_memory_bytes_max = (peak_memory_bytes as f64 * 1.05) as u64;

    // Time factor: decode is generally faster than encode
    // Lossless decode is slightly slower due to entropy decoding
    let time_factor = if is_lossless { 0.3 } else { 0.2 };

    // Allocations: minimal for decode (measured ~10-15)
    let estimated_allocations = 12;

    DecodeEstimate {
        peak_memory_bytes_min,
        peak_memory_bytes,
        peak_memory_bytes_max,
        estimated_allocations,
        time_factor,
        output_bytes,
    }
}

/// Estimate resources for decoding into a pre-allocated buffer.
///
/// This path uses `decode_rgba_into` or similar functions that write directly
/// to a caller-provided buffer, avoiding the output buffer allocation.
///
/// # Measured savings (heaptrack, libwebp 1.5):
/// - Lossy: saves exactly the output buffer size
/// - Lossless: internal allocations may still dominate (less savings observed)
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
    // For lossy, this saves exactly the output buffer size
    // For lossless, internal VP8L allocations may still dominate
    let savings = if is_lossless {
        // Lossless: internal allocations mean less savings
        // Measured: negligible difference
        0
    } else {
        // Lossy: saves the full output buffer
        est.output_bytes
    };

    est.peak_memory_bytes_min = est.peak_memory_bytes_min.saturating_sub(savings);
    est.peak_memory_bytes = est.peak_memory_bytes.saturating_sub(savings);
    est.peak_memory_bytes_max = est.peak_memory_bytes_max.saturating_sub(savings);

    // Fewer allocations since output isn't allocated
    est.estimated_allocations = 8;

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

    // Content-dependent multipliers (same as single frame)
    let (min_mult, typ_mult, max_mult) = if config.lossless {
        (0.6, 1.2, 1.5)
    } else {
        (1.0, 1.2, 2.25)
    };

    EncodeEstimate {
        peak_memory_bytes_min: (peak_memory as f64 * min_mult) as u64,
        peak_memory_bytes: (peak_memory as f64 * typ_mult) as u64,
        peak_memory_bytes_max: (peak_memory as f64 * max_mult) as u64,
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
    let peak_memory_min = single_frame.peak_memory_bytes_min + frame_bytes;
    let peak_memory = single_frame.peak_memory_bytes + frame_bytes;
    let peak_memory_max = single_frame.peak_memory_bytes_max + frame_bytes;

    // Time is linear with frame count
    let time_factor = single_frame.time_factor * frame_count as f32;

    // Allocations: per-frame output
    let estimated_allocations = frame_count * 2 + 5;

    // Total output if collecting all frames
    let output_bytes = single_frame.output_bytes * (frame_count as u64);

    DecodeEstimate {
        peak_memory_bytes_min: peak_memory_min,
        peak_memory_bytes: peak_memory,
        peak_memory_bytes_max: peak_memory_max,
        estimated_allocations,
        time_factor,
        output_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lossy_min_estimate_m4() {
        // Test min estimate against gradient measurement (heaptrack, libwebp 1.5)
        // Gradient is the best case for lossy encoding
        // 1024x1024 method 4: measured 14.01 MB with gradient
        let est = estimate_encode(1024, 1024, 4, &EncoderConfig::default());
        let measured = 14_010_000u64;
        let error =
            (est.peak_memory_bytes_min as f64 - measured as f64).abs() / measured as f64;
        assert!(
            error < 0.10,
            "Lossy 1024x1024 m4 min: estimated {} vs measured {}, error {:.1}%",
            est.peak_memory_bytes_min,
            measured,
            error * 100.0
        );
    }

    #[test]
    fn test_lossy_min_estimate_m0() {
        // 1024x1024 method 0: measured 13.52 MB with gradient
        let est = estimate_encode(1024, 1024, 4, &EncoderConfig::default().method(0));
        let measured = 13_520_000u64;
        let error =
            (est.peak_memory_bytes_min as f64 - measured as f64).abs() / measured as f64;
        assert!(
            error < 0.10,
            "Lossy 1024x1024 m0 min: estimated {} vs measured {}, error {:.1}%",
            est.peak_memory_bytes_min,
            measured,
            error * 100.0
        );
    }

    #[test]
    fn test_lossy_typical_is_higher_than_min() {
        // Typical estimate (for real photos) should be ~1.2x of min
        let est = estimate_encode(1024, 1024, 4, &EncoderConfig::default());
        let ratio = est.peak_memory_bytes as f64 / est.peak_memory_bytes_min as f64;
        assert!(
            (ratio - 1.2).abs() < 0.05,
            "Expected typ/min ratio ~1.2, got {}",
            ratio
        );
    }

    #[test]
    fn test_lossless_min_estimate_m4() {
        // For lossless, gradient is mid-range (solid is min)
        // Test that gradient falls between min and typ
        // 1024x1024 method 4: measured 35.54 MB with gradient
        let est = estimate_encode(
            1024,
            1024,
            4,
            &EncoderConfig::default().lossless(true).method(4),
        );
        let measured = 35_540_000u64;
        // Gradient should be close to typ (which is 1.2x of the gradient-based formula)
        // Actually for lossless, min is 0.6x gradient, so gradient is at the 1.0x point
        // typ is 1.2x, so gradient should be less than typ
        assert!(
            est.peak_memory_bytes_min < measured && measured < est.peak_memory_bytes,
            "Lossless gradient should fall between min ({}) and typ ({}), was {}",
            est.peak_memory_bytes_min,
            est.peak_memory_bytes,
            measured
        );
    }

    #[test]
    fn test_lossless_min_estimate_m0() {
        // 1024x1024 method 0: measured 24.51 MB with gradient
        let est = estimate_encode(
            1024,
            1024,
            4,
            &EncoderConfig::default().lossless(true).method(0),
        );
        let measured = 24_510_000u64;
        // Gradient should fall between min (0.6x) and typ (1.2x of gradient)
        assert!(
            est.peak_memory_bytes_min < measured && measured < est.peak_memory_bytes,
            "Lossless gradient should fall between min ({}) and typ ({}), was {}",
            est.peak_memory_bytes_min,
            est.peak_memory_bytes,
            measured
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

        // Zero-copy should save the output buffer size (for lossy)
        let output_size = 1024 * 1024 * 4;
        assert!(normal.peak_memory_bytes - zerocopy.peak_memory_bytes >= output_size / 2);
    }

    #[test]
    fn test_decode_estimate_accuracy() {
        // Test against heaptrack measurements (libwebp 1.5, gradient images)
        // Measured: 1024x1024 lossy decode = 15.81 MB
        let lossy = estimate_decode(1024, 1024, 4, false);
        let measured_lossy = 15_810_000u64;
        let error_lossy =
            (lossy.peak_memory_bytes as f64 - measured_lossy as f64).abs() / measured_lossy as f64;
        assert!(
            error_lossy < 0.10,
            "Lossy decode 1024x1024: estimated {} vs measured {}, error {:.1}%",
            lossy.peak_memory_bytes,
            measured_lossy,
            error_lossy * 100.0
        );

        // Measured: 1024x1024 lossless decode = 15.91 MB
        let lossless = estimate_decode(1024, 1024, 4, true);
        let measured_lossless = 15_910_000u64;
        let error_lossless = (lossless.peak_memory_bytes as f64 - measured_lossless as f64).abs()
            / measured_lossless as f64;
        assert!(
            error_lossless < 0.10,
            "Lossless decode 1024x1024: estimated {} vs measured {}, error {:.1}%",
            lossless.peak_memory_bytes,
            measured_lossless,
            error_lossless * 100.0
        );
    }

    #[test]
    fn test_decode_min_max_range() {
        // Decode min/max should have ~5% variation (much less than encode)
        let est = estimate_decode(1024, 1024, 4, false);
        let ratio = est.peak_memory_bytes_max as f64 / est.peak_memory_bytes_min as f64;
        assert!(
            (ratio - 1.05).abs() < 0.01,
            "Expected max/min ratio ~1.05, got {}",
            ratio
        );
    }
}
