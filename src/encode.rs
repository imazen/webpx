//! WebP encoding functionality.

use whereat::*;
use crate::config::{EncodeStats, EncoderConfig, Preset};
use crate::error::{EncodingError, Error, Result};
use crate::types::YuvPlanesRef;
use alloc::vec::Vec;
use enough::Stop;
use imgref::ImgRef;
use rgb::alt::{BGR8, BGRA8};
use rgb::{RGB8, RGBA8};

/// Context for progress hook callback.
struct StopContext<'a, S: Stop> {
    stop: &'a S,
}

/// Progress hook that checks the Stop trait.
///
/// Returns 1 to continue, 0 to abort.
extern "C" fn progress_hook<S: Stop>(
    _percent: core::ffi::c_int,
    picture: *const libwebp_sys::WebPPicture,
) -> core::ffi::c_int {
    // SAFETY: user_data is set to a valid StopContext pointer before encoding
    let ctx = unsafe { &*((*picture).user_data as *const StopContext<S>) };
    if ctx.stop.should_stop() {
        0 // abort
    } else {
        1 // continue
    }
}

/// Encode RGBA pixels to WebP.
///
/// # Arguments
///
/// * `data` - RGBA pixel data (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `quality` - Quality factor (0.0 = smallest, 100.0 = best)
/// * `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
///
/// # Example
///
/// ```rust,no_run
/// use webpx::Unstoppable;
///
/// let rgba: &[u8] = &[0u8; 640 * 480 * 4]; // placeholder
/// let webp = webpx::encode_rgba(rgba, 640, 480, 85.0, Unstoppable)?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub fn encode_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    quality: f32,
    stop: impl Stop,
) -> Result<Vec<u8>> {
    EncoderConfig::new()
        .quality(quality)
        .encode_rgba_stoppable(data, width, height, &stop)
}

/// Encode RGB pixels to WebP (no alpha).
///
/// # Arguments
///
/// * `data` - RGB pixel data (3 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `quality` - Quality factor (0.0 = smallest, 100.0 = best)
/// * `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
pub fn encode_rgb(
    data: &[u8],
    width: u32,
    height: u32,
    quality: f32,
    stop: impl Stop,
) -> Result<Vec<u8>> {
    EncoderConfig::new()
        .quality(quality)
        .encode_rgb_stoppable(data, width, height, &stop)
}

/// Encode BGRA pixels to WebP.
///
/// BGRA is the native format on Windows and some GPU APIs.
///
/// # Arguments
///
/// * `data` - BGRA pixel data (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `quality` - Quality factor (0.0 = smallest, 100.0 = best)
/// * `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
pub fn encode_bgra(
    data: &[u8],
    width: u32,
    height: u32,
    quality: f32,
    stop: impl Stop,
) -> Result<Vec<u8>> {
    Encoder::new_bgra(data, width, height)
        .quality(quality)
        .encode(stop)
}

/// Encode BGR pixels to WebP (no alpha).
///
/// BGR is common in OpenCV and some image libraries.
///
/// # Arguments
///
/// * `data` - BGR pixel data (3 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `quality` - Quality factor (0.0 = smallest, 100.0 = best)
/// * `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
pub fn encode_bgr(
    data: &[u8],
    width: u32,
    height: u32,
    quality: f32,
    stop: impl Stop,
) -> Result<Vec<u8>> {
    Encoder::new_bgr(data, width, height)
        .quality(quality)
        .encode(stop)
}

/// Encode to lossless WebP.
///
/// Lossless encoding preserves all pixel values exactly.
///
/// # Arguments
///
/// * `data` - RGBA pixel data (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
pub fn encode_lossless(data: &[u8], width: u32, height: u32, stop: impl Stop) -> Result<Vec<u8>> {
    EncoderConfig::new()
        .lossless(true)
        .encode_rgba_stoppable(data, width, height, &stop)
}

