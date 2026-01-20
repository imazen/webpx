//! Encoder and decoder configuration types.

use crate::error::{Error, Result};
use crate::types::{EncodePixel, PixelLayout};
use alloc::vec::Vec;
use enough::Stop;
use whereat::*;

/// Content-aware encoding presets.
///
/// These presets configure the encoder for different types of content,
/// optimizing the balance between file size and visual quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
#[repr(i32)]
pub enum Preset {
    /// Default preset, balanced for general use.
    #[default]
    Default = 0,
    /// Digital picture (portrait, indoor shot).
    /// Optimizes for smooth skin tones and indoor lighting.
    Picture = 1,
    /// Outdoor photograph with natural lighting.
    /// Best for landscapes, nature, and outdoor scenes.
    Photo = 2,
    /// Hand or line drawing with high-contrast details.
    /// Preserves sharp edges and fine lines.
    Drawing = 3,
    /// Small-sized colorful images like icons or sprites.
    /// Optimizes for small dimensions and sharp edges.
    Icon = 4,
    /// Text-heavy images.
    /// Preserves text readability and sharp character edges.
    Text = 5,
}

/// Hint about image content for encoder optimization.
///
/// Unlike [`Preset`], which configures initial encoding parameters,
/// hints guide the encoder's internal decisions during compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
#[repr(u32)]
pub enum ImageHint {
    /// No specific hint, use default heuristics.
    #[default]
    Default = 0,
    /// Indoor digital picture (portrait, indoor shot).
    Picture = 1,
    /// Outdoor photograph with natural lighting.
    Photo = 2,
    /// Discrete tone image (graph, map, etc.).
    Graph = 3,
}

impl ImageHint {
    pub(crate) fn to_libwebp(self) -> libwebp_sys::WebPImageHint {
        match self {
            ImageHint::Default => libwebp_sys::WebPImageHint::WEBP_HINT_DEFAULT,
            ImageHint::Picture => libwebp_sys::WebPImageHint::WEBP_HINT_PICTURE,
            ImageHint::Photo => libwebp_sys::WebPImageHint::WEBP_HINT_PHOTO,
            ImageHint::Graph => libwebp_sys::WebPImageHint::WEBP_HINT_GRAPH,
        }
    }
}

/// Alpha channel filtering method.
///
/// Controls how the alpha plane is filtered during compression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
#[repr(i32)]
pub enum AlphaFilter {
    /// No filtering.
    None = 0,
    /// Fast filtering (predictive).
    #[default]
    Fast = 1,
    /// Best filtering (slower but better compression).
    Best = 2,
}

/// Encoding statistics returned after compression.
///
/// Provides detailed information about the encoding process,
/// useful for analysis, debugging, and optimization.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct EncodeStats {
    /// Encoded file size in bytes.
    pub coded_size: u32,
    /// PSNR values: [Y, U, V, Alpha, All].
    pub psnr: [f32; 5],
    /// Number of macroblocks in each partition [0-2].
    pub block_count: [u32; 3],
    /// Header size in bytes [lossless, lossy].
    pub header_bytes: [u32; 2],
    /// Size of each segment in bytes.
    pub segment_size: [u32; 4],
    /// Quantizer value for each segment.
    pub segment_quant: [u32; 4],
    /// Filter level for each segment.
    pub segment_level: [u32; 4],
    /// Size of alpha data in bytes.
    pub alpha_data_size: u32,
    /// For lossless: histogram bits used.
    pub histogram_bits: u32,
    /// For lossless: transform bits used.
    pub transform_bits: u32,
    /// For lossless: cache bits used.
    pub cache_bits: u32,
    /// For lossless: palette size (0 = no palette).
    pub palette_size: u32,
    /// For lossless: total compressed size.
    pub lossless_size: u32,
    /// For lossless: header size.
    pub lossless_hdr_size: u32,
    /// For lossless: data size.
    pub lossless_data_size: u32,
}

