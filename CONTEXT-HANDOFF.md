# Context Handoff: Decode Memory Profiling

## Summary

Encoding memory profiling is complete. This handoff is for completing the **decoding** memory profiling to the same level of detail.

## What's Done

### Encoding (Complete)
- Measured all methods (0-6) for lossy and lossless
- Measured content type impact (gradient, noise, solid)
- Validated against real photos (CLIC2025 test set)
- Derived min/typ/max formulas
- Updated `src/heuristics.rs` with `estimate_encode()` returning `EncodeEstimate` with:
  - `peak_memory_bytes_min`
  - `peak_memory_bytes` (typical)
  - `peak_memory_bytes_max`
- Created `docs/webp-memory-use.md` with all findings
- Created `examples/mem_formula.rs` for data collection
- Created `examples/real_image_test.rs` for validation

### Key Findings (Encoding)
- **RGB vs RGBA:** No difference (libwebp converts internally)
- **Lossy method:** ~3% impact on memory
- **Lossless method:** Method 0 uses 30-57% less than methods 1+
- **Content type:**
  - Lossy: noise uses 2.25× more than gradient
  - Lossless: solid uses 0.6×, noise uses 1.4× of gradient
- **Real photos:** Average ~1.2× of gradient baseline

## What's Needed: Decode Profiling

### Tasks

1. **Update `examples/mem_formula.rs`** to support decode modes properly:
   - `--mode decode-lossy` - decode from lossy-encoded WebP
   - `--mode decode-lossless` - decode from lossless-encoded WebP
   - Test with different output formats (RGBA, RGB, YUV)

2. **Collect heaptrack data for decoding:**
   ```bash
   for size in 512 1024 2048; do
     for source in lossy lossless; do
       heaptrack ./target/release/examples/mem_formula \
         --size $size --mode decode-$source
     done
   done
   ```

3. **Measure decode API variants:**
   - `decode_rgba()` - allocates output buffer
   - `decode_rgba_into()` - zero-copy into pre-allocated buffer
   - `Decoder::new().scale().decode()` - with scaling
   - `Decoder::new().crop().decode()` - with cropping
   - `StreamingDecoder` - incremental decoding

4. **Test content type impact on decode** (likely minimal, but verify)

5. **Update `estimate_decode()` in `src/heuristics.rs`:**
   - Currently uses rough estimates (encode cost / 2)
   - Should have measured formulas
   - Add min/typ/max like encoding

6. **Update `DecodeEstimate` struct** to match `EncodeEstimate`:
   ```rust
   pub struct DecodeEstimate {
       pub peak_memory_bytes_min: u64,
       pub peak_memory_bytes: u64,
       pub peak_memory_bytes_max: u64,
       // ...
   }
   ```

7. **Update `docs/webp-memory-use.md`** with decode findings

### Current Decode Estimate Code

Location: `src/heuristics.rs:280-310`

```rust
pub fn estimate_decode(
    width: u32,
    height: u32,
    output_bpp: u8,
    is_lossless: bool,
) -> DecodeEstimate {
    let pixels = (width as u64) * (height as u64);
    let output_bytes = pixels * (output_bpp as u64);

    // TODO: These are rough estimates, need heaptrack validation
    let peak_memory_bytes = if is_lossless {
        LOSSLESS_M1_FIXED_OVERHEAD / 2
            + (pixels as f64 * LOSSLESS_M1_BYTES_PER_PIXEL / 2.0) as u64
    } else {
        LOSSY_M3_FIXED_OVERHEAD + (pixels as f64 * LOSSY_M3_BYTES_PER_PIXEL / 2.0) as u64
    };
    // ...
}
```

### Expected Decode Characteristics

Based on libwebp architecture:
- Decode memory is typically less than encode
- Primary costs: output buffer + internal decode state
- Lossless decode needs hash tables for backward references
- Scaling/cropping may reduce memory (smaller output)
- Zero-copy (`decode_into`) should save output buffer allocation

### Files to Modify

1. `examples/mem_formula.rs` - Add decode measurement support
2. `src/heuristics.rs` - Update `estimate_decode()`, `DecodeEstimate`
3. `docs/webp-memory-use.md` - Add decode section

### Test Commands

```bash
# Build
cargo build --release --all-features --example mem_formula

# Collect decode data
just mem-formula size=1024 mode=decode-lossy
just mem-formula size=1024 mode=decode-lossless

# Run all tests after changes
cargo test --all-features heuristics
```

## Key Files

- `src/heuristics.rs` - Memory estimation functions
- `examples/mem_formula.rs` - Data collection tool
- `examples/real_image_test.rs` - Real image validation
- `docs/webp-memory-use.md` - Documentation
- `mem_data/` - Collected measurement data (gitignored)

## Encoding Formulas (Reference)

```
LOSSY:
  base = 220 KB + pixels × 13.7
  min = base × 1.0, typ = base × 1.2, max = base × 2.25

LOSSLESS Method 0:
  base = 600 KB + pixels × 24
  min = base × 0.6, typ = base × 1.2, max = base × 1.5

LOSSLESS Methods 1-6:
  base = 1.5 MB + pixels × 34
  min = base × 0.6, typ = base × 1.2, max = base × 1.5
```