/// Internal: Encode with full config (called by EncoderConfig).
pub(crate) fn encode_with_config(
    data: &[u8],
    width: u32,
    height: u32,
    bpp: u8,
    config: &EncoderConfig,
) -> Result<Vec<u8>> {
    validate_dimensions(width, height)?;
    validate_buffer_size(data.len(), width, height, bpp as u32)?;

    let webp_config = config.to_libwebp()?;

    // Initialize picture
    let mut picture = libwebp_sys::WebPPicture::new()
        .map_err(|_| at!(Error::InvalidConfig("failed to init picture".into())))?;

    picture.width = width as i32;
    picture.height = height as i32;
    picture.use_argb = 1;

    // Import pixel data
    let import_ok = if bpp == 4 {
        unsafe {
            libwebp_sys::WebPPictureImportRGBA(&mut picture, data.as_ptr(), (width * 4) as i32)
        }
    } else {
        unsafe {
            libwebp_sys::WebPPictureImportRGB(&mut picture, data.as_ptr(), (width * 3) as i32)
        }
    };

    if import_ok == 0 {
        unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
        return Err(at!(Error::EncodeFailed(EncodingError::OutOfMemory)));
    }

    // Setup memory writer
    let mut writer = core::mem::MaybeUninit::<libwebp_sys::WebPMemoryWriter>::uninit();
    unsafe { libwebp_sys::WebPMemoryWriterInit(writer.as_mut_ptr()) };
    let mut writer = unsafe { writer.assume_init() };

    picture.writer = Some(libwebp_sys::WebPMemoryWrite);
    picture.custom_ptr = &mut writer as *mut _ as *mut _;

    // Encode
    let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, &mut picture) };

    let result = if ok == 0 {
        let error = EncodingError::from(picture.error_code as i32);
        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }
        Err(at!(Error::EncodeFailed(error)))
    } else {
        let webp_data = unsafe {
            let slice = core::slice::from_raw_parts(writer.mem, writer.size);
            slice.to_vec()
        };
        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }
        Ok(webp_data)
    };

    // Embed metadata if present
    #[cfg(feature = "icc")]
    if let Ok(mut webp_data) = result {
        if let Some(ref icc) = config.icc_profile {
            webp_data = crate::mux::embed_icc(&webp_data, icc)?;
        }
        if let Some(ref exif) = config.exif_data {
            webp_data = crate::mux::embed_exif(&webp_data, exif)?;
        }
        if let Some(ref xmp) = config.xmp_data {
            webp_data = crate::mux::embed_xmp(&webp_data, xmp)?;
        }
        return Ok(webp_data);
    }

    result
}

/// Internal: Encode with full config and return stats (called by EncoderConfig).
pub(crate) fn encode_with_config_stats(
    data: &[u8],
    width: u32,
    height: u32,
    bpp: u8,
    config: &EncoderConfig,
) -> Result<(Vec<u8>, EncodeStats)> {
    validate_dimensions(width, height)?;
    validate_buffer_size(data.len(), width, height, bpp as u32)?;

    let webp_config = config.to_libwebp()?;

    // Initialize picture
    let mut picture = libwebp_sys::WebPPicture::new()
        .map_err(|_| at!(Error::InvalidConfig("failed to init picture".into())))?;

    picture.width = width as i32;
    picture.height = height as i32;
    picture.use_argb = 1;

    // Initialize stats
    let mut stats = core::mem::MaybeUninit::<libwebp_sys::WebPAuxStats>::uninit();
    picture.stats = stats.as_mut_ptr();

    // Import pixel data
    let import_ok = if bpp == 4 {
        unsafe {
            libwebp_sys::WebPPictureImportRGBA(&mut picture, data.as_ptr(), (width * 4) as i32)
        }
    } else {
        unsafe {
            libwebp_sys::WebPPictureImportRGB(&mut picture, data.as_ptr(), (width * 3) as i32)
        }
    };

    if import_ok == 0 {
        unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
        return Err(at!(Error::EncodeFailed(EncodingError::OutOfMemory)));
    }

    // Setup memory writer
    let mut writer = core::mem::MaybeUninit::<libwebp_sys::WebPMemoryWriter>::uninit();
    unsafe { libwebp_sys::WebPMemoryWriterInit(writer.as_mut_ptr()) };
    let mut writer = unsafe { writer.assume_init() };

    picture.writer = Some(libwebp_sys::WebPMemoryWrite);
    picture.custom_ptr = &mut writer as *mut _ as *mut _;

    // Encode
    let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, &mut picture) };

    let result = if ok == 0 {
        let error = EncodingError::from(picture.error_code as i32);
        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }
        Err(at!(Error::EncodeFailed(error)))
    } else {
        let webp_data = unsafe {
            let slice = core::slice::from_raw_parts(writer.mem, writer.size);
            slice.to_vec()
        };
        let encode_stats = EncodeStats::from_libwebp(unsafe { &stats.assume_init() });
        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }
        Ok((webp_data, encode_stats))
    };

    // Embed metadata if present
    #[cfg(feature = "icc")]
    if let Ok((mut webp_data, stats)) = result {
        if let Some(ref icc) = config.icc_profile {
            webp_data = crate::mux::embed_icc(&webp_data, icc)?;
        }
        if let Some(ref exif) = config.exif_data {
            webp_data = crate::mux::embed_exif(&webp_data, exif)?;
        }
        if let Some(ref xmp) = config.xmp_data {
            webp_data = crate::mux::embed_xmp(&webp_data, xmp)?;
        }
        return Ok((webp_data, stats));
    }

    result
}