impl EncodeStats {
    /// Create from libwebp WebPAuxStats.
    pub(crate) fn from_libwebp(stats: &libwebp_sys::WebPAuxStats) -> Self {
        Self {
            coded_size: stats.coded_size as u32,
            psnr: stats.PSNR,
            block_count: [
                stats.block_count[0] as u32,
                stats.block_count[1] as u32,
                stats.block_count[2] as u32,
            ],
            header_bytes: [stats.header_bytes[0] as u32, stats.header_bytes[1] as u32],
            segment_size: [
                stats.segment_size[0] as u32,
                stats.segment_size[1] as u32,
                stats.segment_size[2] as u32,
                stats.segment_size[3] as u32,
            ],
            segment_quant: [
                stats.segment_quant[0] as u32,
                stats.segment_quant[1] as u32,
                stats.segment_quant[2] as u32,
                stats.segment_quant[3] as u32,
            ],
            segment_level: [
                stats.segment_level[0] as u32,
                stats.segment_level[1] as u32,
                stats.segment_level[2] as u32,
                stats.segment_level[3] as u32,
            ],
            alpha_data_size: stats.alpha_data_size as u32,
            histogram_bits: stats.histogram_bits as u32,
            transform_bits: stats.transform_bits as u32,
            cache_bits: stats.cache_bits as u32,
            palette_size: stats.palette_size as u32,
            lossless_size: stats.lossless_size as u32,
            lossless_hdr_size: stats.lossless_hdr_size as u32,
            lossless_data_size: stats.lossless_data_size as u32,
        }
    }
}

impl Preset {
    /// Convert to libwebp preset value.
    pub(crate) fn to_libwebp(self) -> libwebp_sys::WebPPreset {
        match self {
            Preset::Default => libwebp_sys::WebPPreset::WEBP_PRESET_DEFAULT,
            Preset::Picture => libwebp_sys::WebPPreset::WEBP_PRESET_PICTURE,
            Preset::Photo => libwebp_sys::WebPPreset::WEBP_PRESET_PHOTO,
            Preset::Drawing => libwebp_sys::WebPPreset::WEBP_PRESET_DRAWING,
            Preset::Icon => libwebp_sys::WebPPreset::WEBP_PRESET_ICON,
            Preset::Text => libwebp_sys::WebPPreset::WEBP_PRESET_TEXT,
        }
    }
}

/// WebP encoder configuration. Dimension-independent, reusable across images.
///
/// Use the builder pattern to configure encoding options, then call one of
/// the `encode_*` methods to create an encoder.
///
/// # Example
///
/// ```rust
/// use webpx::{EncoderConfig, Unstoppable};
///
/// let config = EncoderConfig::new()
///     .quality(85.0)
///     .preset(webpx::Preset::Photo)
///     .method(4);
///
/// // Reuse config for multiple images
/// let image1 = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA
/// let image2 = vec![0u8; 8 * 6 * 4]; // 8x6 RGBA
/// let webp1 = config.encode_rgba(&image1, 4, 4, Unstoppable)?;
/// let webp2 = config.encode_rgba(&image2, 8, 6, Unstoppable)?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub(crate) quality: f32,
    pub(crate) preset: Preset,
    pub(crate) lossless: bool,
    pub(crate) method: u8,
    pub(crate) near_lossless: u8,
    pub(crate) alpha_quality: u8,
    pub(crate) alpha_compression: bool,
    pub(crate) alpha_filter: AlphaFilter,
    pub(crate) exact: bool,
    pub(crate) target_size: u32,
    pub(crate) target_psnr: f32,
    pub(crate) sns_strength: u8,
    pub(crate) filter_strength: u8,
    pub(crate) filter_sharpness: u8,
    pub(crate) filter_type: u8,
    pub(crate) autofilter: bool,
    pub(crate) pass: u8,
    pub(crate) segments: u8,
    pub(crate) use_sharp_yuv: bool,
    pub(crate) thread_level: u8,
    pub(crate) low_memory: bool,
    // New compression options
    pub(crate) hint: ImageHint,
    pub(crate) preprocessing: u8,
    pub(crate) partitions: u8,
    pub(crate) partition_limit: u8,
    pub(crate) delta_palette: bool,
    pub(crate) qmin: u8,
    pub(crate) qmax: u8,
    #[cfg(feature = "icc")]
    pub(crate) icc_profile: Option<Vec<u8>>,
    #[cfg(feature = "icc")]
    pub(crate) exif_data: Option<Vec<u8>>,
    #[cfg(feature = "icc")]
    pub(crate) xmp_data: Option<Vec<u8>>,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            quality: 75.0,
            preset: Preset::Default,
            lossless: false,
            method: 4,
            near_lossless: 100,
            alpha_quality: 100,
            alpha_compression: true,
            alpha_filter: AlphaFilter::Fast,
            exact: false,
            target_size: 0,
            target_psnr: 0.0,
            sns_strength: 50,
            filter_strength: 60,
            filter_sharpness: 0,
            filter_type: 1,
            autofilter: false,
            pass: 1,
            segments: 4,
            use_sharp_yuv: false,
            thread_level: 0,
            low_memory: false,
            hint: ImageHint::Default,
            preprocessing: 0,
            partitions: 0,
            partition_limit: 0,
            delta_palette: false,
            qmin: 0,
            qmax: 100,
            #[cfg(feature = "icc")]
            icc_profile: None,
            #[cfg(feature = "icc")]
            exif_data: None,
            #[cfg(feature = "icc")]
            xmp_data: None,
        }
    }
}

