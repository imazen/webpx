//! RAII wrapper for `WebPDemuxer` and `WebPChunkIterator`.
//!
//! `Demux<'a>` ties the demuxer's lifetime to the input slice (libwebp
//! stores the input pointer internally and dereferences it on every
//! demuxer call). `ChunkIter<'_>` ties the iterator's lifetime to the
//! demuxer; both auto-release on drop, including the empty-chunk
//! branch where 0.2.1 leaked the iterator's internal allocation.

use crate::error::{Error, Result};
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use whereat::*;

/// Owned `WebPDemuxer` whose lifetime is tied to the input slice.
pub(crate) struct Demux<'a> {
    inner: NonNull<libwebp_sys::WebPDemuxer>,
    _data: PhantomData<&'a [u8]>,
}

impl<'a> Demux<'a> {
    /// Create a demuxer over `data`. Returns `Err(InvalidWebP)` if
    /// libwebp rejects the bitstream.
    pub(crate) fn new(data: &'a [u8]) -> Result<Self> {
        let webp_data = libwebp_sys::WebPData {
            bytes: data.as_ptr(),
            size: data.len(),
        };
        let demuxer = unsafe {
            libwebp_sys::WebPDemuxInternal(
                &webp_data,
                0,
                core::ptr::null_mut(),
                libwebp_sys::WEBP_DEMUX_ABI_VERSION as i32,
            )
        };
        match NonNull::new(demuxer) {
            Some(nn) => Ok(Self {
                inner: nn,
                _data: PhantomData,
            }),
            None => Err(at!(Error::InvalidWebP)),
        }
    }

    /// Raw demuxer pointer for the few sites that still need it
    /// (frame iteration, info queries). Marked unsafe at the call site
    /// because the raw pointer escapes the lifetime guarantee.
    #[allow(dead_code)] // Wired up by future migrations of the frame-iter path.
    pub(crate) fn as_ptr(&self) -> *mut libwebp_sys::WebPDemuxer {
        self.inner.as_ptr()
    }

    /// Look up the first chunk matching `fourcc`. Returns `None` when
    /// libwebp's `WebPDemuxGetChunk` returns 0 (no chunk found); the
    /// iterator is guaranteed released on drop in every other case.
    pub(crate) fn get_chunk<'demux>(&'demux self, fourcc: &[u8; 4]) -> Option<ChunkIter<'demux>> {
        let mut iter = MaybeUninit::<libwebp_sys::WebPChunkIterator>::zeroed();
        let found = unsafe {
            libwebp_sys::WebPDemuxGetChunk(
                self.inner.as_ptr(),
                fourcc.as_ptr() as *const core::ffi::c_char,
                1,
                iter.as_mut_ptr(),
            )
        };
        if found == 0 {
            None
        } else {
            // SAFETY: `WebPDemuxGetChunk` returning non-zero means the
            // iterator is initialized and must be released on drop.
            Some(ChunkIter {
                inner: unsafe { iter.assume_init() },
                _demux: PhantomData,
            })
        }
    }
}

impl Drop for Demux<'_> {
    fn drop(&mut self) {
        // SAFETY: `inner` is a valid demuxer pointer constructed in `new`.
        unsafe { libwebp_sys::WebPDemuxDelete(self.inner.as_ptr()) };
    }
}

/// `WebPChunkIterator` borrowing from a `Demux`. Auto-releases on drop.
pub(crate) struct ChunkIter<'demux> {
    inner: libwebp_sys::WebPChunkIterator,
    _demux: PhantomData<&'demux Demux<'demux>>,
}

impl ChunkIter<'_> {
    /// Borrow the chunk bytes. Returns `&[]` when libwebp populated the
    /// iterator with a null pointer or zero size (which is reachable
    /// for some chunk types).
    pub(crate) fn bytes(&self) -> &[u8] {
        if self.inner.chunk.bytes.is_null() || self.inner.chunk.size == 0 {
            &[]
        } else {
            // SAFETY: libwebp guarantees `bytes` points to `size` valid
            // bytes inside the demuxer's input slice while the iterator
            // is live; `_demux` lifetime constrains us to that scope.
            unsafe { core::slice::from_raw_parts(self.inner.chunk.bytes, self.inner.chunk.size) }
        }
    }
}

impl Drop for ChunkIter<'_> {
    fn drop(&mut self) {
        // SAFETY: `inner` was populated by a successful
        // `WebPDemuxGetChunk` call. libwebp's contract is to call
        // `WebPDemuxReleaseChunkIterator` on every populated iterator,
        // regardless of chunk contents.
        unsafe { libwebp_sys::WebPDemuxReleaseChunkIterator(&mut self.inner) };
    }
}
