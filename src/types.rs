//! Core types for image data representation.

use alloc::vec::Vec;
#[cfg(any(feature = "encode", feature = "decode"))]
use rgb::alt::{BGR8, BGRA8};
#[cfg(any(feature = "encode", feature = "decode"))]
use rgb::{RGB8, RGBA8};
use whereat::*;

/// Pixel layout describing channel order (implementation detail).
#[cfg(any(feature = "encode", feature = "decode"))]
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelLayout {
    /// RGBA - 4 bytes per pixel (red, green, blue, alpha)
    Rgba,
    /// BGRA - 4 bytes per pixel (blue, green, red, alpha) - Windows/GPU native
    Bgra,
    /// RGB - 3 bytes per pixel (red, green, blue)
    Rgb,
    /// BGR - 3 bytes per pixel (blue, green, red) - OpenCV native
    Bgr,
}

#[cfg(any(feature = "encode", feature = "decode"))]
impl PixelLayout {
    /// Bytes per pixel for this layout.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            PixelLayout::Rgba | PixelLayout::Bgra => 4,
            PixelLayout::Rgb | PixelLayout::Bgr => 3,
        }
    }

    /// Whether this layout has an alpha channel.
    #[must_use]
    pub const fn has_alpha(self) -> bool {
        matches!(self, PixelLayout::Rgba | PixelLayout::Bgra)
    }
}

/// Sealed marker trait for pixel types that can be encoded.
///
/// This trait is an implementation detail and should not be referenced directly.
/// Use concrete types like [`RGB8`], [`RGBA8`], [`BGR8`], [`BGRA8`] with
/// [`Encoder::from_pixels`](crate::Encoder::from_pixels).
#[cfg(feature = "encode")]
#[doc(hidden)]
pub trait EncodePixel: Copy + 'static + private::Sealed {
    /// The pixel layout corresponding to this type.
    const LAYOUT: PixelLayout;
}

#[cfg(feature = "encode")]
impl EncodePixel for RGBA8 {
    const LAYOUT: PixelLayout = PixelLayout::Rgba;
}

#[cfg(feature = "encode")]
impl EncodePixel for BGRA8 {
    const LAYOUT: PixelLayout = PixelLayout::Bgra;
}

#[cfg(feature = "encode")]
impl EncodePixel for RGB8 {
    const LAYOUT: PixelLayout = PixelLayout::Rgb;
}

#[cfg(feature = "encode")]
impl EncodePixel for BGR8 {
    const LAYOUT: PixelLayout = PixelLayout::Bgr;
}

/// Sealed marker trait for pixel types that can be decoded into.
///
/// This trait is an implementation detail and should not be referenced directly.
/// Use concrete types like [`RGB8`], [`RGBA8`], [`BGR8`], [`BGRA8`] with
/// decode functions.
#[cfg(feature = "decode")]
#[doc(hidden)]
pub trait DecodePixel: Copy + 'static + private::Sealed {
    /// The pixel layout corresponding to this type.
    const LAYOUT: PixelLayout;

    /// Decode WebP data into a newly allocated buffer.
    ///
    /// # Safety
    /// This calls libwebp FFI functions.
    #[doc(hidden)]
    fn decode_new(data: &[u8]) -> Option<(*mut u8, i32, i32)>;

    /// Decode WebP data into an existing buffer.
    ///
    /// # Safety
    /// - `output` must be a valid pointer to a buffer of at least `output_len` bytes
    /// - The buffer must remain valid for the duration of the call
    #[doc(hidden)]
    unsafe fn decode_into(data: &[u8], output: *mut u8, output_len: usize, stride: i32) -> bool;
}

#[cfg(feature = "decode")]
impl DecodePixel for RGBA8 {
    const LAYOUT: PixelLayout = PixelLayout::Rgba;

    fn decode_new(data: &[u8]) -> Option<(*mut u8, i32, i32)> {
        let mut width: i32 = 0;
        let mut height: i32 = 0;
        let ptr = unsafe {
            libwebp_sys::WebPDecodeRGBA(data.as_ptr(), data.len(), &mut width, &mut height)
        };
        if ptr.is_null() {
            None
        } else {
            Some((ptr, width, height))
        }
    }

    unsafe fn decode_into(data: &[u8], output: *mut u8, output_len: usize, stride: i32) -> bool {
        // SAFETY: Caller guarantees output is valid for output_len bytes
        let result = unsafe {
            libwebp_sys::WebPDecodeRGBAInto(data.as_ptr(), data.len(), output, output_len, stride)
        };
        !result.is_null()
    }
}