impl EncoderConfig {
    // === Constructors ===

    /// Create a new encoder configuration with default settings.
    ///
    /// Default: lossy encoding at quality 75, method 4, no preset.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a lossless encoder configuration.
    ///
    /// Lossless mode preserves all pixel values exactly. The quality
    /// parameter controls compression effort (higher = slower but smaller).
    #[must_use]
    pub fn new_lossless() -> Self {
        Self {
            lossless: true,
            quality: 75.0,
            alpha_compression: false,
            ..Self::default()
        }
    }

    /// Create a lossless encoder with a compression level (0-9).
    ///
    /// This uses libwebp's lossless presets which configure optimal
    /// settings for each compression level:
    /// - 0: Fastest, largest files
    /// - 9: Slowest, smallest files (maximum compression)
    ///
    /// Level 6 is a good balance of speed and compression.
    #[must_use]
    pub fn new_lossless_level(level: u8) -> Self {
        let level = level.min(9);
        // Map lossless level to method and quality
        // Based on WebPConfigLosslessPreset behavior
        let method = match level {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 3,
            5 => 4,
            6 => 4,
            7 => 4,
            8 => 5,
            _ => 6,
        };
        let quality = match level {
            0..=4 => 25.0 + (level as f32) * 15.0,
            _ => 80.0 + ((level - 5) as f32) * 4.0,
        };

        Self {
            lossless: true,
            method,
            quality,
            alpha_compression: false,
            ..Self::default()
        }
    }

    /// Create a configuration with preset and quality.
    ///
    /// Presets configure optimal settings for different content types.
    #[must_use]
    pub fn with_preset(preset: Preset, quality: f32) -> Self {
        Self {
            preset,
            quality,
            ..Self::default()
        }
    }

    /// Create a maximum compression configuration.
    ///
    /// Configures all options for smallest possible file size.
    /// Encoding will be slow but produces optimal compression.
    #[must_use]
    pub fn max_compression() -> Self {
        Self {
            quality: 90.0,
            method: 6,
            pass: 10,
            segments: 4,
            sns_strength: 100,
            autofilter: true,
            use_sharp_yuv: true,
            partition_limit: 100,
            ..Self::default()
        }
    }

    /// Create a maximum compression lossless configuration.
    ///
    /// Uses all available techniques for smallest lossless output.
    #[must_use]
    pub fn max_compression_lossless() -> Self {
        Self {
            lossless: true,
            quality: 100.0,
            method: 6,
            near_lossless: 100,
            delta_palette: true,
            alpha_compression: false,
            ..Self::default()
        }
    }

