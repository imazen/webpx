//! Internal FFI wrapper layer.
//!
//! Every libwebp resource (`WebPDemuxer`, `WebPMux`, `WebPAnimDecoder`,
//! `WebPAnimEncoder`, `WebPIDecoder`, `WebPPicture`, `WebPMemoryWriter`,
//! `WebPChunkIterator`) is exposed here as a typed RAII handle whose
//! `Drop` impl runs the matching libwebp release / delete / clear /
//! free function. Every call site that used to hand-roll
//! `if x.is_null() { return Err... }` + `unsafe { Free(...) }` on the
//! error path now gets correctness for free.
//!
//! Validation helpers (`validate::check_stride_fits_i32`,
//! `validate::check_buffer_for_stride`, `validate::check_yuv_planes`)
//! live alongside so the bounds checks that used to be duplicated at
//! every FFI boundary have one canonical implementation.
//!
//! This module is `pub(crate)` — the wrappers exist to constrain the
//! `unsafe` blocks inside webpx, not to be exposed to downstream users.

#[cfg(any(feature = "icc", feature = "animation"))]
pub(crate) mod demux;

#[cfg(feature = "encode")]
pub(crate) mod mem_writer;

#[cfg(feature = "encode")]
pub(crate) mod picture;

pub(crate) mod validate;
