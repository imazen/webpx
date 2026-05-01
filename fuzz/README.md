# webpx fuzzing

Coverage-guided fuzz targets for webpx, run with [`cargo-fuzz`].

## Targets

| Target | Surface |
|--------|---------|
| `image_info` | Header probe (`ImageInfo::from_webp`). Cheap, fast iteration. |
| `decode_static` | All top-level decode functions: `decode_rgba`/`rgb`/`bgra`/`bgr`/`yuv`, plus the typed `decode<P>` generic. |
| `decode_into` | `_into` variants with caller-provided buffers and arbitrary stride padding. Probes for OOB writes on under/over-allocated buffers. |
| `decoder_builder` | `Decoder` builder with crop and rescale. Targets the historically-vulnerable advanced libwebp path (CVE-2023-4863 / Huffman + rescaler). |
| `decode_streaming` | `StreamingDecoder` fed in arbitrary chunk sizes; both append-mode and `with_buffer` constructors. |
| `decode_animation` | `AnimationDecoder` frame-by-frame, plus `decode_all` and `reset`. |
| `mux_metadata` | ICC / EXIF / XMP get + embed + remove round trips. Demux + mux paths. |
| `encode_roundtrip` | Encode pixels under all knobs, decode the result, assert dimensions match. Includes animation encoding. |

## Running

```bash
# Need the nightly toolchain for cargo-fuzz
cargo install cargo-fuzz

# One-shot run, 60s
cargo +nightly fuzz run image_info -- -max_total_time=60

# Use the dictionary
cargo +nightly fuzz run decode_static fuzz/corpus/decode_static \
  -- -dict=fuzz/webp.dict -max_total_time=300

# Reuse the seed corpus
cp fuzz/seeds/*.webp fuzz/corpus/decode_static/
```

## Layout

- `fuzz_targets/` — target source code, tracked.
- `webp.dict` — libFuzzer dictionary covering RIFF, VP8, VP8L, ALPH, ANIM/ANMF, VP8X, ICC/EXIF/XMP. Tracked.
- `seeds/` — small hand-curated valid inputs (~28 KB total). Tracked.
- `regression/` — minimized POCs for previously-fixed crashes. Tracked when populated. Each file ≤ 8 KB.
- `corpus/` — accumulated working corpus, gitignored. Sync to `/mnt/v/fuzzes/webpx/` per the repo-wide protocol.
- `artifacts/` — raw libFuzzer crash artifacts, gitignored.

## OOM artifacts are DoS findings

webpx is consumed by image-processing services that handle untrusted user
uploads. An attacker-controlled input that drives libwebp into very large
allocations is a denial-of-service vector. **`oom-*` artifacts produced by
any fuzz target should be triaged the same as `crash-*` — they're not
noise.**

The fuzz targets gate on a defensive `MAX_PIXELS` so the *fuzzer* doesn't
poison itself, but artifacts that slip past those gates (e.g., inputs that
decode to a small image but force huge intermediate libwebp buffers) are
real findings. Mirror them into `/mnt/v/fuzzes/webpx/artifacts/<target>/`,
attempt minimization with a generous `-rss_limit_mb`, and either commit
the minimized POC (≤8 KB) to `fuzz/regression/` or reference the block-
storage path from the issue/PR if it's larger.

A configurable `DecoderConfig::max_pixels` in webpx itself is the right
upstream fix; tracked separately.

## Regression gate

`tests/fuzz_regression.rs` runs every file in `fuzz/regression/` through every
fuzz-target entry point on the stable toolchain. It runs as part of
`cargo test --all-features` (or `just test`).

To add a regression seed: minimize the crashing input with
`cargo +nightly fuzz tmin <target> <crash-file>`, drop the result into
`fuzz/regression/` named `crash-<sha>`, and commit.

[`cargo-fuzz`]: https://github.com/rust-fuzz/cargo-fuzz
