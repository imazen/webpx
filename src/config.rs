//! Encoder and decoder configuration types.

use crate::error::{Error, Result};
use alloc::vec::Vec;

/// Content-aware encoding presets.
///
/// These presets configure the encoder for different types of content,
/// optimizing the balance between file size and visual quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
/// use webpx::EncoderConfig;
///
/// let config = EncoderConfig::new()
///     .quality(85.0)
///     .preset(webpx::Preset::Photo)
///     .method(4);
///
/// // Reuse config for multiple images
/// let image1 = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA
/// let image2 = vec![0u8; 8 * 6 * 4]; // 8x6 RGBA
/// let webp1 = config.encode_rgba(&image1, 4, 4)?;
/// let webp2 = config.encode_rgba(&image2, 8, 6)?;
/// # Ok::<(), webpx::Error>(())
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

    /// Encode RGBA pixel data to WebP.
    ///
    /// # Arguments
    /// - `data`: RGBA pixel data (4 bytes per pixel)
    /// - `width`: Image width in pixels
    /// - `height`: Image height in pixels
    ///
    /// # Example
    /// ```rust
    /// use webpx::EncoderConfig;
    ///
    /// let rgba_data = vec![0u8; 4 * 4 * 4]; // 4x4 RGBA
    /// let config = EncoderConfig::new().quality(85.0);
    /// let webp = config.encode_rgba(&rgba_data, 4, 4)?;
    /// # Ok::<(), webpx::Error>(())
    /// ```
    pub fn encode_rgba(&self, data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
        crate::encode::encode_with_config(data, width, height, 4, self)
    }

    /// Encode RGB pixel data to WebP (no alpha).
    ///
    /// # Arguments
    /// - `data`: RGB pixel data (3 bytes per pixel)
    /// - `width`: Image width in pixels
    /// - `height`: Image height in pixels
    pub fn encode_rgb(&self, data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
        crate::encode::encode_with_config(data, width, height, 3, self)
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
                .map_err(|_| Error::InvalidConfig("failed to initialize config".into()))?;

        config.lossless = self.lossless as i32;
        config.method = self.method as i32;
        config.near_lossless = self.near_lossless as i32;
        config.alpha_quality = self.alpha_quality as i32;
        config.alpha_compression = self.alpha_compression as i32;
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

        // Validate the config
        if unsafe { libwebp_sys::WebPValidateConfig(&config) } == 0 {
            return Err(Error::InvalidConfig("config validation failed".into()));
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
