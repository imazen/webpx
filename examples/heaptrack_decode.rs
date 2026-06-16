//! Heaptrack harness for WebP decode-from-bytes allocation profiling.
//!
//! Profiles the production-critical path: `webpx::decode_rgba(&bytes)` — decoding a
//! WebP file (untrusted input) all the way to interleaved RGBA8 pixels. The goal is
//! to surface allocation *pathologies* that don't show up in a wall-clock benchmark:
//! a high allocation *count* relative to image size, per-pixel or per-macroblock-row
//! mallocs, large transient peaks, or unbounded growth across repeated decodes (a
//! leak). High allocation churn hurts most under contended allocators (Windows,
//! multi-threaded servers) where a single decode of an untrusted upload turns into
//! many allocator lock round-trips.
//!
//! NOTE: webpx is a thin safe wrapper over `libwebp` via `libwebp-sys` (C FFI), so
//! the bulk of the allocation call-sites originate inside libwebp's C decoder
//! (`WebPDecode`); webpx owns the RAII resource wrappers and the output `Vec`. The
//! report below notes that the allocations are libwebp's, not Rust-side churn.
//! heaptrack captures the C `malloc`s too.
//!
//! This complements the existing `examples/alloc_profile.rs` (which single-shots a
//! mix of encode + decode methods on a synthesized image): this harness decodes a
//! committed fixture from bytes in a *loop*, so a per-decode leak shows up as
//! monotonic growth across iterations.
//!
//! Usage:
//!   cargo build --release --example heaptrack_decode
//!   heaptrack ./target/release/examples/heaptrack_decode                 # default fixture
//!   heaptrack ./target/release/examples/heaptrack_decode <file.webp> [iters]
//!
//! Then inspect:
//!   heaptrack_print heaptrack.heaptrack_decode.*.zst | less
//!
//! Defaults to the committed `tests/fixtures/lossy_rgb.webp` (100x100 lossy VP8)
//! decoded 8 times. Pass a larger WebP to judge the allocation count against a
//! bigger macroblock grid; a large fixture should be decoded fewer times (pass a
//! smaller `iters`).

use std::hint::black_box;
use std::path::{Path, PathBuf};

/// Resolve the default bundled fixture relative to the crate manifest so the
/// example runs from any working directory.
fn default_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lossy_rgb.webp")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let path: PathBuf = match args.get(1) {
        Some(p) => PathBuf::from(p),
        None => default_fixture(),
    };
    // Default 8 iterations; a leak shows up as monotonic growth across them, and a
    // healthy decoder's steady-state per-decode allocation count is iterations-stable.
    let iters: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8);

    let data = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("failed to read {}: {e}", path.display());
        std::process::exit(1);
    });

    // Probe once so the report can state the dimensions the alloc count is relative to.
    match webpx::ImageInfo::from_webp(&data) {
        Ok(info) => {
            eprintln!("fixture: {} ({} bytes on disk)", path.display(), data.len());
            eprintln!(
                "  decoded image: {}x{} ({:.2} MP), alpha {}, format {:?}",
                info.width,
                info.height,
                (f64::from(info.width) * f64::from(info.height)) / 1.0e6,
                info.has_alpha,
                info.format,
            );
            eprintln!(
                "  RGBA8 output buffer: {} bytes",
                u64::from(info.width) * u64::from(info.height) * 4
            );
        }
        Err(e) => {
            eprintln!(
                "probe (ImageInfo::from_webp) failed for {}: {e}",
                path.display()
            );
            std::process::exit(1);
        }
    }

    eprintln!("decoding {iters}x via webpx::decode_rgba(..) ...");

    let mut total_pixels: u64 = 0;
    for i in 0..iters {
        let (pixels, w, h) = webpx::decode_rgba(&data).unwrap_or_else(|e| {
            eprintln!("decode iteration {i} failed: {e}");
            std::process::exit(1);
        });
        total_pixels += u64::from(w) * u64::from(h);
        // Consume the decoded buffer so the optimizer can't elide the decode or the
        // allocation of the output Vec.
        black_box(&pixels);
        black_box(w);
        black_box(h);
    }

    eprintln!("done: decoded {total_pixels} total pixels across {iters} iterations");
}
