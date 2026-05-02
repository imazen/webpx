//! RAII wrapper for `WebPMemoryWriter`.
//!
//! `MemWriter::new` initializes a zeroed `WebPMemoryWriter` (matching
//! the 0.2.1 zeroed-init discipline — libwebp's `*Init*` functions
//! only set their documented fields, so the padding stays
//! unconditionally zero rather than uninitialized). `into_vec`
//! transfers ownership of the encoded buffer into a `Vec<u8>` and
//! prevents the destructor from clearing the underlying allocation.
//! On drop, any unconsumed buffer is freed via `WebPMemoryWriterClear`
//! — which means every encode error path (and every panic during
//! encode) cleans up automatically without an explicit `Clear` call.

use alloc::vec::Vec;
use core::mem::MaybeUninit;

/// Owned `WebPMemoryWriter`. The libwebp allocation is freed on drop
/// via `WebPMemoryWriterClear`, so every encode error path (and every
/// panic during encode) cleans up unconditionally — except after
/// `into_webp_data`, which transfers ownership of the buffer to a
/// `WebPData` and suppresses the destructor (the `WebPData` will run
/// `WebPFree` on the same allocation; calling `Clear` here would be a
/// double-free).
pub(crate) struct MemWriter {
    inner: libwebp_sys::WebPMemoryWriter,
    /// Set by `into_webp_data` to suppress `WebPMemoryWriterClear`.
    consumed: bool,
}

impl MemWriter {
    pub(crate) fn new() -> Self {
        let mut writer = MaybeUninit::<libwebp_sys::WebPMemoryWriter>::zeroed();
        // SAFETY: libwebp initializes the bookkeeping fields of an
        // already-zeroed struct.
        unsafe { libwebp_sys::WebPMemoryWriterInit(writer.as_mut_ptr()) };
        Self {
            inner: unsafe { writer.assume_init() },
            consumed: false,
        }
    }

    /// Mutable pointer for assignment to `WebPPicture.custom_ptr`.
    pub(crate) fn as_mut_ptr(&mut self) -> *mut libwebp_sys::WebPMemoryWriter {
        &mut self.inner
    }

    /// Copy the encoded buffer into an owned `Vec<u8>`. The libwebp
    /// allocation is still freed by `Drop` via `WebPMemoryWriterClear`
    /// — we avoid `Vec::from_raw_parts` because libwebp's writer uses
    /// libc `malloc`/`realloc`, not Rust's global allocator (mixing
    /// the two is unsound even when the underlying `malloc` is the
    /// same one in practice). The copy is a single `memcpy` of the
    /// encoded output — sub-millisecond for any realistic frame size.
    pub(crate) fn to_vec(&self) -> Vec<u8> {
        if self.inner.mem.is_null() || self.inner.size == 0 {
            return Vec::new();
        }
        // SAFETY: libwebp guarantees `mem` points to `size` initialized
        // bytes for the writer's lifetime; we're inside that lifetime
        // (the `&self` borrow holds the writer alive).
        let bytes = unsafe { core::slice::from_raw_parts(self.inner.mem, self.inner.size) };
        bytes.to_vec()
    }

    /// Transfer ownership of libwebp's encoded buffer to a `WebPData`.
    ///
    /// `WebPData::Drop` calls `WebPFree` on the same allocation that
    /// `WebPMemoryWriterClear` would have freed; consuming the writer
    /// suppresses our destructor so the buffer isn't double-freed.
    /// Use this instead of `to_vec` when callers want zero-copy
    /// ownership.
    pub(crate) fn into_webp_data(mut self) -> crate::WebPData {
        if self.inner.mem.is_null() || self.inner.size == 0 {
            // Nothing libwebp-allocated to transfer; let Drop's
            // `WebPMemoryWriterClear` no-op handle the empty case.
            // Construct an empty WebPData via from_raw with a null ptr
            // — its Drop short-circuits on null.
            return unsafe { crate::WebPData::from_raw(core::ptr::null_mut(), 0) };
        }
        let webp = unsafe { crate::WebPData::from_raw(self.inner.mem, self.inner.size) };
        self.consumed = true;
        webp
    }
}

impl Drop for MemWriter {
    fn drop(&mut self) {
        if self.consumed {
            return;
        }
        // SAFETY: `inner` was initialized in `new` via
        // `WebPMemoryWriterInit`. `WebPMemoryWriterClear` is the
        // matching tear-down that frees libwebp's internal buffer.
        unsafe { libwebp_sys::WebPMemoryWriterClear(&mut self.inner) };
    }
}
