//! Core types for image data representation.

use alloc::vec::Vec;
use rgb::alt::{BGR8, BGRA8};
use rgb::{RGB8, RGBA8};
use whereat::*;

/// Pixel layout describing channel order (implementation detail).
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
#[doc(hidden)]
pub trait EncodePixel: Copy + 'static + private::Sealed {
    /// The pixel layout corresponding to this type.
    const LAYOUT: PixelLayout;
}

impl EncodePixel for RGBA8 {
    const LAYOUT: PixelLayout = PixelLayout::Rgba;
}

impl EncodePixel for BGRA8 {
    const LAYOUT: PixelLayout = PixelLayout::Bgra;
}

impl EncodePixel for RGB8 {
    const LAYOUT: PixelLayout = PixelLayout::Rgb;
}

impl EncodePixel for BGR8 {
    const LAYOUT: PixelLayout = PixelLayout::Bgr;
}

/// Sealed marker trait for pixel types that can be decoded into.
///
/// This trait is an implementation detail and should not be referenced directly.
/// Use concrete types like [`RGB8`], [`RGBA8`], [`BGR8`], [`BGRA8`] with
/// decode functions.
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

        // Get more detailed features
        let mut features = core::mem::MaybeUninit::<libwebp_sys::WebPBitstreamFeatures>::uninit();
        let status = unsafe {
            libwebp_sys::WebPGetFeatures(data.as_ptr(), data.len(), features.as_mut_ptr())
        };
        let features = unsafe { features.assume_init() };

        if status != libwebp_sys::VP8StatusCode::VP8_STATUS_OK {
            return Err(at!(crate::Error::DecodeFailed(
                crate::error::DecodingError::from(status as i32),
            )));
        }

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
    pub fn new(width: u32, height: u32, with_alpha: bool) -> Self {
        let y_stride = width as usize;
        let uv_stride = (width as usize).div_ceil(2);
        let uv_height = (height as usize).div_ceil(2);

        Self {
            y: alloc::vec![0u8; y_stride * height as usize],
            y_stride,
            u: alloc::vec![0u8; uv_stride * uv_height],
            u_stride: uv_stride,
            v: alloc::vec![0u8; uv_stride * uv_height],
            v_stride: uv_stride,
            a: if with_alpha {
                Some(alloc::vec![0u8; y_stride * height as usize])
            } else {
                None
            },
            a_stride: y_stride,
            width,
            height,
        }
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
