//! Encoder and decoder configuration types.

use crate::error::{Error, Result};

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

/// Encoder configuration.
///
/// Controls all aspects of WebP encoding including quality, compression method,
/// and advanced options.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    /// Quality factor (0.0 = smallest, 100.0 = best quality).
    /// For lossy: controls size/quality tradeoff.
    /// For lossless: controls compression effort (100 = maximum compression).
    pub quality: f32,

    /// Content-aware preset.
    pub preset: Preset,

    /// Enable lossless compression.
    pub lossless: bool,

    /// Quality/speed tradeoff (0 = fast, 6 = slower but better).
    pub method: u8,

    /// Near-lossless preprocessing (0 = max preprocessing, 100 = off).
    /// Only used when `lossless` is true.
    pub near_lossless: u8,

    /// Alpha plane quality (0-100, default 100).
    pub alpha_quality: u8,

    /// Alpha compression method (0 = none, 1 = lossless).
    pub alpha_compression: bool,

    /// Preserve exact RGB values under transparent areas.
    pub exact: bool,

    /// Target file size in bytes (0 = disabled).
    /// Takes precedence over quality if non-zero.
    pub target_size: u32,

    /// Target PSNR in dB (0.0 = disabled).
    /// Takes precedence over target_size if non-zero.
    pub target_psnr: f32,

    /// Spatial noise shaping strength (0-100, 0 = off).
    pub sns_strength: u8,

    /// Filter strength (0-100, 0 = off).
    pub filter_strength: u8,

    /// Filter sharpness (0-7, 0 = sharpest).
    pub filter_sharpness: u8,

    /// Filter type (0 = simple, 1 = strong).
    pub filter_type: u8,

    /// Auto-adjust filter strength.
    pub autofilter: bool,

    /// Number of entropy analysis passes (1-10).
    pub pass: u8,

    /// Number of segments (1-4).
    pub segments: u8,

    /// Use sharp YUV conversion (slower but better quality).
    pub use_sharp_yuv: bool,

    /// Multi-threaded encoding.
    pub thread_level: u8,

    /// Reduce memory usage at cost of CPU.
    pub low_memory: bool,
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
        }
    }
}

impl EncoderConfig {
    /// Create a new encoder configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a lossless encoder configuration.
    pub fn lossless() -> Self {
        Self {
            lossless: true,
            quality: 75.0, // compression effort for lossless
            alpha_compression: false,
            ..Self::default()
        }
    }

    /// Create a configuration from a preset and quality.
    pub fn with_preset(preset: Preset, quality: f32) -> Self {
        Self {
            preset,
            quality,
            ..Self::default()
        }
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
}

/// Decoder configuration.
#[derive(Debug, Clone, Default)]
pub struct DecoderConfig {
    /// Bypass filtering (useful for inspection).
    pub bypass_filtering: bool,
    /// Don't use multi-threading.
    pub no_fancy_upsampling: bool,
    /// Use cropping (set crop_* fields).
    pub use_cropping: bool,
    /// Crop left offset.
    pub crop_left: u32,
    /// Crop top offset.
    pub crop_top: u32,
    /// Crop width.
    pub crop_width: u32,
    /// Crop height.
    pub crop_height: u32,
    /// Use scaling (set scaled_* fields).
    pub use_scaling: bool,
    /// Scaled width.
    pub scaled_width: u32,
    /// Scaled height.
    pub scaled_height: u32,
    /// Use multi-threading.
    pub use_threads: bool,
    /// Flip output vertically.
    pub flip: bool,
    /// Alpha dithering strength (0-100).
    pub alpha_dithering: u8,
}

impl DecoderConfig {
    /// Create a new decoder configuration with default settings.
    pub fn new() -> Self {
        Self::default()
    }
}