/// Internal: Encode with config and cooperative cancellation support.
pub(crate) fn encode_with_config_stoppable<S: Stop>(
    data: &[u8],
    width: u32,
    height: u32,
    bpp: u8,
    config: &EncoderConfig,
    stop: &S,
) -> Result<Vec<u8>> {
    validate_dimensions(width, height)?;
    validate_buffer_size(data.len(), width, height, bpp as u32)?;

    // Check for early cancellation
    stop.check()
        .map_err(|reason| at!(Error::Stopped(reason)))?;

    let webp_config = config.to_libwebp()?;

    // Initialize picture
    let mut picture = libwebp_sys::WebPPicture::new()
        .map_err(|_| at!(Error::InvalidConfig("failed to init picture".into())))?;

    picture.width = width as i32;
    picture.height = height as i32;
    picture.use_argb = 1;

    // Import pixel data
    let import_ok = if bpp == 4 {
        unsafe {
            libwebp_sys::WebPPictureImportRGBA(&mut picture, data.as_ptr(), (width * 4) as i32)
        }
    } else {
        unsafe {
            libwebp_sys::WebPPictureImportRGB(&mut picture, data.as_ptr(), (width * 3) as i32)
        }
    };

    if import_ok == 0 {
        unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
        return Err(at!(Error::EncodeFailed(EncodingError::OutOfMemory)));
    }

    // Setup memory writer
    let mut writer = core::mem::MaybeUninit::<libwebp_sys::WebPMemoryWriter>::uninit();
    unsafe { libwebp_sys::WebPMemoryWriterInit(writer.as_mut_ptr()) };
    let mut writer = unsafe { writer.assume_init() };

    picture.writer = Some(libwebp_sys::WebPMemoryWrite);
    picture.custom_ptr = &mut writer as *mut _ as *mut _;

    // Setup progress hook for cancellation
    let ctx = StopContext { stop };
    picture.progress_hook = Some(progress_hook::<S>);
    picture.user_data = &ctx as *const _ as *mut _;

    // Encode
    let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, &mut picture) };

    let result = if ok == 0 {
        let error_code = picture.error_code as i32;
        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }
        // Check if this was a user abort (cancellation)
        if error_code == 10 {
            // VP8_ENC_ERROR_USER_ABORT
            // Get the actual stop reason
            if let Err(reason) = stop.check() {
                return Err(at!(Error::Stopped(reason)));
            }
            // Fallback if stop doesn't report stopped (shouldn't happen)
            Err(at!(Error::EncodeFailed(EncodingError::UserAbort)))
        } else {
            Err(at!(Error::EncodeFailed(EncodingError::from(error_code))))
        }
    } else {
        let webp_data = unsafe {
            let slice = core::slice::from_raw_parts(writer.mem, writer.size);
            slice.to_vec()
        };
        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }
        Ok(webp_data)
    };

    // Embed metadata if present
    #[cfg(feature = "icc")]
    if let Ok(mut webp_data) = result {
        if let Some(ref icc) = config.icc_profile {
            webp_data = crate::mux::embed_icc(&webp_data, icc)?;
        }
        if let Some(ref exif) = config.exif_data {
            webp_data = crate::mux::embed_exif(&webp_data, exif)?;
        }
        if let Some(ref xmp) = config.xmp_data {
            webp_data = crate::mux::embed_xmp(&webp_data, xmp)?;
        }
        return Ok(webp_data);
    }

    result
}

