//! Error types for webpx operations.
//!
//! This module uses the [`whereat`] crate for lightweight error location tracking.
//!
//! # For Callers
//!
//! `webpx` functions return `Result<T, At<Error>>`, where [`At`] wraps the error
//! with source location traces. You have several options:
//!
//! ## Option 1: Propagate traces with `.at()`
//!
//! Use [`ResultAtExt::at()`] to add your call site to the trace:
//!
//! ```rust,ignore
//! use webpx::{ResultAtExt, encode_rgba, Unstoppable};
//!
//! fn my_encode(data: &[u8]) -> webpx::Result<Vec<u8>> {
//!     encode_rgba(data, 100, 100, 85.0, Unstoppable).at()?  // Adds location
//! }
//! ```
//!
//! ## Option 2: Add your crate's info to the trace
//!
//! For traces with your crate's GitHub links, define crate info and use `at_crate!`:
//!
//! ```rust,ignore
//! whereat::define_at_crate_info!();  // In your lib.rs
//!
//! fn my_encode(data: &[u8]) -> webpx::Result<Vec<u8>> {
//!     webpx::at_crate!(webpx::encode_rgba(data, 100, 100, 85.0, Unstoppable))?
//! }
//! ```
//!
//! ## Option 3: Convert to plain errors
//!
//! Use `.into_inner()` to discard traces and get the plain [`Error`]:
//!
//! ```rust,ignore
//! let result = encode_rgba(data, 100, 100, 85.0, Unstoppable);
//! let plain_result: Result<Vec<u8>, Error> = result.map_err(|e| e.into_inner());
//! ```
//!
//! ## Option 4: Inspect errors without traces
//!
//! Use `.error()` to get a reference to the underlying error:
//!
//! ```rust,ignore
//! if let Err(at_err) = encode_rgba(data, 100, 100, 85.0, Unstoppable) {
//!     match at_err.error() {
//!         Error::InvalidInput(msg) => println!("Bad input: {}", msg),
//!         _ => println!("{:?}", at_err),  // Print with trace
//!     }
//! }
//! ```
//!
//! [`whereat`]: https://docs.rs/whereat
//! [`At`]: whereat::At
//! [`ResultAtExt::at()`]: whereat::ResultAtExt::at

use alloc::string::String;
use core::fmt;
use enough::StopReason;
use whereat::At;

/// Result type for webpx operations.
///
/// Errors include source location traces via [`At<Error>`](whereat::At).
/// See the module-level documentation for how to work with traced errors.
pub type Result<T> = core::result::Result<T, At<Error>>;

/// Error type for webpx operations.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Invalid input parameters (dimensions, buffer size, etc.)
    InvalidInput(String),
    /// Encoding failed
    EncodeFailed(EncodingError),
    /// Decoding failed
    DecodeFailed(DecodingError),
    /// Configuration validation failed
    InvalidConfig(String),
    /// Memory allocation failed
    OutOfMemory,
    /// ICC profile operation failed
    IccError(String),
    /// Mux/demux operation failed
    MuxError(MuxError),
    /// Animation operation failed
    AnimationError(String),
    /// Streaming operation requires more data
    NeedMoreData,
    /// Invalid WebP data
    InvalidWebP,
    /// Operation was stopped via cooperative cancellation
    Stopped(StopReason),
    /// I/O error
    IoError(String),
    /// A configured resource limit ([`Limits`](crate::Limits)) was exceeded.
    LimitExceeded(crate::LimitExceeded),
    /// The zencodec operation is not supported by this codec (or by this
    /// build's feature set). Recoverable through the error chain with
    /// `zencodec::CodecErrorExt::unsupported_operation`.
    #[cfg(feature = "zencodec")]
    Unsupported(zencodec::UnsupportedOperation),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidInput(msg) => write!(f, "invalid input: {}", msg),
            Error::EncodeFailed(e) => write!(f, "encode failed: {}", e),
            Error::DecodeFailed(e) => write!(f, "decode failed: {}", e),
            Error::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
            Error::OutOfMemory => write!(f, "out of memory"),
            Error::IccError(msg) => write!(f, "ICC error: {}", msg),
            Error::MuxError(e) => write!(f, "mux error: {}", e),
            Error::AnimationError(msg) => write!(f, "animation error: {}", msg),
            Error::NeedMoreData => write!(f, "need more data"),
            Error::InvalidWebP => write!(f, "invalid WebP data"),
            Error::Stopped(reason) => write!(f, "{}", reason),
            Error::IoError(msg) => write!(f, "I/O error: {}", msg),
            Error::LimitExceeded(e) => write!(f, "limit exceeded: {}", e),
            #[cfg(feature = "zencodec")]
            Error::Unsupported(op) => write!(f, "{}", op),
        }
    }
}

impl From<crate::LimitExceeded> for Error {
    fn from(e: crate::LimitExceeded) -> Self {
        Error::LimitExceeded(e)
    }
}