    // === Quality & Compression ===

    /// Set encoding quality (0.0 = smallest, 100.0 = best).
    ///
    /// For lossy: controls size/quality tradeoff.
    /// For lossless: controls compression effort (100 = maximum compression).
    #[must_use]
    pub fn quality(mut self, quality: f32) -> Self {
        self.quality = quality.clamp(0.0, 100.0);
        self
    }

    /// Set content-aware preset.
    ///
    /// Presets configure optimal settings for different content types:
    /// - `Photo`: outdoor photographs, landscapes
    /// - `Picture`: indoor photos, portraits
    /// - `Drawing`: line art, high contrast
    /// - `Icon`: small colorful images
    /// - `Text`: text-heavy images
    #[must_use]
    pub fn preset(mut self, preset: Preset) -> Self {
        self.preset = preset;
        self
    }

    /// Enable or disable lossless compression.
    ///
    /// Lossless mode preserves all pixel values exactly.
    #[must_use]
    pub fn lossless(mut self, lossless: bool) -> Self {
        self.lossless = lossless;
        if lossless {
            self.alpha_compression = false;
        }
        self
    }

    /// Set quality/speed tradeoff (0 = fast, 6 = slower but better).
    ///
    /// Higher values produce smaller files at the cost of encoding time.
    #[must_use]
    pub fn method(mut self, method: u8) -> Self {
        self.method = method.min(6);
        self
    }

    /// Set near-lossless preprocessing (0 = max preprocessing, 100 = off).
    ///
    /// Only used when `lossless` is true. Lower values allow more
    /// preprocessing for better compression at slight quality cost.
    #[must_use]
    pub fn near_lossless(mut self, value: u8) -> Self {
        self.near_lossless = value.min(100);
        self
    }

    // === Alpha Channel ===

    /// Set alpha plane quality (0-100, default 100).
    #[must_use]
    pub fn alpha_quality(mut self, quality: u8) -> Self {
        self.alpha_quality = quality.min(100);
        self
    }

    /// Enable or disable alpha compression.
    ///
    /// When disabled, alpha is stored uncompressed.
    #[must_use]
    pub fn alpha_compression(mut self, enable: bool) -> Self {
        self.alpha_compression = enable;
        self
    }

    /// Set alpha filtering method.
    ///
    /// Controls how the alpha plane is filtered:
    /// - `None`: No filtering
    /// - `Fast`: Predictive filtering (default, good balance)
    /// - `Best`: Best compression (slower)
    #[must_use]
    pub fn alpha_filter(mut self, filter: AlphaFilter) -> Self {
        self.alpha_filter = filter;
        self
    }

    /// Preserve exact RGB values under transparent areas.
    ///
    /// By default, the encoder may modify RGB values where alpha is 0
    /// to improve compression. Enable this to preserve exact values.
    #[must_use]
    pub fn exact(mut self, exact: bool) -> Self {
        self.exact = exact;
        self
    }

    // === Target Size/Quality ===

    /// Set target file size in bytes (0 = disabled).
    ///
    /// When set, the encoder will adjust quality to meet the target size.
    /// Takes precedence over quality setting.
    #[must_use]
    pub fn target_size(mut self, size: u32) -> Self {
        self.target_size = size;
        self
    }

    /// Set target PSNR in dB (0.0 = disabled).
    ///
    /// Takes precedence over target_size if non-zero.
    #[must_use]
    pub fn target_psnr(mut self, psnr: f32) -> Self {
        self.target_psnr = psnr;
        self
    }

    // === Filtering ===

    /// Set spatial noise shaping strength (0-100, 0 = off).
    #[must_use]
    pub fn sns_strength(mut self, strength: u8) -> Self {
        self.sns_strength = strength.min(100);
        self
    }

    /// Set filter strength (0-100, 0 = off).
    #[must_use]
    pub fn filter_strength(mut self, strength: u8) -> Self {
        self.filter_strength = strength.min(100);
        self
    }

