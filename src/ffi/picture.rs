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
