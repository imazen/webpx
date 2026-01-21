//! WebP encoding functionality.

use crate::config::{EncodeStats, EncoderConfig, Preset};
use crate::error::{EncodingError, Error, Result};
use crate::types::{EncodePixel, PixelLayout, YuvPlanesRef};
use alloc::vec::Vec;
use enough::Stop;
use imgref::ImgRef;
use rgb::alt::{BGR8, BGRA8};
use rgb::{RGB8, RGBA8};
use whereat::*;

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
    stop.check().map_err(|reason| at!(Error::Stopped(reason)))?;

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
/// All formats store stride in bytes, except ARGB which stores stride in pixels.
enum EncoderInput<'a> {
    /// RGBA 4-channel data with stride in bytes.
    Rgba { data: &'a [u8], stride_bytes: u32 },
    /// BGRA 4-channel data with stride in bytes.
    Bgra { data: &'a [u8], stride_bytes: u32 },
    /// RGB 3-channel data with stride in bytes.
    Rgb { data: &'a [u8], stride_bytes: u32 },
    /// BGR 3-channel data with stride in bytes.
    Bgr { data: &'a [u8], stride_bytes: u32 },
    /// Native ARGB as u32 (zero-copy fast path). Stride is in pixels.
    Argb { data: &'a [u32], stride_pixels: u32 },
    /// YUV planar data.
    Yuv(YuvPlanesRef<'a>),
}

impl<'a> Encoder<'a> {
    /// Create a new encoder for contiguous RGBA data.
    ///
    /// For non-contiguous data with stride, use [`Self::new_rgba_stride`].
    #[must_use]
    pub fn new_rgba(data: &'a [u8], width: u32, height: u32) -> Self {
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

    /// Alias for [`Self::new_rgba`] for backwards compatibility.
    #[must_use]
    #[doc(hidden)]
    pub fn new(data: &'a [u8], width: u32, height: u32) -> Self {
        Self::new_rgba(data, width, height)
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

    /// Create a new encoder for YUV planar data (zero-copy).
    ///
    /// The YUV planes are borrowed directly without copying.
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

    /// Create a new encoder for native ARGB data (zero-copy fast path).
    ///
    /// This is the fastest encoding path - data is passed directly to libwebp
    /// without any pixel format conversion or memory copying.
    ///
    /// # Format
    ///
    /// Each `u32` is a pixel in `0xAARRGGBB` format (native endian):
    /// - Bits 24-31: Alpha
    /// - Bits 16-23: Red
    /// - Bits 8-15: Green
    /// - Bits 0-7: Blue
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use webpx::{Encoder, Unstoppable};
    ///
    /// // Pack ARGB pixels: alpha=255, red=255, green=0, blue=0
    /// let red_pixel: u32 = 0xFF_FF_00_00;
    /// let argb_data: Vec<u32> = vec![red_pixel; 100 * 100];
    ///
    /// let webp = Encoder::new_argb(&argb_data, 100, 100)
    ///     .quality(85.0)
    ///     .encode(Unstoppable)?;
    /// # Ok::<(), webpx::At<webpx::Error>>(())
    /// ```
    #[must_use]
    pub fn new_argb(data: &'a [u32], width: u32, height: u32) -> Self {
        Self {
            data: EncoderInput::Argb {
                data,
                stride_pixels: width,
            },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for native ARGB data with explicit stride (zero-copy fast path).
    ///
    /// See [`Self::new_argb`] for format details.
    ///
    /// # Arguments
    /// * `data` - ARGB pixel data as u32 values
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `stride_pixels` - Row stride in pixels (must be >= width)
    #[must_use]
    pub fn new_argb_stride(data: &'a [u32], width: u32, height: u32, stride_pixels: u32) -> Self {
        Self {
            data: EncoderInput::Argb {
                data,
                stride_pixels,
            },
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create encoder from an imgref image.
    ///
    /// Accepts `ImgRef<RGBA8>`, `ImgRef<RGB8>`, `ImgRef<BGRA8>`, or `ImgRef<BGR8>`.
    /// Properly handles non-contiguous stride from imgref.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use webpx::{Encoder, Unstoppable};
    /// use rgb::RGBA8;
    ///
    /// let pixels: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 100 * 100];
    /// let img = imgref::Img::new(pixels.as_slice(), 100, 100);
    /// let webp = Encoder::from_img(img)
    ///     .quality(85.0)
    ///     .encode(Unstoppable)?;
    /// # Ok::<(), webpx::At<webpx::Error>>(())
    /// ```
    #[must_use]
    pub fn from_img<P: EncodePixel>(img: ImgRef<'a, P>) -> Self {
        let bpp = P::LAYOUT.bytes_per_pixel();
        // SAFETY: Pixel types are repr(C) and have the same layout as their byte arrays
        let data = unsafe {
            core::slice::from_raw_parts(img.buf().as_ptr() as *const u8, img.buf().len() * bpp)
        };
        // imgref stride() returns stride in pixels, we need bytes
        let stride_bytes = (img.stride() * bpp) as u32;
        Self::from_pixels_internal(
            data,
            img.width() as u32,
            img.height() as u32,
            stride_bytes,
            P::LAYOUT,
        )
    }

    /// Alias for [`Self::from_img`] for backwards compatibility.
    #[must_use]
    #[doc(hidden)]
    pub fn from_rgba(img: ImgRef<'a, RGBA8>) -> Self {
        Self::from_img(img)
    }

    /// Alias for [`Self::from_img`] for backwards compatibility.
    #[must_use]
    #[doc(hidden)]
    pub fn from_bgra(img: ImgRef<'a, BGRA8>) -> Self {
        Self::from_img(img)
    }

    /// Alias for [`Self::from_img`] for backwards compatibility.
    #[must_use]
    #[doc(hidden)]
    pub fn from_rgb(img: ImgRef<'a, RGB8>) -> Self {
        Self::from_img(img)
    }

    /// Alias for [`Self::from_img`] for backwards compatibility.
    #[must_use]
    #[doc(hidden)]
    pub fn from_bgr(img: ImgRef<'a, BGR8>) -> Self {
        Self::from_img(img)
    }

    /// Create encoder from a slice of typed pixels.
    ///
    /// This is the preferred method for type-safe encoding with rgb crate types.
    /// The pixel format is determined at compile time from the type parameter.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use webpx::{Encoder, Unstoppable};
    /// use rgb::RGBA8;
    ///
    /// let pixels: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 100 * 100];
    /// let webp = Encoder::from_pixels(&pixels, 100, 100)
    ///     .quality(85.0)
    ///     .encode(Unstoppable)?;
    /// # Ok::<(), webpx::At<webpx::Error>>(())
    /// ```
    #[must_use]
    pub fn from_pixels<P: EncodePixel>(pixels: &'a [P], width: u32, height: u32) -> Self {
        let bpp = P::LAYOUT.bytes_per_pixel();
        // SAFETY: Pixel types are repr(C) and have the same layout as their byte arrays
        let data = unsafe {
            core::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * bpp)
        };
        let stride_bytes = width * bpp as u32;
        Self::from_pixels_internal(data, width, height, stride_bytes, P::LAYOUT)
    }

    /// Create encoder from a slice of typed pixels with explicit stride.
    ///
    /// # Arguments
    /// * `pixels` - Pixel data
    /// * `width` - Image width in pixels
    /// * `height` - Image height in pixels
    /// * `stride_pixels` - Row stride in pixels (must be >= width)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use webpx::{Encoder, Unstoppable};
    /// use rgb::RGB8;
    ///
    /// // Buffer with 128-pixel alignment (stride = 128, width = 100)
    /// let pixels: Vec<RGB8> = vec![RGB8::new(0, 0, 0); 128 * 100];
    /// let webp = Encoder::from_pixels_stride(&pixels, 100, 100, 128)
    ///     .quality(85.0)
    ///     .encode(Unstoppable)?;
    /// # Ok::<(), webpx::At<webpx::Error>>(())
    /// ```
    #[must_use]
    pub fn from_pixels_stride<P: EncodePixel>(
        pixels: &'a [P],
        width: u32,
        height: u32,
        stride_pixels: u32,
    ) -> Self {
        let bpp = P::LAYOUT.bytes_per_pixel();
        // SAFETY: Pixel types are repr(C) and have the same layout as their byte arrays
        let data = unsafe {
            core::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * bpp)
        };
        let stride_bytes = stride_pixels * bpp as u32;
        Self::from_pixels_internal(data, width, height, stride_bytes, P::LAYOUT)
    }

    /// Internal helper to create encoder from byte data with a specific format.
    fn from_pixels_internal(
        data: &'a [u8],
        width: u32,
        height: u32,
        stride_bytes: u32,
        format: PixelLayout,
    ) -> Self {
        let input = match format {
            PixelLayout::Rgba => EncoderInput::Rgba { data, stride_bytes },
            PixelLayout::Bgra => EncoderInput::Bgra { data, stride_bytes },
            PixelLayout::Rgb => EncoderInput::Rgb { data, stride_bytes },
            PixelLayout::Bgr => EncoderInput::Bgr { data, stride_bytes },
        };
        Self {
            data: input,
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
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
        stop.check().map_err(|reason| at!(Error::Stopped(reason)))?;

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
            EncoderInput::Argb {
                data,
                stride_pixels,
            } => {
                // Zero-copy fast path: set argb pointer directly without Import
                let min_len = (*stride_pixels as usize) * (self.height as usize);
                if data.len() < min_len {
                    return Err(at!(Error::InvalidInput(alloc::format!(
                        "ARGB buffer too small: got {} pixels, expected {}",
                        data.len(),
                        min_len
                    ))));
                }
                if *stride_pixels < self.width {
                    return Err(at!(Error::InvalidInput(alloc::format!(
                        "ARGB stride too small: got {}, minimum {}",
                        stride_pixels,
                        self.width
                    ))));
                }
                picture.use_argb = 1;
                picture.argb = data.as_ptr() as *mut u32;
                picture.argb_stride = *stride_pixels as i32;
                1 // Success - no import function needed (zero-copy)
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

    /// Encode to WebP, returning owned data without copying.
    ///
    /// This is the most efficient encoding method when you don't need a `Vec<u8>`.
    /// The returned [`WebPData`](crate::WebPData) directly owns libwebp's internal
    /// buffer and frees it on drop.
    ///
    /// # Arguments
    /// - `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use webpx::{Encoder, Unstoppable};
    ///
    /// let rgba = vec![255u8; 100 * 100 * 4];
    /// let webp_data = Encoder::new_rgba(&rgba, 100, 100)
    ///     .quality(85.0)
    ///     .encode_owned(Unstoppable)?;
    ///
    /// // Use as slice without copying
    /// std::fs::write("output.webp", &*webp_data)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn encode_owned<S: Stop>(self, stop: S) -> Result<crate::WebPData> {
        validate_dimensions(self.width, self.height)?;

        // Check for early cancellation
        stop.check().map_err(|reason| at!(Error::Stopped(reason)))?;

        let webp_config = self.config.to_libwebp()?;

        // Initialize picture
        let mut picture = libwebp_sys::WebPPicture::new()
            .map_err(|_| at!(Error::InvalidConfig("failed to init picture".into())))?;

        picture.width = self.width as i32;
        picture.height = self.height as i32;

        // Import pixel data (same as encode())
        let import_ok = self.import_pixels(&mut picture)?;

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

        if ok == 0 {
            let error_code = picture.error_code as i32;
            unsafe {
                libwebp_sys::WebPPictureFree(&mut picture);
                libwebp_sys::WebPMemoryWriterClear(&mut writer);
            }
            if error_code == 10 {
                if let Err(reason) = stop.check() {
                    return Err(at!(Error::Stopped(reason)));
                }
                return Err(at!(Error::EncodeFailed(EncodingError::UserAbort)));
            }
            return Err(at!(Error::EncodeFailed(EncodingError::from(error_code))));
        }

        unsafe { libwebp_sys::WebPPictureFree(&mut picture) };

        // Transfer ownership to WebPData (don't clear the writer!)
        let webp_data = unsafe { crate::WebPData::from_raw(writer.mem, writer.size) };

        // Note: ICC profile embedding is not supported with encode_owned()
        // because it requires reallocating the buffer. Use encode() instead.
        #[cfg(feature = "icc")]
        if self.icc_profile.is_some() {
            // Drop webp_data (frees libwebp memory), then use regular encode path
            drop(webp_data);
            // Re-encode through the Vec path which handles ICC
            // This is inefficient but ICC embedding is rare
            return Err(at!(Error::InvalidConfig(
                "ICC profile embedding not supported with encode_owned(), use encode() instead"
                    .into()
            )));
        }

        Ok(webp_data)
    }

    /// Encode to WebP, appending to an existing Vec.
    ///
    /// This avoids allocation if you already have a Vec with capacity.
    ///
    /// # Arguments
    /// - `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
    /// - `output` - Vec to append encoded data to
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use webpx::{Encoder, Unstoppable};
    ///
    /// let rgba = vec![255u8; 100 * 100 * 4];
    /// let mut output = Vec::with_capacity(10000);
    ///
    /// Encoder::new_rgba(&rgba, 100, 100)
    ///     .quality(85.0)
    ///     .encode_into(Unstoppable, &mut output)?;
    ///
    /// println!("Encoded {} bytes", output.len());
    /// # Ok::<(), webpx::At<webpx::Error>>(())
    /// ```
    pub fn encode_into<S: Stop>(self, stop: S, output: &mut Vec<u8>) -> Result<()> {
        let data = self.encode_owned(stop)?;
        output.extend_from_slice(&data);
        Ok(())
    }

    /// Encode to WebP, writing to an [`io::Write`](std::io::Write) implementor.
    ///
    /// This is useful for streaming output to files or network without
    /// buffering the entire result in memory.
    ///
    /// # Arguments
    /// - `stop` - Cooperative cancellation token (use `Unstoppable` if not needed)
    /// - `writer` - Destination for encoded data
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use webpx::{Encoder, Unstoppable};
    /// use std::fs::File;
    ///
    /// let rgba = vec![255u8; 100 * 100 * 4];
    /// let mut file = File::create("output.webp")?;
    ///
    /// Encoder::new_rgba(&rgba, 100, 100)
    ///     .quality(85.0)
    ///     .encode_to_writer(Unstoppable, &mut file)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[cfg(feature = "std")]
    pub fn encode_to_writer<S: Stop, W: std::io::Write>(
        self,
        stop: S,
        mut writer: W,
    ) -> Result<()> {
        let data = self.encode_owned(stop)?;
        writer
            .write_all(&data)
            .map_err(|e| at!(Error::IoError(e.to_string())))?;
        Ok(())
    }

    /// Import pixels into the WebPPicture, returning the success code.
    fn import_pixels(&self, picture: &mut libwebp_sys::WebPPicture) -> Result<i32> {
        let import_ok = match &self.data {
            EncoderInput::Rgba { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 4)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportRGBA(picture, data.as_ptr(), *stride_bytes as i32)
                }
            }
            EncoderInput::Bgra { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 4)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportBGRA(picture, data.as_ptr(), *stride_bytes as i32)
                }
            }
            EncoderInput::Rgb { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 3)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportRGB(picture, data.as_ptr(), *stride_bytes as i32)
                }
            }
            EncoderInput::Bgr { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 3)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportBGR(picture, data.as_ptr(), *stride_bytes as i32)
                }
            }
            EncoderInput::Argb {
                data,
                stride_pixels,
            } => {
                let min_len = (*stride_pixels as usize) * (self.height as usize);
                if data.len() < min_len {
                    return Err(at!(Error::InvalidInput(alloc::format!(
                        "ARGB buffer too small: got {} pixels, expected {}",
                        data.len(),
                        min_len
                    ))));
                }
                if *stride_pixels < self.width {
                    return Err(at!(Error::InvalidInput(alloc::format!(
                        "ARGB stride too small: got {}, minimum {}",
                        stride_pixels,
                        self.width
                    ))));
                }
                picture.use_argb = 1;
                picture.argb = data.as_ptr() as *mut u32;
                picture.argb_stride = *stride_pixels as i32;
                1
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
                1
            }
        };
        Ok(import_ok)
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