    /// Set filter sharpness (0-7, 0 = sharpest).
    #[must_use]
    pub fn filter_sharpness(mut self, sharpness: u8) -> Self {
        self.filter_sharpness = sharpness.min(7);
        self
    }

    /// Set filter type (0 = simple, 1 = strong).
    #[must_use]
    pub fn filter_type(mut self, filter_type: u8) -> Self {
        self.filter_type = filter_type.min(1);
        self
    }

    /// Enable auto-adjustment of filter strength.
    #[must_use]
    pub fn autofilter(mut self, enable: bool) -> Self {
        self.autofilter = enable;
        self
    }

    // === Advanced ===

    /// Set number of entropy analysis passes (1-10).
    #[must_use]
    pub fn pass(mut self, passes: u8) -> Self {
        self.pass = passes.clamp(1, 10);
        self
    }

    /// Set number of segments (1-4).
    #[must_use]
    pub fn segments(mut self, segments: u8) -> Self {
        self.segments = segments.clamp(1, 4);
        self
    }

    /// Use sharp YUV conversion (slower but better quality).
    ///
    /// Produces sharper color edges at the cost of encoding time.
    #[must_use]
    pub fn sharp_yuv(mut self, enable: bool) -> Self {
        self.use_sharp_yuv = enable;
        self
    }

    /// Set thread level for multi-threaded encoding.
    #[must_use]
    pub fn thread_level(mut self, level: u8) -> Self {
        self.thread_level = level;
        self
    }

    /// Reduce memory usage at cost of CPU.
    #[must_use]
    pub fn low_memory(mut self, enable: bool) -> Self {
        self.low_memory = enable;
        self
    }

    // === Content Hints ===

    /// Set image content hint for encoder optimization.
    ///
    /// Hints guide the encoder's internal compression decisions:
    /// - `Default`: No specific optimization
    /// - `Picture`: Indoor digital pictures, portraits
    /// - `Photo`: Outdoor photographs, landscapes
    /// - `Graph`: Discrete tone images (diagrams, maps, charts)
    ///
    /// This complements [`preset`](Self::preset) - preset sets initial parameters,
    /// while hint guides runtime decisions.
    #[must_use]
    pub fn hint(mut self, hint: ImageHint) -> Self {
        self.hint = hint;
        self
    }

    // === Advanced Compression ===

    /// Set preprocessing filter (0-7).
    ///
    /// Applies filtering before encoding:
    /// - Bit 0: Pseudo-random dithering (helps gradients)
    /// - Bit 1: Segment-smooth
    /// - Bit 2: Additional filtering
    ///
    /// Higher values may improve compression for images with gradients.
    #[must_use]
    pub fn preprocessing(mut self, level: u8) -> Self {
        self.preprocessing = level.min(7);
        self
    }

    /// Set number of token partitions (0-3).
    ///
    /// Controls parallelism in the bitstream:
    /// - 0 = 1 partition
    /// - 1 = 2 partitions
    /// - 2 = 4 partitions
    /// - 3 = 8 partitions
    ///
    /// More partitions enable parallel decoding but may reduce compression.
    #[must_use]
    pub fn partitions(mut self, log2_partitions: u8) -> Self {
        self.partitions = log2_partitions.min(3);
        self
    }

    /// Set partition size limit (0-100).
    ///
    /// Controls quality degradation allowed to fit partitions within
    /// the 512k limit. Higher values allow more degradation for smaller
    /// partitions, which can help streaming/parallel decode.
    ///
    /// 0 = no degradation allowed, 100 = full degradation allowed.
    #[must_use]
    pub fn partition_limit(mut self, limit: u8) -> Self {
        self.partition_limit = limit.min(100);
        self
    }

    /// Enable delta palette encoding for lossless mode.
    ///
    /// Uses delta encoding for palette entries, which can significantly
    /// improve compression for images with smooth color gradients or
    /// palette-based content. Only effective in lossless mode.
    #[must_use]
    pub fn delta_palette(mut self, enable: bool) -> Self {
        self.delta_palette = enable;
        self
    }