#[cfg(feature = "decode")]
impl DecodePixel for BGRA8 {
    const LAYOUT: PixelLayout = PixelLayout::Bgra;

    fn decode_new(data: &[u8]) -> Option<(*mut u8, i32, i32)> {
        let mut width: i32 = 0;
        let mut height: i32 = 0;
        let ptr = unsafe {
            libwebp_sys::WebPDecodeBGRA(data.as_ptr(), data.len(), &mut width, &mut height)
        };
        if ptr.is_null() {
            None
        } else {
            Some((ptr, width, height))
        }
    }

    unsafe fn decode_into(data: &[u8], output: *mut u8, output_len: usize, stride: i32) -> bool {
        // SAFETY: Caller guarantees output is valid for output_len bytes
        let result = unsafe {
            libwebp_sys::WebPDecodeBGRAInto(data.as_ptr(), data.len(), output, output_len, stride)
        };
        !result.is_null()
    }
}

#[cfg(feature = "decode")]
impl DecodePixel for RGB8 {
    const LAYOUT: PixelLayout = PixelLayout::Rgb;

    fn decode_new(data: &[u8]) -> Option<(*mut u8, i32, i32)> {
        let mut width: i32 = 0;
        let mut height: i32 = 0;
        let ptr = unsafe {
            libwebp_sys::WebPDecodeRGB(data.as_ptr(), data.len(), &mut width, &mut height)
        };
        if ptr.is_null() {
            None
        } else {
            Some((ptr, width, height))
        }
    }

    unsafe fn decode_into(data: &[u8], output: *mut u8, output_len: usize, stride: i32) -> bool {
        // SAFETY: Caller guarantees output is valid for output_len bytes
        let result = unsafe {
            libwebp_sys::WebPDecodeRGBInto(data.as_ptr(), data.len(), output, output_len, stride)
        };
        !result.is_null()
    }
}

#[cfg(feature = "decode")]
impl DecodePixel for BGR8 {
    const LAYOUT: PixelLayout = PixelLayout::Bgr;

    fn decode_new(data: &[u8]) -> Option<(*mut u8, i32, i32)> {
        let mut width: i32 = 0;
        let mut height: i32 = 0;
        let ptr = unsafe {
            libwebp_sys::WebPDecodeBGR(data.as_ptr(), data.len(), &mut width, &mut height)
        };
        if ptr.is_null() {
            None
        } else {
            Some((ptr, width, height))
        }
    }

    unsafe fn decode_into(data: &[u8], output: *mut u8, output_len: usize, stride: i32) -> bool {
        // SAFETY: Caller guarantees output is valid for output_len bytes
        let result = unsafe {
            libwebp_sys::WebPDecodeBGRInto(data.as_ptr(), data.len(), output, output_len, stride)
        };
        !result.is_null()
    }
}

#[cfg(any(feature = "encode", feature = "decode"))]
mod private {
    use super::*;

    pub trait Sealed {}
    impl Sealed for RGBA8 {}
    impl Sealed for BGRA8 {}
    impl Sealed for RGB8 {}
    impl Sealed for BGR8 {}
}

/// Information about a WebP image.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImageInfo {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Whether the image has an alpha channel.
    pub has_alpha: bool,
    /// Whether the image is animated.
    pub has_animation: bool,
    /// Number of frames (1 for static images).
    pub frame_count: u32,
    /// Bitstream format (lossy or lossless).
    pub format: BitstreamFormat,
}