/// WebP encoder with full configuration options.
///
/// This is a convenience wrapper around [`EncoderConfig`]. For new code,
/// prefer using `EncoderConfig` directly for its cleaner API.
///
/// # Example
///
/// ```rust,no_run
/// use webpx::{Encoder, Preset, Unstoppable};
///
/// let rgba: &[u8] = &[0u8; 640 * 480 * 4]; // placeholder
/// let webp = Encoder::new(rgba, 640, 480)
///     .preset(Preset::Photo)
///     .quality(85.0)
///     .encode(Unstoppable)?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub struct Encoder<'a> {
    data: EncoderInput<'a>,
    width: u32,
    height: u32,
    config: EncoderConfig,
    #[cfg(feature = "icc")]
    icc_profile: Option<&'a [u8]>,
}

/// Input pixel format for the encoder.
///
/// All formats store stride in bytes. For contiguous data, stride = width * bpp.
enum EncoderInput<'a> {
    /// RGBA 4-channel data with stride in bytes.
    Rgba {
        data: &'a [u8],
        stride_bytes: u32,
    },
    /// BGRA 4-channel data with stride in bytes.
    Bgra {
        data: &'a [u8],
        stride_bytes: u32,
    },
    /// RGB 3-channel data with stride in bytes.
    Rgb {
        data: &'a [u8],
        stride_bytes: u32,
    },
    /// BGR 3-channel data with stride in bytes.
    Bgr {
        data: &'a [u8],
        stride_bytes: u32,
    },
    /// YUV planar data.
    Yuv(YuvPlanesRef<'a>),
}