impl From<StopReason> for Error {
    fn from(reason: StopReason) -> Self {
        Error::Stopped(reason)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::LimitExceeded(e) => Some(e),
            #[cfg(feature = "zencodec")]
            Error::Unsupported(op) => Some(op),
            _ => None,
        }
    }
}

#[cfg(feature = "zencodec")]
impl From<zencodec::UnsupportedOperation> for Error {
    fn from(op: zencodec::UnsupportedOperation) -> Self {
        Error::Unsupported(op)
    }
}

/// Maps every variant to the zencodec [`ErrorCategory`](zencodec::ErrorCategory),
/// so generic zencodec consumers can route webpx errors (HTTP status, retry,
/// logging) without naming this type.
#[cfg(feature = "zencodec")]
impl zencodec::CategorizedError for Error {
    fn codec_name(&self) -> Option<&'static str> {
        Some("webpx")
    }

    fn category(&self) -> zencodec::ErrorCategory {
        use zencodec::{
            CodecIoKind, ErrorCategory as C, ImageError, InternalKind, InvalidKind, LimitKind,
            RequestError, ResourceError, UnsupportedImageKind,
        };
        let params = C::Request(RequestError::Invalid(InvalidKind::Parameters));
        match self {
            Error::InvalidInput(_) => C::Request(RequestError::Invalid(InvalidKind::Buffer)),
            Error::InvalidConfig(_) | Error::IccError(_) => params,
            Error::EncodeFailed(e) => match e {
                EncodingError::OutOfMemory | EncodingError::BitstreamOutOfMemory => {
                    C::Resource(ResourceError::OutOfMemory)
                }
                EncodingError::InvalidConfiguration
                | EncodingError::Partition0Overflow
                | EncodingError::PartitionOverflow => params,
                EncodingError::BadDimension | EncodingError::FileTooBig => {
                    C::Request(RequestError::Invalid(InvalidKind::Buffer))
                }
                EncodingError::BadWrite => C::Io(CodecIoKind::opaque()),
                EncodingError::UserAbort => C::Stopped(StopReason::Cancelled),
                EncodingError::NullParameter => C::Internal(InternalKind::Bug),
                EncodingError::Ok | EncodingError::Last => C::Internal(InternalKind::Dependency),
            },
            Error::DecodeFailed(e) => match e {
                DecodingError::OutOfMemory => C::Resource(ResourceError::OutOfMemory),
                DecodingError::InvalidParam => params,
                DecodingError::BitstreamError => C::Image(ImageError::Malformed),
                DecodingError::UnsupportedFeature => {
                    C::Image(ImageError::Unsupported(UnsupportedImageKind::Feature))
                }
                DecodingError::Suspended | DecodingError::NotEnoughData => {
                    C::Image(ImageError::UnexpectedEof)
                }
                DecodingError::UserAbort => C::Stopped(StopReason::Cancelled),
                DecodingError::Ok => C::Internal(InternalKind::Dependency),
            },
            Error::MuxError(e) => match e {
                MuxError::BadData | MuxError::NotFound => C::Image(ImageError::Malformed),
                MuxError::NotEnoughData => C::Image(ImageError::UnexpectedEof),
                MuxError::MemoryError => C::Resource(ResourceError::OutOfMemory),
                MuxError::InvalidArgument => params,
                MuxError::Ok => C::Internal(InternalKind::Dependency),
            },
            Error::OutOfMemory => C::Resource(ResourceError::OutOfMemory),
            // libwebp's animation encoder reports failures as text only.
            Error::AnimationError(_) => C::Internal(InternalKind::Dependency),
            Error::NeedMoreData => C::Image(ImageError::UnexpectedEof),
            Error::InvalidWebP => C::Image(ImageError::Malformed),
            Error::Stopped(reason) => C::Stopped(*reason),
            Error::IoError(_) => C::Io(CodecIoKind::opaque()),
            Error::LimitExceeded(e) => C::Resource(ResourceError::Limits(match e {
                crate::LimitExceeded::Width { .. } => LimitKind::Width,
                crate::LimitExceeded::Height { .. } => LimitKind::Height,
                crate::LimitExceeded::Pixels { .. } => LimitKind::Pixels,
                crate::LimitExceeded::TotalPixels { .. } => LimitKind::TotalPixels,
                crate::LimitExceeded::InputSize { .. } => LimitKind::InputSize,
                crate::LimitExceeded::OutputSize { .. } => LimitKind::OutputSize,
                crate::LimitExceeded::Frames { .. } => LimitKind::Frames,
                crate::LimitExceeded::Duration { .. } => LimitKind::Duration,
                // A cap on how much metadata the decoder will allocate.
                crate::LimitExceeded::MetadataSize { .. } => LimitKind::Memory,
            })),
            Error::Unsupported(op) => C::Request(RequestError::Unsupported(*op)),
        }
    }
}