impl ImageInfo {
    /// Get info from WebP data without decoding.
    pub fn from_webp(data: &[u8]) -> crate::Result<Self> {
        let mut width: i32 = 0;
        let mut height: i32 = 0;

        let result =
            unsafe { libwebp_sys::WebPGetInfo(data.as_ptr(), data.len(), &mut width, &mut height) };

        if result == 0 {
            return Err(at!(crate::Error::InvalidWebP));
        }

        // Get more detailed features. Only `assume_init` after `WebPGetFeatures`
        // returns `VP8_STATUS_OK` — on failure libwebp may not have written all
        // fields, and `MaybeUninit::assume_init` on uninitialized memory is UB
        // even for plain integer structs.
        let mut features = core::mem::MaybeUninit::<libwebp_sys::WebPBitstreamFeatures>::zeroed();
        let status = unsafe {
            libwebp_sys::WebPGetFeatures(data.as_ptr(), data.len(), features.as_mut_ptr())
        };
        if status != libwebp_sys::VP8StatusCode::VP8_STATUS_OK {
            return Err(at!(crate::Error::DecodeFailed(
                crate::error::DecodingError::from(status as i32),
            )));
        }
        let features = unsafe { features.assume_init() };

        let format = match features.format {
            0 => BitstreamFormat::Undefined,
            1 => BitstreamFormat::Lossy,
            2 => BitstreamFormat::Lossless,
            _ => BitstreamFormat::Undefined,
        };

        Ok(ImageInfo {
            width: width as u32,
            height: height as u32,
            has_alpha: features.has_alpha != 0,
            has_animation: features.has_animation != 0,
            frame_count: if features.has_animation != 0 { 0 } else { 1 }, // Animation frame count needs demux
            format,
        })
    }
}

/// Bitstream format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum BitstreamFormat {
    /// Format not determined.
    #[default]
    Undefined,
    /// Lossy compression (VP8).
    Lossy,
    /// Lossless compression (VP8L).
    Lossless,
}

/// Color mode for output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ColorMode {
    /// RGBA (8 bits per channel, 32 bits per pixel).
    #[default]
    Rgba,
    /// BGRA (8 bits per channel, 32 bits per pixel).
    Bgra,
    /// ARGB (8 bits per channel, 32 bits per pixel).
    Argb,
    /// RGB (8 bits per channel, 24 bits per pixel).
    Rgb,
    /// BGR (8 bits per channel, 24 bits per pixel).
    Bgr,
    /// YUV420 (separate Y, U, V planes).
    Yuv420,
    /// YUVA420 (YUV420 with alpha plane).
    Yuva420,
}

impl ColorMode {
    /// Bytes per pixel for packed formats.
    pub fn bytes_per_pixel(self) -> Option<usize> {
        match self {
            ColorMode::Rgba | ColorMode::Bgra | ColorMode::Argb => Some(4),
            ColorMode::Rgb | ColorMode::Bgr => Some(3),
            ColorMode::Yuv420 | ColorMode::Yuva420 => None, // Planar
        }
    }

    /// Whether this mode has an alpha channel.
    pub fn has_alpha(self) -> bool {
        matches!(
            self,
            ColorMode::Rgba | ColorMode::Bgra | ColorMode::Argb | ColorMode::Yuva420
        )
    }

    /// Whether this is a planar YUV format.
    pub fn is_yuv(self) -> bool {
        matches!(self, ColorMode::Yuv420 | ColorMode::Yuva420)
    }
}

/// YUV plane data for planar formats.
#[derive(Debug, Clone)]
pub struct YuvPlanes {
    /// Y (luma) plane data.
    pub y: Vec<u8>,
    /// Y plane stride in bytes.
    pub y_stride: usize,
    /// U (chroma blue) plane data.
    pub u: Vec<u8>,
    /// U plane stride in bytes.
    pub u_stride: usize,
    /// V (chroma red) plane data.
    pub v: Vec<u8>,
    /// V plane stride in bytes.
    pub v_stride: usize,
    /// Alpha plane data (optional).
    pub a: Option<Vec<u8>>,
    /// Alpha plane stride in bytes.
    pub a_stride: usize,
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
}

impl YuvPlanes {
    /// Create new YUV planes with the given dimensions.
    ///
    /// Allocates planes for YUV420 format where U and V are half the resolution.
    ///
    /// Returns `None` if the dimensions exceed libwebp's 16383×16383 limit
    /// or would otherwise overflow plane size calculations on 32-bit usize.
    /// For valid encode-side dimensions, prefer this over the panic-on-overflow
    /// `vec!` macro.
    #[must_use]
    pub fn new_checked(width: u32, height: u32, with_alpha: bool) -> Option<Self> {
        // Mirror the encoder's MAX_DIMENSION (libwebp's WEBP_MAX_DIMENSION).
        const MAX_DIMENSION: u32 = 16383;
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return None;
        }

        let y_stride = width as usize;
        let uv_stride = (width as usize).div_ceil(2);
        let uv_height = (height as usize).div_ceil(2);

        // saturating_mul guards 32-bit usize even though MAX_DIMENSION should
        // prevent overflow at the dimension cap.
        let y_size = y_stride.saturating_mul(height as usize);
        let uv_size = uv_stride.saturating_mul(uv_height);