    /// Set quantizer range (min, max) for lossy encoding.
    ///
    /// Constrains the quantizer values used during encoding:
    /// - `min`: Minimum quantizer (0-100, lower = higher quality floor)
    /// - `max`: Maximum quantizer (0-100, lower = higher quality ceiling)
    ///
    /// Useful for ensuring consistent quality across images.
    /// Default is (0, 100) = full range.
    #[must_use]
    pub fn quality_range(mut self, min: u8, max: u8) -> Self {
        self.qmin = min.min(100);
        self.qmax = max.min(100).max(self.qmin);
        self
    }

    // === Metadata (ICC feature) ===

    /// Attach an ICC color profile to the output.
    #[cfg(feature = "icc")]
    #[must_use]
    pub fn icc_profile(mut self, profile: impl Into<Vec<u8>>) -> Self {
        self.icc_profile = Some(profile.into());
        self
    }

    /// Attach EXIF metadata to the output.
    #[cfg(feature = "icc")]
    #[must_use]
    pub fn exif(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.exif_data = Some(data.into());
        self
    }

    /// Attach XMP metadata to the output.
    #[cfg(feature = "icc")]
    #[must_use]
    pub fn xmp(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.xmp_data = Some(data.into());
        self
    }

    // === Encoding Entry Points ===