/// Encoding error codes from libwebp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[repr(i32)]
pub enum EncodingError {
    /// No error
    Ok = 0,
    /// Memory allocation error
    OutOfMemory = 1,
    /// Bitstream out of memory
    BitstreamOutOfMemory = 2,
    /// NULL parameter
    NullParameter = 3,
    /// Invalid configuration
    InvalidConfiguration = 4,
    /// Bad dimension (width or height is 0 or > 16383)
    BadDimension = 5,
    /// Partition is bigger than 512k
    Partition0Overflow = 6,
    /// Partition is bigger than 16M
    PartitionOverflow = 7,
    /// Bad write callback
    BadWrite = 8,
    /// File is bigger than 4G
    FileTooBig = 9,
    /// User abort
    UserAbort = 10,
    /// Last error (unknown)
    Last = 11,
}

impl From<i32> for EncodingError {
    fn from(code: i32) -> Self {
        match code {
            0 => EncodingError::Ok,
            1 => EncodingError::OutOfMemory,
            2 => EncodingError::BitstreamOutOfMemory,
            3 => EncodingError::NullParameter,
            4 => EncodingError::InvalidConfiguration,
            5 => EncodingError::BadDimension,
            6 => EncodingError::Partition0Overflow,
            7 => EncodingError::PartitionOverflow,
            8 => EncodingError::BadWrite,
            9 => EncodingError::FileTooBig,
            10 => EncodingError::UserAbort,
            _ => EncodingError::Last,
        }
    }
}

impl fmt::Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            EncodingError::Ok => "ok",
            EncodingError::OutOfMemory => "out of memory",
            EncodingError::BitstreamOutOfMemory => "bitstream out of memory",
            EncodingError::NullParameter => "null parameter",
            EncodingError::InvalidConfiguration => "invalid configuration",
            EncodingError::BadDimension => "bad dimension",
            EncodingError::Partition0Overflow => "partition0 overflow",
            EncodingError::PartitionOverflow => "partition overflow",
            EncodingError::BadWrite => "bad write",
            EncodingError::FileTooBig => "file too big",
            EncodingError::UserAbort => "user abort",
            EncodingError::Last => "unknown error",
        };
        write!(f, "{}", msg)
    }
}

/// Decoding error codes from libwebp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[repr(i32)]
pub enum DecodingError {
    /// No error
    Ok = 0,
    /// Memory allocation error
    OutOfMemory = 1,
    /// Invalid parameter
    InvalidParam = 2,
    /// Bitstream error
    BitstreamError = 3,
    /// Unsupported feature
    UnsupportedFeature = 4,
    /// Suspended (need more data)
    Suspended = 5,
    /// User abort
    UserAbort = 6,
    /// Not enough data
    NotEnoughData = 7,
}

impl From<i32> for DecodingError {
    fn from(code: i32) -> Self {
        match code {
            0 => DecodingError::Ok,
            1 => DecodingError::OutOfMemory,
            2 => DecodingError::InvalidParam,
            3 => DecodingError::BitstreamError,
            4 => DecodingError::UnsupportedFeature,
            5 => DecodingError::Suspended,
            6 => DecodingError::UserAbort,
            _ => DecodingError::NotEnoughData,
        }
    }
}

impl fmt::Display for DecodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            DecodingError::Ok => "ok",
            DecodingError::OutOfMemory => "out of memory",
            DecodingError::InvalidParam => "invalid param",
            DecodingError::BitstreamError => "bitstream error",
            DecodingError::UnsupportedFeature => "unsupported feature",
            DecodingError::Suspended => "suspended",
            DecodingError::UserAbort => "user abort",
            DecodingError::NotEnoughData => "not enough data",
        };
        write!(f, "{}", msg)
    }
}

/// Mux error codes from libwebp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[repr(i32)]
pub enum MuxError {
    /// Operation completed successfully
    Ok = 1,
    /// Object not present
    NotFound = 0,
    /// Invalid argument
    InvalidArgument = -1,
    /// Bad data
    BadData = -2,
    /// Memory error
    MemoryError = -3,
    /// Not enough data
    NotEnoughData = -4,
}

impl From<i32> for MuxError {
    fn from(code: i32) -> Self {
        match code {
            1 => MuxError::Ok,
            0 => MuxError::NotFound,
            -1 => MuxError::InvalidArgument,
            -2 => MuxError::BadData,
            -3 => MuxError::MemoryError,
            _ => MuxError::NotEnoughData,
        }
    }
}

impl fmt::Display for MuxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            MuxError::Ok => "ok",
            MuxError::NotFound => "not found",
            MuxError::InvalidArgument => "invalid argument",
            MuxError::BadData => "bad data",
            MuxError::MemoryError => "memory error",
            MuxError::NotEnoughData => "not enough data",
        };
        write!(f, "{}", msg)
    }
}
