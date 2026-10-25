//! RAII wrapper for `WebPPicture`.
//!
//! `Picture::new` initializes a zeroed struct via `WebPPictureInit`
//! (matching the 0.2.1 zeroed-init discipline so libwebp's `padding`
//! arrays start zero rather than uninitialized). `Drop` runs
//! `WebPPictureFree` unconditionally, so every error path — including
//! panics during `WebPEncode` — releases libwebp's internal Y/U/V/A
//! plane allocations without an explicit `Free` call at every site.
//!
//! The wrapper exposes the underlying struct via `as_mut_ptr` /
//! `inner_mut` for the libwebp Import / Encode calls that take a
//! mutable pointer. All `unsafe` blocks remain at the call site so the
//! reviewer can see what the FFI pointer is being handed to.

use crate::error::{Error, Result};
use core::mem::MaybeUninit;
use whereat::*;

/// Owned `WebPPicture`. `Drop` calls `WebPPictureFree`.
pub(crate) struct Picture {
    inner: libwebp_sys::WebPPicture,
}

impl Picture {
    /// Initialize a zeroed picture. Returns `Err(InvalidConfig)` if
    /// libwebp's `WebPPictureInit` rejects the version check
    /// (impossible in practice with the linked `libwebp-sys`, but we
    /// surface it as a regular error rather than `unreachable!`).
    pub(crate) fn new() -> Result<Self> {
        let mut pic = MaybeUninit::<libwebp_sys::WebPPicture>::zeroed();
        // SAFETY: libwebp populates the documented fields of an
        // already-zeroed struct.
        let ok = unsafe { libwebp_sys::WebPPictureInit(pic.as_mut_ptr()) };
        if !ok {
            return Err(at!(Error::InvalidConfig(
                "WebPPictureInit failed (libwebp ABI mismatch)".into(),
            )));
        }
        Ok(Self {
            inner: unsafe { pic.assume_init() },
        })
    }

    /// Borrow the underlying struct mutably for libwebp Import / Encode
    /// calls. The struct stays owned by the wrapper; `Drop` will still
    /// run `WebPPictureFree` after the borrow ends.
    pub(crate) fn inner_mut(&mut self) -> &mut libwebp_sys::WebPPicture {
        &mut self.inner
    }

    /// Mutable raw pointer for libwebp APIs that take `*mut WebPPicture`.
    pub(crate) fn as_mut_ptr(&mut self) -> *mut libwebp_sys::WebPPicture {
        &mut self.inner
    }
}

impl Drop for Picture {
    fn drop(&mut self) {
        // SAFETY: `inner` was initialized by `WebPPictureInit` in
        // `new`. `WebPPictureFree` is the matching tear-down for any
        // Y/U/V/A plane buffers libwebp may have allocated on import.
        unsafe { libwebp_sys::WebPPictureFree(&mut self.inner) };
    }
}

/// `WebPPictureImportRGBA` without libwebp 1.6.0's unaligned reads.
///
/// For an ARGB picture on little-endian targets, libwebp 1.6.0 reads the
/// RGBA rows through `uint32_t*` (`VP8LConvertBGRAToRGBA`), which is
/// undefined behaviour unless the pointer and the stride are 4-byte
/// aligned, and faults on targets without unaligned loads (fixed upstream
/// after 1.6.0, b31469df48). A `&[u8]` from the caller has no alignment
/// guarantee, so unaligned input is first copied into an aligned buffer.
///
/// # Safety
///
/// `picture` must point to an initialized `WebPPicture` whose `width` and
/// `height` are set, and `rgba` must hold `height` rows `stride` bytes apart
/// (the last row needs only `width * 4` bytes).
pub(crate) unsafe fn import_rgba(
    picture: *mut libwebp_sys::WebPPicture,
    rgba: &[u8],
    stride: usize,
) -> i32 {
    // SAFETY: the caller guarantees `picture` is valid.
    let (w, h) = unsafe {
        (
            (*picture).width.max(0) as usize,
            (*picture).height.max(0) as usize,
        )
    };
    if (rgba.as_ptr() as usize).is_multiple_of(4) && stride.is_multiple_of(4) {
        // SAFETY: as documented; the buffer and stride are aligned.
        return unsafe {
            libwebp_sys::WebPPictureImportRGBA(picture, rgba.as_ptr(), stride as i32)
        };
    }
    let row = w.saturating_mul(4);
    // Each u32 holds one pixel's four bytes in memory order.
    let mut aligned = alloc::vec![0u32; w.saturating_mul(h)];
    for y in 0..h {
        let src = &rgba[y * stride..y * stride + row];
        for (d, p) in aligned[y * w..(y + 1) * w]
            .iter_mut()
            .zip(src.as_chunks::<4>().0)
        {
            *d = u32::from_ne_bytes(*p);
        }
    }
    // SAFETY: `aligned` is 4-byte aligned, holds `h` tight rows of `row` bytes
    // and outlives the call (libwebp copies the pixels into the picture).
    unsafe {
        libwebp_sys::WebPPictureImportRGBA(picture, aligned.as_ptr() as *const u8, row as i32)
    }
}