impl<'a> Encoder<'a> {
    /// Create a new encoder for contiguous RGBA data.
    ///
    /// For non-contiguous data with stride, use [`Self::new_rgba_stride`].
    #[must_use]
    pub fn new(data: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            data: EncoderInput::Rgba {
                data,
                stride_bytes: width * 4,
            },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for RGBA data with explicit stride.
    ///
    /// # Arguments
    /// * `data` - RGBA pixel data
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `stride_bytes` - Row stride in bytes (must be >= width * 4)
    #[must_use]
    pub fn new_rgba_stride(data: &'a [u8], width: u32, height: u32, stride_bytes: u32) -> Self {
        Self {
            data: EncoderInput::Rgba { data, stride_bytes },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for contiguous BGRA data.
    ///
    /// For non-contiguous data with stride, use [`Self::new_bgra_stride`].
    #[must_use]
    pub fn new_bgra(data: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            data: EncoderInput::Bgra {
                data,
                stride_bytes: width * 4,
            },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for BGRA data with explicit stride.
    ///
    /// # Arguments
    /// * `data` - BGRA pixel data
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `stride_bytes` - Row stride in bytes (must be >= width * 4)
    #[must_use]
    pub fn new_bgra_stride(data: &'a [u8], width: u32, height: u32, stride_bytes: u32) -> Self {
        Self {
            data: EncoderInput::Bgra { data, stride_bytes },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for contiguous RGB data (no alpha).
    ///
    /// For non-contiguous data with stride, use [`Self::new_rgb_stride`].
    #[must_use]
    pub fn new_rgb(data: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            data: EncoderInput::Rgb {
                data,
                stride_bytes: width * 3,
            },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for RGB data with explicit stride.
    ///
    /// # Arguments
    /// * `data` - RGB pixel data
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `stride_bytes` - Row stride in bytes (must be >= width * 3)
    #[must_use]
    pub fn new_rgb_stride(data: &'a [u8], width: u32, height: u32, stride_bytes: u32) -> Self {
        Self {
            data: EncoderInput::Rgb { data, stride_bytes },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for contiguous BGR data (no alpha).
    ///
    /// For non-contiguous data with stride, use [`Self::new_bgr_stride`].
    #[must_use]
    pub fn new_bgr(data: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            data: EncoderInput::Bgr {
                data,
                stride_bytes: width * 3,
            },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for BGR data with explicit stride.
    ///
    /// # Arguments
    /// * `data` - BGR pixel data
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `stride_bytes` - Row stride in bytes (must be >= width * 3)
    #[must_use]
    pub fn new_bgr_stride(data: &'a [u8], width: u32, height: u32, stride_bytes: u32) -> Self {
        Self {
            data: EncoderInput::Bgr { data, stride_bytes },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for YUV planar data.
    #[must_use]
    pub fn new_yuv(planes: YuvPlanesRef<'a>) -> Self {
        let width = planes.width;
        let height = planes.height;
        Self {
            data: EncoderInput::Yuv(planes),
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create encoder from an imgref `ImgRef<RGBA8>`.
    ///
    /// Properly handles non-contiguous stride from imgref.
    #[must_use]
    pub fn from_rgba(img: ImgRef<'a, RGBA8>) -> Self {
        // SAFETY: RGBA8 is repr(C) and has the same layout as [u8; 4]
        let data = unsafe {
            core::slice::from_raw_parts(img.buf().as_ptr() as *const u8, img.buf().len() * 4)
        };
        // imgref stride() returns stride in pixels, we need bytes
        let stride_bytes = (img.stride() * 4) as u32;
        Self::new_rgba_stride(data, img.width() as u32, img.height() as u32, stride_bytes)
    }

    /// Create encoder from an imgref `ImgRef<BGRA8>`.
    ///
    /// Properly handles non-contiguous stride from imgref.
    #[must_use]
    pub fn from_bgra(img: ImgRef<'a, BGRA8>) -> Self {
        // SAFETY: BGRA8 is repr(C) and has the same layout as [u8; 4]
        let data = unsafe {
            core::slice::from_raw_parts(img.buf().as_ptr() as *const u8, img.buf().len() * 4)
        };
        // imgref stride() returns stride in pixels, we need bytes
        let stride_bytes = (img.stride() * 4) as u32;
        Self::new_bgra_stride(data, img.width() as u32, img.height() as u32, stride_bytes)
    }

    /// Create encoder from an imgref `ImgRef<RGB8>`.
    ///
    /// Properly handles non-contiguous stride from imgref.
    #[must_use]
    pub fn from_rgb(img: ImgRef<'a, RGB8>) -> Self {
        // SAFETY: RGB8 is repr(C) and has the same layout as [u8; 3]
        let data = unsafe {
            core::slice::from_raw_parts(img.buf().as_ptr() as *const u8, img.buf().len() * 3)
        };
        // imgref stride() returns stride in pixels, we need bytes
        let stride_bytes = (img.stride() * 3) as u32;
        Self::new_rgb_stride(data, img.width() as u32, img.height() as u32, stride_bytes)
    }

    /// Create encoder from an imgref `ImgRef<BGR8>`.
    ///
    /// Properly handles non-contiguous stride from imgref.
    #[must_use]
    pub fn from_bgr(img: ImgRef<'a, BGR8>) -> Self {
        // SAFETY: BGR8 is repr(C) and has the same layout as [u8; 3]
        let data = unsafe {
            core::slice::from_raw_parts(img.buf().as_ptr() as *const u8, img.buf().len() * 3)
        };
        // imgref stride() returns stride in pixels, we need bytes
        let stride_bytes = (img.stride() * 3) as u32;
        Self::new_bgr_stride(data, img.width() as u32, img.height() as u32, stride_bytes)
    }

    /// Set encoding quality (0.0 = smallest, 100.0 = best).
    #[must_use]
    pub fn quality(mut self, quality: f32) -> Self {
        self.config = self.config.quality(quality);
        self
    }

    /// Set content-aware preset.
    #[must_use]
    pub fn preset(mut self, preset: Preset) -> Self {
        self.config = self.config.preset(preset);
        self
    }

    /// Enable lossless compression.
    #[must_use]
    pub fn lossless(mut self, lossless: bool) -> Self {
        self.config = self.config.lossless(lossless);
        self
    }

    /// Set quality/speed tradeoff (0 = fast, 6 = slower but better).
    #[must_use]
    pub fn method(mut self, method: u8) -> Self {
        self.config = self.config.method(method);
        self
    }

    /// Set near-lossless preprocessing (0 = max, 100 = off).
    #[must_use]
    pub fn near_lossless(mut self, value: u8) -> Self {
        self.config = self.config.near_lossless(value);
        self
    }

    /// Set alpha quality (0-100).
    #[must_use]
    pub fn alpha_quality(mut self, quality: u8) -> Self {
        self.config = self.config.alpha_quality(quality);
        self
    }

    /// Preserve exact RGB values under transparent areas.
    #[must_use]
    pub fn exact(mut self, exact: bool) -> Self {
        self.config = self.config.exact(exact);
        self
    }

    /// Set target file size in bytes (0 = disabled).
    #[must_use]
    pub fn target_size(mut self, size: u32) -> Self {
        self.config = self.config.target_size(size);
        self
    }

    /// Use sharp YUV conversion (slower but better).
    #[must_use]
    pub fn sharp_yuv(mut self, enable: bool) -> Self {
        self.config = self.config.sharp_yuv(enable);
        self
    }

    /// Set full encoder configuration.
    #[must_use]
    pub fn config(mut self, config: EncoderConfig) -> Self {
        self.config = config;
        self
    }

    /// Set ICC profile to embed.
    #[cfg(feature = "icc")]
    #[must_use]
    pub fn icc_profile(mut self, profile: &'a [u8]) -> Self {
        self.icc_profile = Some(profile);
        self
    }

    /// Encode to WebP bytes with cooperative cancellation support.
    ///
    /// # Arguments
    /// - `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
    pub fn encode<S: Stop>(self, stop: S) -> Result<Vec<u8>> {
        validate_dimensions(self.width, self.height)?;

        // Check for early cancellation
        stop.check()
            .map_err(|reason| at!(Error::Stopped(reason)))?;

        let webp_config = self.config.to_libwebp()?;

        // Initialize picture
        let mut picture = libwebp_sys::WebPPicture::new()
            .map_err(|_| at!(Error::InvalidConfig("failed to init picture".into())))?;

        picture.width = self.width as i32;
        picture.height = self.height as i32;

        // Import pixel data
        let import_ok = match &self.data {
            EncoderInput::Rgba { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 4)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportRGBA(
                        &mut picture,
                        data.as_ptr(),
                        *stride_bytes as i32,
                    )
                }
            }
            EncoderInput::Bgra { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 4)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportBGRA(
                        &mut picture,
                        data.as_ptr(),
                        *stride_bytes as i32,
                    )
                }
            }
            EncoderInput::Rgb { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 3)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportRGB(
                        &mut picture,
                        data.as_ptr(),
                        *stride_bytes as i32,
                    )
                }
            }
            EncoderInput::Bgr { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 3)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportBGR(
                        &mut picture,
                        data.as_ptr(),
                        *stride_bytes as i32,
                    )
                }
            }
            EncoderInput::Yuv(planes) => {
                picture.use_argb = 0;
                picture.colorspace = if planes.a.is_some() {
                    libwebp_sys::WebPEncCSP::WEBP_YUV420A
                } else {
                    libwebp_sys::WebPEncCSP::WEBP_YUV420
                };
                picture.y = planes.y.as_ptr() as *mut _;
                picture.u = planes.u.as_ptr() as *mut _;
                picture.v = planes.v.as_ptr() as *mut _;
                picture.y_stride = planes.y_stride as i32;
                picture.uv_stride = planes.u_stride as i32;
                if let Some(a) = &planes.a {
                    picture.a = a.as_ptr() as *mut _;
                    picture.a_stride = planes.a_stride as i32;
                }
                1 // YUV doesn't use import functions
            }
        };

        if import_ok == 0 {
            unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
            return Err(at!(Error::EncodeFailed(EncodingError::OutOfMemory)));
        }

        // Setup memory writer
        let mut writer = core::mem::MaybeUninit::<libwebp_sys::WebPMemoryWriter>::uninit();
        unsafe { libwebp_sys::WebPMemoryWriterInit(writer.as_mut_ptr()) };
        let mut writer = unsafe { writer.assume_init() };

        picture.writer = Some(libwebp_sys::WebPMemoryWrite);
        picture.custom_ptr = &mut writer as *mut _ as *mut _;

        // Setup progress hook for cancellation
        let ctx = StopContext { stop: &stop };
        picture.progress_hook = Some(progress_hook::<S>);
        picture.user_data = &ctx as *const _ as *mut _;

        // Encode
        let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, &mut picture) };

        let result = if ok == 0 {
            let error_code = picture.error_code as i32;
            unsafe {
                libwebp_sys::WebPPictureFree(&mut picture);
                libwebp_sys::WebPMemoryWriterClear(&mut writer);
            }
            // Check if this was a user abort (cancellation)
            if error_code == 10 {
                // VP8_ENC_ERROR_USER_ABORT
                if let Err(reason) = stop.check() {
                    return Err(at!(Error::Stopped(reason)));
                }
                Err(at!(Error::EncodeFailed(EncodingError::UserAbort)))
            } else {
                Err(at!(Error::EncodeFailed(EncodingError::from(error_code))))
            }
        } else {
            let webp_data = unsafe {
                let slice = core::slice::from_raw_parts(writer.mem, writer.size);
                slice.to_vec()
            };
            unsafe {
                libwebp_sys::WebPPictureFree(&mut picture);
                libwebp_sys::WebPMemoryWriterClear(&mut writer);
            }

            #[cfg(feature = "icc")]
            if let Some(icc) = self.icc_profile {
                return crate::mux::embed_icc(&webp_data, icc);
            }

            Ok(webp_data)
        };

        result
    }
}

pub(crate) fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    const MAX_DIMENSION: u32 = 16383;

    if width == 0 || height == 0 {
        return Err(at!(Error::InvalidInput(
            "width and height must be non-zero".into(),
        )));
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "dimensions exceed maximum ({} x {})",
            MAX_DIMENSION,
            MAX_DIMENSION
        ))));
    }
    Ok(())
}

