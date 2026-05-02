//! WebP encoding functionality.
//!
//! # Stride Convention
//!
//! Functions with `_stride` suffix accept an explicit row stride parameter.
//! **The stride unit matches the data type:**
//!
//! | Data type | Stride unit | Example |
//! |-----------|-------------|---------|
//! | `&[u8]` (raw bytes) | bytes | `new_rgba_stride(..., stride_bytes)` |
//! | `&[u32]` (ARGB packed) | pixels | `new_argb_stride(..., stride_pixels)` |
//! | `&[P]` (typed pixels) | pixels | `from_pixels_stride(..., stride_pixels)` |
//!
//! Stride must always be >= width (in the appropriate unit).

use crate::config::{EncodeStats, EncoderConfig, Preset};
use crate::error::{EncodingError, Error, Result};
use crate::ffi::mem_writer::MemWriter;
use crate::ffi::picture::Picture;
use crate::types::{EncodePixel, PixelLayout, YuvPlanesRef};
use alloc::vec::Vec;
use enough::Stop;
use imgref::ImgRef;
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

    // RAII: Picture::Drop runs WebPPictureFree, MemWriter::Drop runs
    // WebPMemoryWriterClear — every error path below releases libwebp's
    // internal allocations without an explicit cleanup call.
    let mut picture = Picture::new()?;
    picture.inner_mut().width = width as i32;
    picture.inner_mut().height = height as i32;
    picture.inner_mut().use_argb = 1;

    // Initialize stats
    let mut stats = core::mem::MaybeUninit::<libwebp_sys::WebPAuxStats>::zeroed();
    picture.inner_mut().stats = stats.as_mut_ptr();

    // Import pixel data
    let import_ok = if bpp == 4 {
        unsafe {
            libwebp_sys::WebPPictureImportRGBA(
                picture.as_mut_ptr(),
                data.as_ptr(),
                (width * 4) as i32,
            )
        }
    } else {
        unsafe {
            libwebp_sys::WebPPictureImportRGB(
                picture.as_mut_ptr(),
                data.as_ptr(),
                (width * 3) as i32,
            )
        }
    };

    if import_ok == 0 {
        return Err(at!(Error::EncodeFailed(EncodingError::OutOfMemory)));
    }

    let mut writer = MemWriter::new();
    picture.inner_mut().writer = Some(libwebp_sys::WebPMemoryWrite);
    picture.inner_mut().custom_ptr = writer.as_mut_ptr() as *mut _;

    let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, picture.as_mut_ptr()) };

    let result = if ok == 0 {
        let error = EncodingError::from(picture.inner_mut().error_code as i32);
        Err(at!(Error::EncodeFailed(error)))
    } else {
        let webp_data = writer.to_vec();
        let stats_val = unsafe { stats.assume_init() };
        let encode_stats = EncodeStats::from_libwebp(&stats_val);
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

    // RAII: see `encode_with_config_stats` for the cleanup discipline.
    let mut picture = Picture::new()?;
    picture.inner_mut().width = width as i32;
    picture.inner_mut().height = height as i32;
    picture.inner_mut().use_argb = 1;

    // Import pixel data
    let import_ok = if bpp == 4 {
        unsafe {
            libwebp_sys::WebPPictureImportRGBA(
                picture.as_mut_ptr(),
                data.as_ptr(),
                (width * 4) as i32,
            )
        }
    } else {
        unsafe {
            libwebp_sys::WebPPictureImportRGB(
                picture.as_mut_ptr(),
                data.as_ptr(),
                (width * 3) as i32,
            )
        }
    };

    if import_ok == 0 {
        return Err(at!(Error::EncodeFailed(EncodingError::OutOfMemory)));
    }

    let mut writer = MemWriter::new();
    picture.inner_mut().writer = Some(libwebp_sys::WebPMemoryWrite);
    picture.inner_mut().custom_ptr = writer.as_mut_ptr() as *mut _;

    // Setup progress hook for cancellation
    let ctx = StopContext { stop };
    picture.inner_mut().progress_hook = Some(progress_hook::<S>);
    picture.inner_mut().user_data = &ctx as *const _ as *mut _;

    let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, picture.as_mut_ptr()) };

    let result = if ok == 0 {
        let error_code = picture.inner_mut().error_code as i32;
        // Check if this was a user abort (cancellation)
        if error_code == 10 {
            // VP8_ENC_ERROR_USER_ABORT
            if let Err(reason) = stop.check() {
                return Err(at!(Error::Stopped(reason)));
            }
            // Fallback if stop doesn't report stopped (shouldn't happen)
            Err(at!(Error::EncodeFailed(EncodingError::UserAbort)))
        } else {
            Err(at!(Error::EncodeFailed(EncodingError::from(error_code))))
        }
    } else {
        Ok(writer.to_vec())
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
/// let webp = Encoder::new_rgba(rgba, 640, 480)
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

impl EncoderInput<'_> {
    /// Returns true when this input variant points libwebp at user-borrowed
    /// memory and therefore needs `WebPConfig.exact = 1` to suppress
    /// libwebp's transparent-area cleanup writes (which would mutate the
    /// borrowed buffer and violate Rust's aliasing model).
    fn requires_exact(&self) -> bool {
        match self {
            EncoderInput::Argb { .. } => true,
            EncoderInput::Yuv(planes) => planes.a.is_some(),
            // The Rgba/Bgra/Rgb/Bgr variants go through WebPPictureImport*,
            // which copies into a libwebp-allocated argb buffer that libwebp
            // is then free to mutate.
            EncoderInput::Rgba { .. }
            | EncoderInput::Bgra { .. }
            | EncoderInput::Rgb { .. }
            | EncoderInput::Bgr { .. } => false,
        }
    }
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
                stride_bytes: width.saturating_mul(4),
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
                stride_bytes: width.saturating_mul(4),
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
                stride_bytes: width.saturating_mul(3),
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
                stride_bytes: width.saturating_mul(3),
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
    ///
    /// # Note on `exact`
    ///
    /// When the YUV input includes an alpha plane, `EncoderConfig.exact`
    /// is forced to `true` regardless of the configured value, so libwebp
    /// will not write back to the borrowed Y/U/V planes when smoothing
    /// fully-transparent regions. YUV-without-alpha is unaffected.
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
    /// Each `u32` is a pixel in `0xAARRGGBB` numeric layout (native
    /// integer encoding):
    /// - Bits 24-31: Alpha
    /// - Bits 16-23: Red
    /// - Bits 8-15: Green
    /// - Bits 0-7: Blue
    ///
    /// libwebp's encoder reads `pic->argb` byte-wise assuming the
    /// little-endian in-memory layout `[B, G, R, A]`. On little-endian
    /// targets the `0xAARRGGBB` numeric value lays out exactly as
    /// libwebp expects — that's every target webpx's CI exercises
    /// (x86_64, i686, aarch64, wasm32). On big-endian targets the
    /// numeric `0xAARRGGBB` lays out as `[A, R, G, B]` and the
    /// resulting WebP would have its color channels permuted; webpx
    /// has no big-endian CI coverage today, so big-endian callers
    /// should treat this path as unsupported.
    ///
    /// # Note on `exact`
    ///
    /// To keep the input buffer read-only from C, `EncoderConfig.exact` is
    /// forced to `true` for this path regardless of the configured value.
    /// This disables libwebp's optimization that overwrites RGB values
    /// under fully-transparent pixels — a small compression cost in
    /// exchange for keeping the borrow safe.
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
            core::slice::from_raw_parts(
                img.buf().as_ptr() as *const u8,
                img.buf().len().saturating_mul(bpp),
            )
        };
        // imgref stride() returns stride in pixels, we need bytes.
        // Saturate-then-clamp to u32::MAX so a `usize` stride that
        // overflows `u32::MAX` becomes a stride that
        // `validate_buffer_size_stride` will reject — preventing a
        // silent wrong-stride encode from a truncating `as u32` cast.
        let stride_bytes_usize = img.stride().saturating_mul(bpp);
        let stride_bytes = u32::try_from(stride_bytes_usize).unwrap_or(u32::MAX);
        Self::from_pixels_internal(
            data,
            img.width() as u32,
            img.height() as u32,
            stride_bytes,
            P::LAYOUT,
        )
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
            core::slice::from_raw_parts(
                pixels.as_ptr() as *const u8,
                pixels.len().saturating_mul(bpp),
            )
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
        // SAFETY: Pixel types are repr(C) and have the same layout as their byte arrays.
        let data = unsafe {
            core::slice::from_raw_parts(
                pixels.as_ptr() as *const u8,
                pixels.len().saturating_mul(bpp),
            )
        };
        // Saturate so an `u32` stride near `u32::MAX / bpp` does not wrap
        // and produce a stride smaller than `width × bpp` would expect —
        // pushing libwebp through a wrong-stride encode that
        // `validate_buffer_size_stride` couldn't catch with the truncated
        // value. After saturation, oversized strides hit the `i32::MAX`
        // upper-bound check in `validate_buffer_size_stride`.
        let stride_bytes =
            u32::try_from((stride_pixels as u64).saturating_mul(bpp as u64)).unwrap_or(u32::MAX);
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

        let mut webp_config = self.config.to_libwebp()?;

        // Zero-copy paths point libwebp at user-borrowed memory. With
        // `config.exact == 0`, libwebp's `WebPCleanupTransparentArea` /
        // `WebPReplaceTransparentPixels` write into that memory, which
        // would be UB given the borrow is shared from Rust's POV. Force
        // exact=1 on the zero-copy paths so the buffer stays read-only.
        if self.data.requires_exact() {
            webp_config.exact = 1;
        }

        // RAII picture: WebPPictureFree on drop, including the YUV
        // validation early return below.
        let mut picture = Picture::new()?;
        picture.inner_mut().width = self.width as i32;
        picture.inner_mut().height = self.height as i32;

        // Import pixel data
        let import_ok = match &self.data {
            EncoderInput::Rgba { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 4)?;
                picture.inner_mut().use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportRGBA(
                        picture.as_mut_ptr(),
                        data.as_ptr(),
                        *stride_bytes as i32,
                    )
                }
            }
            EncoderInput::Bgra { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 4)?;
                picture.inner_mut().use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportBGRA(
                        picture.as_mut_ptr(),
                        data.as_ptr(),
                        *stride_bytes as i32,
                    )
                }
            }
            EncoderInput::Rgb { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 3)?;
                picture.inner_mut().use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportRGB(
                        picture.as_mut_ptr(),
                        data.as_ptr(),
                        *stride_bytes as i32,
                    )
                }
            }
            EncoderInput::Bgr { data, stride_bytes } => {
                validate_buffer_size_stride(data.len(), self.width, self.height, *stride_bytes, 3)?;
                picture.inner_mut().use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportBGR(
                        picture.as_mut_ptr(),
                        data.as_ptr(),
                        *stride_bytes as i32,
                    )
                }
            }
            EncoderInput::Argb {
                data,
                stride_pixels,
            } => {
                // Zero-copy fast path: set argb pointer directly without Import.
                // Use saturating_mul so 32-bit usize (i686) cannot wrap around
                // a maliciously large stride and bypass the length guard.
                let min_len = (*stride_pixels as usize).saturating_mul(self.height as usize);
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
                // Reject stride values that would wrap to a negative i32
                // when cast for libwebp's i32 stride parameter.
                crate::ffi::validate::stride_fits_i32(
                    *stride_pixels as usize,
                    "Encoder::new_argb_stride",
                )?;
                let pic = picture.inner_mut();
                pic.use_argb = 1;
                pic.argb = data.as_ptr() as *mut u32;
                pic.argb_stride = *stride_pixels as i32;
                1 // Success - no import function needed (zero-copy)
            }
            EncoderInput::Yuv(planes) => {
                validate_yuv_planes(planes)?;
                let pic = picture.inner_mut();
                pic.use_argb = 0;
                pic.colorspace = if planes.a.is_some() {
                    libwebp_sys::WebPEncCSP::WEBP_YUV420A
                } else {
                    libwebp_sys::WebPEncCSP::WEBP_YUV420
                };
                pic.y = planes.y.as_ptr() as *mut _;
                pic.u = planes.u.as_ptr() as *mut _;
                pic.v = planes.v.as_ptr() as *mut _;
                pic.y_stride = planes.y_stride as i32;
                // u_stride == v_stride is enforced by validate_yuv_planes;
                // libwebp uses a single uv_stride field for both planes.
                pic.uv_stride = planes.u_stride as i32;
                if let Some(a) = &planes.a {
                    pic.a = a.as_ptr() as *mut _;
                    pic.a_stride = planes.a_stride as i32;
                }
                1 // YUV doesn't use import functions
            }
        };

        if import_ok == 0 {
            return Err(at!(Error::EncodeFailed(EncodingError::OutOfMemory)));
        }

        let mut writer = MemWriter::new();
        picture.inner_mut().writer = Some(libwebp_sys::WebPMemoryWrite);
        picture.inner_mut().custom_ptr = writer.as_mut_ptr() as *mut _;

        // Setup progress hook for cancellation
        let ctx = StopContext { stop: &stop };
        picture.inner_mut().progress_hook = Some(progress_hook::<S>);
        picture.inner_mut().user_data = &ctx as *const _ as *mut _;

        let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, picture.as_mut_ptr()) };

        if ok == 0 {
            let error_code = picture.inner_mut().error_code as i32;
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
            let webp_data = writer.to_vec();

            #[cfg(feature = "icc")]
            if let Some(icc) = self.icc_profile {
                return crate::mux::embed_icc(&webp_data, icc);
            }

            Ok(webp_data)
        }
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

        let mut webp_config = self.config.to_libwebp()?;
        // See `Encoder::encode` — zero-copy inputs need exact=1 to keep
        // libwebp from writing through borrowed user memory.
        if self.data.requires_exact() {
            webp_config.exact = 1;
        }

        // RAII picture + writer. Picture::Drop runs WebPPictureFree on
        // every error path. MemWriter::into_webp_data transfers
        // ownership of the encoded buffer to a WebPData (which frees
        // via WebPFree on drop) and suppresses our destructor.
        let mut picture = Picture::new()?;
        picture.inner_mut().width = self.width as i32;
        picture.inner_mut().height = self.height as i32;

        let import_ok = self.import_pixels(picture.inner_mut())?;
        if import_ok == 0 {
            return Err(at!(Error::EncodeFailed(EncodingError::OutOfMemory)));
        }

        let mut writer = MemWriter::new();
        picture.inner_mut().writer = Some(libwebp_sys::WebPMemoryWrite);
        picture.inner_mut().custom_ptr = writer.as_mut_ptr() as *mut _;

        // Setup progress hook for cancellation
        let ctx = StopContext { stop: &stop };
        picture.inner_mut().progress_hook = Some(progress_hook::<S>);
        picture.inner_mut().user_data = &ctx as *const _ as *mut _;

        let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, picture.as_mut_ptr()) };

        if ok == 0 {
            let error_code = picture.inner_mut().error_code as i32;
            if error_code == 10 {
                if let Err(reason) = stop.check() {
                    return Err(at!(Error::Stopped(reason)));
                }
                return Err(at!(Error::EncodeFailed(EncodingError::UserAbort)));
            }
            return Err(at!(Error::EncodeFailed(EncodingError::from(error_code))));
        }

        // Note: ICC profile embedding is not supported with encode_owned()
        // because it requires reallocating the buffer. Use encode() instead.
        #[cfg(feature = "icc")]
        if self.icc_profile.is_some() {
            // writer drops here (frees libwebp memory via Clear)
            return Err(at!(Error::InvalidConfig(
                "ICC profile embedding not supported with encode_owned(), use encode() instead"
                    .into()
            )));
        }

        Ok(writer.into_webp_data())
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
                // saturating_mul so 32-bit usize cannot wrap and bypass the guard.
                let min_len = (*stride_pixels as usize).saturating_mul(self.height as usize);
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
                // Reject stride values that would wrap to a negative i32
                // when cast for libwebp's i32 stride parameter.
                crate::ffi::validate::stride_fits_i32(
                    *stride_pixels as usize,
                    "Encoder::new_argb_stride",
                )?;
                picture.use_argb = 1;
                picture.argb = data.as_ptr() as *mut u32;
                picture.argb_stride = *stride_pixels as i32;
                1
            }
            EncoderInput::Yuv(planes) => {
                // Caller (encode_owned) owns `picture` and frees it on error.
                validate_yuv_planes(planes)?;
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

/// Validate YUV planes against image dimensions before passing the raw
/// pointers to libwebp. libwebp dereferences `y`/`u`/`v`/`a` directly via
/// the supplied strides; if a plane slice is shorter than `stride * rows`,
/// libwebp would read out of bounds. Each plane's stride must also be
/// large enough for its sample width.
pub(crate) fn validate_yuv_planes(planes: &YuvPlanesRef<'_>) -> Result<()> {
    let width = planes.width as usize;
    let height = planes.height as usize;
    // 4:2:0 chroma subsampling: ceil(w/2) × ceil(h/2)
    let uv_width = width.div_ceil(2);
    let uv_height = height.div_ceil(2);

    // Stride floors.
    if planes.y_stride < width {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "Y stride too small: got {}, minimum {}",
            planes.y_stride,
            width
        ))));
    }
    if planes.u_stride < uv_width {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "U stride too small: got {}, minimum {}",
            planes.u_stride,
            uv_width
        ))));
    }
    if planes.v_stride < uv_width {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "V stride too small: got {}, minimum {}",
            planes.v_stride,
            uv_width
        ))));
    }
    // libwebp's WebPPicture has a single uv_stride field used for both U
    // and V; if these don't match, V would be read with U's stride.
    if planes.u_stride != planes.v_stride {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "U and V strides must match (got u={}, v={}); libwebp uses a single uv_stride",
            planes.u_stride,
            planes.v_stride
        ))));
    }
    // Reject strides that would wrap to a negative i32 when cast for
    // libwebp's i32 stride parameters. libwebp's row-pointer arithmetic
    // (`pic->y + row * y_stride`) treats the stride as signed, so a
    // wrapped-negative stride would walk backwards through process
    // memory. The plane-length checks above don't catch this — a caller
    // can construct an oversized `&[u8]` (>=2 GB on 64-bit) that
    // satisfies `slice.len() >= stride * rows` for `stride >= 2^31`.
    crate::ffi::validate::stride_fits_i32(planes.y_stride, "YuvPlanes::y_stride")?;
    crate::ffi::validate::stride_fits_i32(planes.u_stride, "YuvPlanes::uv_stride")?;

    // Plane-length floors. The last row only needs `width` (chroma_width)
    // samples, but libwebp treats the slice as a contiguous stride×rows
    // block in several places; require the conservative full-stride extent.
    let y_min = planes.y_stride.saturating_mul(height);
    if planes.y.len() < y_min {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "Y plane too short: got {}, expected at least {} (y_stride {} × height {})",
            planes.y.len(),
            y_min,
            planes.y_stride,
            height
        ))));
    }
    let u_min = planes.u_stride.saturating_mul(uv_height);
    if planes.u.len() < u_min {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "U plane too short: got {}, expected at least {} (u_stride {} × uv_height {})",
            planes.u.len(),
            u_min,
            planes.u_stride,
            uv_height
        ))));
    }
    let v_min = planes.v_stride.saturating_mul(uv_height);
    if planes.v.len() < v_min {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "V plane too short: got {}, expected at least {} (v_stride {} × uv_height {})",
            planes.v.len(),
            v_min,
            planes.v_stride,
            uv_height
        ))));
    }

    if let Some(a) = planes.a {
        if planes.a_stride < width {
            return Err(at!(Error::InvalidInput(alloc::format!(
                "A stride too small: got {}, minimum {}",
                planes.a_stride,
                width
            ))));
        }
        crate::ffi::validate::stride_fits_i32(planes.a_stride, "YuvPlanes::a_stride")?;
        let a_min = planes.a_stride.saturating_mul(height);
        if a.len() < a_min {
            return Err(at!(Error::InvalidInput(alloc::format!(
                "A plane too short: got {}, expected at least {} (a_stride {} × height {})",
                a.len(),
                a_min,
                planes.a_stride,
                height
            ))));
        }
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
    // Stride is later cast to `i32` for libwebp's stride parameters.
    // A `u32` value >= 2^31 wraps to a negative `i32`; libwebp's pointer
    // arithmetic then walks backwards through process memory. Reject
    // before the cast.
    crate::ffi::validate::stride_fits_i32(stride_bytes as usize, "validate_buffer_size_stride")?;

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

/// Validate YUV planes against the picture dimensions.
///
/// libwebp reads each plane via `pic->{y,u,v,a} + row * stride`, so each
/// plane must hold at least `stride * rows` bytes. Strides themselves
/// must be wide enough for the row. libwebp uses a single `uv_stride`
/// for both U and V, so the caller-supplied `u_stride` and `v_stride`
/// must match — otherwise V would be read with U's stride.
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
