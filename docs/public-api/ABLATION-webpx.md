# webpx public-API ablation report

**Date:** 2026-06-11
**Snapshot commit:** 40d142a
**Crate analyzed:** `webpx` (1,119 default / 1,583 all-features items)
**Grep template:** `ugrep -r --include="*.rs" --include="*.toml" "<symbol>" /home/lilith/work/ --exclude-dir=target --exclude-dir=.jj`

## Consumer context

No org-internal consumers found. webpx is a standalone crate (libwebp FFI wrapper). The library's doc header explicitly recommends `zenwebp` for new projects and notes webpx is maintained for existing libwebp link paths. Primary consumers would be external users and imageflow if wired up.

## Summary

**0 items flagged for action.**

### Observations (informational, no action needed)

1. **`ffi` module** — entirely `pub(crate)`. Five submodules (`demux`, `mem_writer`, `picture`, `validate`, plus a top-level `mod.rs`) all use `pub(crate)` visibility. Does not appear in the snapshot. Clean.

2. **`EncodePixel` / `DecodePixel` traits** — sealed via `private::Sealed` super-trait. They appear as type-parameter bounds in encode/decode free functions and methods but do not show up as top-level pub trait items in the snapshot. Correct sealed-trait pattern.

3. **`pub mod webpx::zencodec`** — gated by `#[cfg(feature = "zencodec")]`. Exports the full zencodec adapter surface (8 types). This is intentional public API documented in lib.rs.

4. **`wrap_sink_error`** on `WebpAnimationFrameDecoder` — required by the `zencodec::traits::decoder` trait contract (`fn wrap_sink_error(err: SinkError) -> Self::Error` is a required method on the `DecodeJob` trait). Not a leak; it's a mandated trait method.

5. **`EncodeStats`** — public struct with pub fields. Returned by advanced encode functions (returns `(Vec<u8>, EncodeStats)`). Intentional detailed encode output. No external consumers in org scan, but this is a standalone published crate.

6. **`pub mod webpx::heuristics`** — `DecodeEstimate`, `EncodeEstimate`, and five `estimate_*` free functions. All `#[non_exhaustive]` structs. No org consumers, but standard estimation API for resource-limit planning. Intentional.

7. **`pub mod webpx::compat`** — compatibility shims for migration from other WebP crates (`compat::webp`, `compat::webp_animation`). Explicitly documented in lib.rs. Intentional.

8. **`pub use webpx::{At, ResultAtExt, Stop, StopReason, Unstoppable, at, at_crate}`** — re-exports from `whereat` and `enough` crates. Documented in lib.rs error-handling section; these are needed so users can propagate `At<Error>` without depending on `whereat` directly. Intentional.

### No zencodec streaming-decoder leak pattern

`WebpDecodeJob::StreamDec` resolves to `WebpStreamingDecoder`, which correctly implements `zencodec::traits::decoder::StreamingDecode`. This is the expected association type pattern, not a leak.

## Flagged items

| # | Item | Category | Proposal | Confidence |
|---|------|----------|----------|------------|
| — | (none) | — | — | — |

**0 flagged. 0 % of surface.**

## Digest

webpx's public surface is well-bounded. FFI internals are `pub(crate)` throughout. Sealed traits do not leak as standalone public items. The re-export of `whereat`/`enough` types is documented and necessary for ergonomic error propagation. The `zencodec` module, heuristics, and compat shims are all intentional, feature-gated or documented public APIs.