pub(crate) fn validate_buffer_size(size: usize, width: u32, height: u32, bpp: u32) -> Result<()> {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(bpp as usize);

    if size < expected {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "buffer too small: got {}, expected {}",
            size,
            expected
        ))));
    }
    Ok(())
}

/// Validate buffer size with stride support.
///
/// The buffer must have at least `stride_bytes * height` bytes,
/// and stride must be at least `width * bpp`.
pub(crate) fn validate_buffer_size_stride(
    size: usize,
    width: u32,
    height: u32,
    stride_bytes: u32,
    bpp: u32,
) -> Result<()> {
    let min_stride = (width as usize).saturating_mul(bpp as usize);
    if (stride_bytes as usize) < min_stride {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "stride too small: got {}, minimum {}",
            stride_bytes,
            min_stride
        ))));
    }

    let expected = (stride_bytes as usize).saturating_mul(height as usize);
    if size < expected {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "buffer too small: got {}, expected {} (stride {} × height {})",
            size,
            expected,
            stride_bytes,
            height
        ))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_dimensions() {
        assert!(validate_dimensions(0, 100).is_err());
        assert!(validate_dimensions(100, 0).is_err());
        assert!(validate_dimensions(20000, 100).is_err());
        assert!(validate_dimensions(100, 100).is_ok());
    }

    #[test]
    fn test_validate_buffer_size() {
        assert!(validate_buffer_size(100, 10, 10, 4).is_err());
        assert!(validate_buffer_size(400, 10, 10, 4).is_ok());
        assert!(validate_buffer_size(500, 10, 10, 4).is_ok());
    }
}