    /// Encode typed pixel data to WebP.
    ///
    /// This is the preferred method for type-safe encoding with rgb crate types.
    /// The pixel format is determined at compile time from the type parameter.
    ///
    /// # Supported Types
    /// - [`rgb::RGBA8`] - 4-channel RGBA
    /// - [`rgb::RGB8`] - 3-channel RGB
    /// - [`rgb::alt::BGRA8`] - 4-channel BGRA (Windows/GPU native)
    /// - [`rgb::alt::BGR8`] - 3-channel BGR (OpenCV)
    ///
    /// # Arguments
    /// - `pixels`: Slice of typed pixels
    /// - `width`: Image width in pixels
    /// - `height`: Image height in pixels
    /// - `stop`: Cooperative cancellation token (use [`Unstoppable`](crate::Unstoppable) if not needed)
    ///
    /// # Example
    /// ```rust
    /// use webpx::{EncoderConfig, Unstoppable};
    /// use rgb::RGBA8;
    ///
    /// let pixels: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 4 * 4];
    /// let config = EncoderConfig::new().quality(85.0);
    /// let webp = config.encode(&pixels, 4, 4, Unstoppable)?;
    /// # Ok::<(), webpx::At<webpx::Error>>(())
    /// ```
    pub fn encode<P: EncodePixel>(
        &self,
        pixels: &[P],
        width: u32,
        height: u32,
        stop: impl Stop,
    ) -> Result<Vec<u8>> {
        let bpp = P::LAYOUT.bytes_per_pixel();
        let data = unsafe {
            core::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * bpp)
        };
        self.encode_internal(data, width, height, P::LAYOUT, stop)
    }

    /// Encode RGBA byte data to WebP.
    ///
    /// # Arguments
    /// - `data`: RGBA pixel data (4 bytes per pixel: red, green, blue, alpha)
    /// - `width`: Image width in pixels
    /// - `height`: Image height in pixels
    /// - `stop`: Cooperative cancellation token (use [`Unstoppable`](crate::Unstoppable) if not needed)
    ///
    /// # Example
    /// ```rust
    /// use webpx::{EncoderConfig, Unstoppable};
    ///
    /// let rgba_data = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA
    /// let config = EncoderConfig::new().quality(85.0);
    /// let webp = config.encode_rgba(&rgba_data, 4, 4, Unstoppable)?;
    /// # Ok::<(), webpx::At<webpx::Error>>(())
    /// ```
    pub fn encode_rgba(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        stop: impl Stop,
    ) -> Result<Vec<u8>> {
        self.encode_internal(data, width, height, PixelLayout::Rgba, stop)
    }

    /// Encode RGB byte data to WebP (no alpha).
    ///
    /// # Arguments
    /// - `data`: RGB pixel data (3 bytes per pixel: red, green, blue)
    /// - `width`: Image width in pixels
    /// - `height`: Image height in pixels
    /// - `stop`: Cooperative cancellation token (use [`Unstoppable`](crate::Unstoppable) if not needed)
    pub fn encode_rgb(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        stop: impl Stop,
    ) -> Result<Vec<u8>> {
        self.encode_internal(data, width, height, PixelLayout::Rgb, stop)
    }

    /// Encode BGRA byte data to WebP.
    ///
    /// BGRA is the native format on Windows and some GPU APIs.
    ///
    /// # Arguments
    /// - `data`: BGRA pixel data (4 bytes per pixel: blue, green, red, alpha)
    /// - `width`: Image width in pixels
    /// - `height`: Image height in pixels
    /// - `stop`: Cooperative cancellation token (use [`Unstoppable`](crate::Unstoppable) if not needed)
    pub fn encode_bgra(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        stop: impl Stop,
    ) -> Result<Vec<u8>> {
        self.encode_internal(data, width, height, PixelLayout::Bgra, stop)
    }

    /// Encode BGR byte data to WebP (no alpha).
    ///
    /// BGR is common in OpenCV and some image libraries.
    ///
    /// # Arguments
    /// - `data`: BGR pixel data (3 bytes per pixel: blue, green, red)
    /// - `width`: Image width in pixels
    /// - `height`: Image height in pixels
    /// - `stop`: Cooperative cancellation token (use [`Unstoppable`](crate::Unstoppable) if not needed)
    pub fn encode_bgr(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        stop: impl Stop,
    ) -> Result<Vec<u8>> {
        self.encode_internal(data, width, height, PixelLayout::Bgr, stop)
    }

    /// Encode RGBA pixel data and return encoding statistics.
    ///
    /// Returns both the encoded WebP data and detailed encoding statistics
    /// including PSNR, segment info, and compression metrics.
    pub fn encode_rgba_with_stats(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(Vec<u8>, EncodeStats)> {
        crate::encode::encode_with_config_stats(data, width, height, 4, self)
    }

    /// Encode RGB pixel data and return encoding statistics.
    pub fn encode_rgb_with_stats(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(Vec<u8>, EncodeStats)> {
        crate::encode::encode_with_config_stats(data, width, height, 3, self)
    }

    /// Internal: encode bytes with a specific pixel layout.
    fn encode_internal(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        layout: PixelLayout,
        stop: impl Stop,
    ) -> Result<Vec<u8>> {
        match layout {
            PixelLayout::Rgba => {
                crate::encode::encode_with_config_stoppable(data, width, height, 4, self, &stop)
            }
            PixelLayout::Rgb => {
                crate::encode::encode_with_config_stoppable(data, width, height, 3, self, &stop)
            }
            PixelLayout::Bgra => crate::Encoder::new_bgra(data, width, height)
                .config(self.clone())
                .encode(stop),
            PixelLayout::Bgr => crate::Encoder::new_bgr(data, width, height)
                .config(self.clone())
                .encode(stop),
        }
    }

    // === Validation ===

    /// Validate the configuration.
    pub fn validate(&self) -> Result<()> {
        // Validation is done in to_libwebp
        let _ = self.to_libwebp()?;
        Ok(())
    }

    /// Convert to libwebp WebPConfig.
    pub(crate) fn to_libwebp(&self) -> Result<libwebp_sys::WebPConfig> {
        let mut config =
            libwebp_sys::WebPConfig::new_with_preset(self.preset.to_libwebp(), self.quality)
                .map_err(|_| at!(Error::InvalidConfig("failed to initialize config".into())))?;

        config.lossless = self.lossless as i32;
        config.method = self.method as i32;
        config.near_lossless = self.near_lossless as i32;
        config.alpha_quality = self.alpha_quality as i32;
        config.alpha_compression = self.alpha_compression as i32;
        config.alpha_filtering = self.alpha_filter as i32;
        config.exact = self.exact as i32;
        config.target_size = self.target_size as i32;
        config.target_PSNR = self.target_psnr;
        config.sns_strength = self.sns_strength as i32;
        config.filter_strength = self.filter_strength as i32;
        config.filter_sharpness = self.filter_sharpness as i32;
        config.filter_type = self.filter_type as i32;
        config.autofilter = self.autofilter as i32;
        config.pass = self.pass as i32;
        config.segments = self.segments as i32;
        config.use_sharp_yuv = self.use_sharp_yuv as i32;
        config.thread_level = self.thread_level as i32;
        config.low_memory = self.low_memory as i32;
        // New compression options
        config.image_hint = self.hint.to_libwebp();
        config.preprocessing = self.preprocessing as i32;
        config.partitions = self.partitions as i32;
        config.partition_limit = self.partition_limit as i32;
        config.use_delta_palette = self.delta_palette as i32;
        config.qmin = self.qmin as i32;
        config.qmax = self.qmax as i32;

        // Validate the config
        if unsafe { libwebp_sys::WebPValidateConfig(&config) } == 0 {
            return Err(at!(Error::InvalidConfig("config validation failed".into())));
        }

        Ok(config)
    }

    // === Accessors (read-only) ===

    /// Get the quality setting.
    #[must_use]
    pub fn get_quality(&self) -> f32 {
        self.quality
    }

    /// Get the preset.
    #[must_use]
    pub fn get_preset(&self) -> Preset {
        self.preset
    }

    /// Check if lossless mode is enabled.
    #[must_use]
    pub fn is_lossless(&self) -> bool {
        self.lossless
    }

    /// Get the method (quality/speed tradeoff).
    #[must_use]
    pub fn get_method(&self) -> u8 {
        self.method
    }
}

/// Decoder configuration.
#[derive(Debug, Clone, Default)]
pub struct DecoderConfig {
    pub(crate) bypass_filtering: bool,
    pub(crate) no_fancy_upsampling: bool,
    pub(crate) use_cropping: bool,
    pub(crate) crop_left: u32,
    pub(crate) crop_top: u32,
    pub(crate) crop_width: u32,
    pub(crate) crop_height: u32,
    pub(crate) use_scaling: bool,
    pub(crate) scaled_width: u32,
    pub(crate) scaled_height: u32,
    pub(crate) use_threads: bool,
    pub(crate) flip: bool,
    pub(crate) alpha_dithering: u8,
}

impl DecoderConfig {
    /// Create a new decoder configuration with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bypass filtering for faster decoding.
    #[must_use]
    pub fn bypass_filtering(mut self, enable: bool) -> Self {
        self.bypass_filtering = enable;
        self
    }

    /// Disable fancy upsampling.
    #[must_use]
    pub fn no_fancy_upsampling(mut self, enable: bool) -> Self {
        self.no_fancy_upsampling = enable;
        self
    }

    /// Set crop region.
    #[must_use]
    pub fn crop(mut self, left: u32, top: u32, width: u32, height: u32) -> Self {
        self.use_cropping = true;
        self.crop_left = left;
        self.crop_top = top;
        self.crop_width = width;
        self.crop_height = height;
        self
    }

    /// Set scaled output dimensions.
    #[must_use]
    pub fn scale(mut self, width: u32, height: u32) -> Self {
        self.use_scaling = true;
        self.scaled_width = width;
        self.scaled_height = height;
        self
    }

    /// Enable multi-threaded decoding.
    #[must_use]
    pub fn use_threads(mut self, enable: bool) -> Self {
        self.use_threads = enable;
        self
    }

    /// Flip output vertically.
    #[must_use]
    pub fn flip(mut self, enable: bool) -> Self {
        self.flip = enable;
        self
    }

    /// Set alpha dithering strength (0-100).
    #[must_use]
    pub fn alpha_dithering(mut self, strength: u8) -> Self {
        self.alpha_dithering = strength.min(100);
        self
    }
}