        Some(Self {
            y: alloc::vec![0u8; y_size],
            y_stride,
            u: alloc::vec![0u8; uv_size],
            u_stride: uv_stride,
            v: alloc::vec![0u8; uv_size],
            v_stride: uv_stride,
            a: if with_alpha {
                Some(alloc::vec![0u8; y_size])
            } else {
                None
            },
            a_stride: y_stride,
            width,
            height,
        })
    }

    /// Create new YUV planes with the given dimensions.
    ///
    /// Allocates planes for YUV420 format where U and V are half the resolution.
    ///
    /// # Panics
    ///
    /// Panics if `width` or `height` is zero or exceeds 16383. For a
    /// non-panicking version, use [`Self::new_checked`].
    pub fn new(width: u32, height: u32, with_alpha: bool) -> Self {
        Self::new_checked(width, height, with_alpha)
            .expect("YuvPlanes::new: dimensions out of range (1..=16383)")
    }

    /// Get the U plane dimensions (half width/height for YUV420).
    pub fn uv_dimensions(&self) -> (u32, u32) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }
}

/// Reference to YUV planes (borrowed version).
#[derive(Debug, Clone, Copy)]
pub struct YuvPlanesRef<'a> {
    /// Y (luma) plane data.
    pub y: &'a [u8],
    /// Y plane stride in bytes.
    pub y_stride: usize,
    /// U (chroma blue) plane data.
    pub u: &'a [u8],
    /// U plane stride in bytes.
    pub u_stride: usize,
    /// V (chroma red) plane data.
    pub v: &'a [u8],
    /// V plane stride in bytes.
    pub v_stride: usize,
    /// Alpha plane data (optional).
    pub a: Option<&'a [u8]>,
    /// Alpha plane stride in bytes.
    pub a_stride: usize,
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
}

impl<'a> From<&'a YuvPlanes> for YuvPlanesRef<'a> {
    fn from(planes: &'a YuvPlanes) -> Self {
        Self {
            y: &planes.y,
            y_stride: planes.y_stride,
            u: &planes.u,
            u_stride: planes.u_stride,
            v: &planes.v,
            v_stride: planes.v_stride,
            a: planes.a.as_deref(),
            a_stride: planes.a_stride,
            width: planes.width,
            height: planes.height,
        }
    }
}

/// Owned WebP data that wraps libwebp's native memory allocation.
///
/// This type provides zero-copy access to encoded WebP data by directly
/// holding libwebp's allocated buffer. The memory is freed when dropped.
///
/// Use this when you want to avoid the copy from libwebp's internal buffer
/// to a `Vec<u8>`.
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
/// // Access the data without copying
/// let bytes: &[u8] = &webp_data;
/// println!("Encoded {} bytes", bytes.len());
///
/// // Or convert to Vec when needed (copies)
/// let vec: Vec<u8> = webp_data.into();
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub struct WebPData {
    ptr: *mut u8,
    len: usize,
}

// SAFETY: The data is heap-allocated and owned exclusively
unsafe impl Send for WebPData {}
unsafe impl Sync for WebPData {}

impl WebPData {
    /// Create a new WebPData from a raw pointer and length.
    ///
    /// # Safety
    ///
    /// - `ptr` must be a valid pointer allocated by libwebp's memory allocator
    /// - `len` must be the exact size of the allocation
    /// - The caller transfers ownership of the memory to this struct
    #[cfg(feature = "encode")]
    #[must_use]
    pub(crate) unsafe fn from_raw(ptr: *mut u8, len: usize) -> Self {
        Self { ptr, len }
    }

    /// Returns the length of the encoded data in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the data is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns a slice of the encoded data.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        if self.ptr.is_null() || self.len == 0 {
            &[]
        } else {
            // SAFETY: ptr is valid and len is correct per from_raw contract
            unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

impl Drop for WebPData {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: ptr was allocated by libwebp's WebPMemoryWriter
            unsafe {
                libwebp_sys::WebPFree(self.ptr as *mut core::ffi::c_void);
            }
        }
    }
}

impl core::ops::Deref for WebPData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl AsRef<[u8]> for WebPData {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl From<WebPData> for Vec<u8> {
    fn from(data: WebPData) -> Self {
        data.as_slice().to_vec()
    }
}

impl core::fmt::Debug for WebPData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WebPData")
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}
