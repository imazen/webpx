# WebP Memory Usage Guide

This document describes the memory consumption patterns of libwebp encoding and decoding
operations, based on empirical measurements using heaptrack with libwebp 1.5.

## Quick Reference

### Encoding Memory Formulas

**Lossy Encoding:**
```
min = 115 KB + pixels × 13.4 bytes     (methods 0-2)
min = 220 KB + pixels × 13.7 bytes     (methods 3-6)
typ = min × 1.2                        (real photos)
max = min × 2.25                       (high-entropy/noise)
```

**Lossless Encoding:**
```
Method 0:
  base = 600 KB + pixels × 24 bytes
  min = base × 0.6    (solid color)
  typ = base × 1.2    (real photos)
  max = base × 1.5    (noise)

Methods 1-6:
  base = 1.5 MB + pixels × 34 bytes
  min = base × 0.6    (solid color)
  typ = base × 1.2    (real photos)
  max = base × 1.5    (noise)
```

### Quick Estimates

| Size | Lossy Typ | Lossy Max | Lossless M0 Typ | Lossless M4 Typ |
|------|-----------|-----------|-----------------|-----------------|
| 512×512 | 4.3 MB | 8.1 MB | 8.0 MB | 11.8 MB |
| 1024×1024 | 17.0 MB | 31.5 MB | 31.0 MB | 44.6 MB |
| 1920×1080 | 33.5 MB | 62.0 MB | 61.0 MB | 87.8 MB |
| 2048×2048 | 66.1 MB | 124.0 MB | 121.6 MB | 175.0 MB |
| 4096×4096 | 262.0 MB | 491.0 MB | 482.0 MB | 693.0 MB |

---

## Measurement Methodology

All measurements were collected using:
- **Tool:** heaptrack (intercepts all malloc/free including C library calls)
- **Library:** libwebp 1.5 via libwebp-sys
- **Platform:** Linux x86_64
- **Test images:** Synthetic (gradient, noise, solid) and real photos (CLIC2025)

### Why heaptrack?

Tools like Rust's dhat only intercept the Rust global allocator. Since libwebp is a C
library, its internal allocations would be missed. heaptrack uses LD_PRELOAD to intercept
all memory allocations system-wide, giving accurate measurements of actual peak memory.

---

## Lossy Encoding

### Method Impact

Method (0-6) has minimal impact on lossy memory usage (~3% variation):

| Size | Method 0-2 | Method 3-6 | Difference |
|------|------------|------------|------------|
| 512×512 | 3.46 MB | 3.58 MB | +3.5% |
| 1024×1024 | 13.52 MB | 14.01 MB | +3.6% |
| 2048×2048 | 53.73 MB | 55.06 MB | +2.5% |

**Formula (gradient baseline):**
```
Methods 0-2: peak = 115 KB + pixels × 13.4 bytes
Methods 3-6: peak = 220 KB + pixels × 13.7 bytes
```

### Content Type Impact (Major Finding)

Content complexity dramatically affects lossy encoding memory:

| Size | Gradient | Noise | Solid | Noise/Gradient |
|------|----------|-------|-------|----------------|
| 512×512 | 3.58 MB | 7.97 MB | 7.50 MB | 2.23× |
| 1024×1024 | 14.01 MB | 31.56 MB | 27.37 MB | 2.25× |

- **Gradient:** Best case (smooth color transitions)
- **Noise:** Worst case (2.25× more memory)
- **Solid:** Also high (1.95×, triggers different code paths)

### Real Photo Validation

Testing against CLIC2025 competition images:

| Image | Pixels | Est Typ | Actual | Error |
|-------|--------|---------|--------|-------|
| Photo 1 | 2.79M | 46.05 MB | 48.02 MB | -4% |
| Photo 2 | 2.79M | 46.05 MB | 39.79 MB | +16% |
| Photo 3 | 2.79M | 46.05 MB | 45.74 MB | +1% |
| Photo 4 | 2.79M | 46.05 MB | 48.93 MB | -6% |
| Photo 5 | 2.49M | 41.21 MB | 38.45 MB | +7% |

Real photos average ~1.17× the gradient baseline, so typical estimate uses 1.2× multiplier.

### Final Lossy Formulas

```
base = 220 KB + pixels × 13.7 bytes    (method 3-6, conservative)

min = base × 1.0     Best case: smooth gradients
typ = base × 1.2     Typical: real photographs
max = base × 2.25    Worst case: noise, high-entropy
```

---

## Lossless Encoding

### Method Impact (Significant)

Unlike lossy, method has major impact on lossless memory:

| Size | M0 | M1-2 | M3-6 | M0 vs M4 |
|------|-----|------|------|----------|
| 512×512 | 6.98 MB | 9.51 MB | 16.09 MB | -57% |
| 1024×1024 | 24.51 MB | 35.53 MB | 35.54 MB | -31% |
| 2048×2048 | 97.66 MB | 137.52 MB | 137.52 MB | -29% |

**Key finding:** Method 0 uses 30-57% less memory than methods 1+.

At large sizes (1024+), methods 1-6 converge to similar memory usage.

### Formulas by Method

**Method 0 (fastest, least memory):**
```
base = 600 KB + pixels × 24 bytes
```

**Methods 1-6 (better compression, more memory):**
```
base = 1.5 MB + pixels × 34 bytes
```

### Content Type Impact

| Size | Solid | Gradient | Noise | Solid/Gradient |
|------|-------|----------|-------|----------------|
| 512×512 | 5.98 MB | 16.09 MB | 19.06 MB | 0.37× |
| 1024×1024 | 21.36 MB | 35.54 MB | 49.42 MB | 0.60× |

- **Solid:** Best case (0.6× of gradient)
- **Gradient:** Baseline
- **Noise:** Worst case (1.4× of gradient)

### Final Lossless Formulas

**Method 0:**
```
base = 600 KB + pixels × 24 bytes

min = base × 0.6     Best case: solid color
typ = base × 1.2     Typical: real photographs
max = base × 1.5     Worst case: noise
```

**Methods 1-6:**
```
base = 1.5 MB + pixels × 34 bytes

min = base × 0.6     Best case: solid color
typ = base × 1.2     Typical: real photographs
max = base × 1.5     Worst case: noise
```

---

## Input Format Impact

### RGB vs RGBA

**No measurable difference.** libwebp internally converts to a common format regardless
of whether input is RGB (3 bytes/pixel) or RGBA (4 bytes/pixel).

| Size | RGB Input | RGBA Input |
|------|-----------|------------|
| 512×512 | 3.58 MB | 3.58 MB |
| 1024×1024 | 14.01 MB | 14.01 MB |
| 2048×2048 | 55.06 MB | 55.06 MB |

---

## Quality Impact

Quality setting (0-100) has negligible impact on peak memory for lossy encoding.
The memory is dominated by internal buffers sized for the image dimensions, not
the compression parameters.

---

## Decoding Memory

### Key Findings

**Decode memory is simpler than encode:**
- Lossy and lossless have nearly identical memory usage (~15 bytes/pixel)
- Content type has minimal impact (~5% variation, vs 2.25× for encode)
- Primary cost is output buffer + internal YUV/color conversion workspace

### Decoding Memory Formula

```
Lossy:    peak = 76 KB + pixels × 15 bytes
Lossless: peak = 133 KB + pixels × 15 bytes
```

### Quick Decode Estimates

| Size | Pixels | Lossy | Lossless | Output Buffer |
|------|--------|-------|----------|---------------|
| 256×256 | 65K | 1.06 MB | 1.11 MB | 0.26 MB |
| 512×512 | 262K | 4.01 MB | 4.08 MB | 1.0 MB |
| 1024×1024 | 1M | 15.81 MB | 15.91 MB | 4.0 MB |
| 1920×1080 | 2.1M | 31.2 MB | 31.3 MB | 8.3 MB |
| 2048×2048 | 4.2M | 63.02 MB | 63.22 MB | 16.8 MB |

### API Variants

| API | Memory | Notes |
|-----|--------|-------|
| `decode_rgba()` | Full formula | Standard decode, allocates output |
| `decode_rgba_into()` | Formula − output buffer | Zero-copy for lossy (saves ~4 bytes/pixel) |
| `Decoder::new().decode_rgba()` | Full formula | Builder API, same as decode_rgba() |

**Zero-copy savings:**
- Lossy: `decode_into` saves exactly the output buffer (pixels × 4 bytes for RGBA)
- Lossless: Internal VP8L allocations dominate; minimal savings observed

### Content Type Impact

Unlike encoding, content type has minimal impact on decode memory:

| Size | Gradient | Noise | Solid | Noise/Gradient |
|------|----------|-------|-------|----------------|
| 1024×1024 | 15.81 MB | 16.59 MB | 15.81 MB | 1.05× |

Decode memory varies only ~5% with content type, compared to 2.25× for lossy encoding.

### Decode vs Encode Comparison

At 1024×1024:
| Operation | Min | Typical | Max |
|-----------|-----|---------|-----|
| Lossy decode | 15.8 MB | 15.8 MB | 16.6 MB |
| Lossy encode | 14.0 MB | 16.8 MB | 31.5 MB |
| Lossless decode | 15.9 MB | 15.9 MB | 16.7 MB |
| Lossless encode (M0) | 14.7 MB | 29.4 MB | 36.8 MB |
| Lossless encode (M4) | 21.3 MB | 42.6 MB | 53.3 MB |

---

## API Usage

### Getting Encode Estimates

```rust
use webpx::heuristics::estimate_encode;
use webpx::EncoderConfig;

let config = EncoderConfig::default()
    .quality(85.0)
    .method(4);

let est = estimate_encode(1920, 1080, 4, &config);

println!("Memory estimates for 1920×1080 lossy encode:");
println!("  Min (smooth): {:.1} MB", est.peak_memory_bytes_min as f64 / 1e6);
println!("  Typical:      {:.1} MB", est.peak_memory_bytes as f64 / 1e6);
println!("  Max (noise):  {:.1} MB", est.peak_memory_bytes_max as f64 / 1e6);
```

### Getting Decode Estimates

```rust
use webpx::heuristics::{estimate_decode, estimate_decode_zerocopy};

// Standard decode (allocates output buffer)
let est = estimate_decode(1920, 1080, 4, false); // lossy source

println!("Decode estimates for 1920×1080:");
println!("  Output buffer: {:.1} MB", est.output_bytes as f64 / 1e6);
println!("  Peak memory:   {:.1} MB", est.peak_memory_bytes as f64 / 1e6);

// Zero-copy decode (caller provides output buffer)
let est_zc = estimate_decode_zerocopy(1920, 1080, false);
println!("  Zero-copy peak: {:.1} MB", est_zc.peak_memory_bytes as f64 / 1e6);
```

### Choosing the Right Estimate

- **Memory budgeting:** Use `peak_memory_bytes_max` for safety
- **Progress bars:** Use `peak_memory_bytes` (typical)
- **Capacity planning:** Use `peak_memory_bytes` with 20% headroom

---

## Recommendations

### For Memory-Constrained Environments

1. **Use lossy encoding** - 2-3× less memory than lossless
2. **Use method 0 for lossless** - 30-57% less memory than method 4+
3. **Process in tiles** for very large images
4. **Pre-allocate** based on `peak_memory_bytes_max`

### For Throughput Optimization

1. **Method has minimal impact** on lossy memory, choose based on quality/speed
2. **Batch similar-sized images** to reuse allocator pools
3. **Consider streaming API** for memory-constrained scenarios

---

## Raw Measurement Data

### Lossy Encoding by Method (gradient images)

| Size | M0 | M1 | M2 | M3 | M4 | M5 | M6 |
|------|-----|-----|-----|-----|-----|-----|-----|
| 512×512 | 3.46 | 3.46 | 3.46 | 3.58 | 3.58 | 3.58 | 3.58 |
| 1024×1024 | 13.52 | 13.52 | 13.52 | 14.01 | 14.01 | 14.01 | 14.01 |
| 2048×2048 | 53.73 | 53.73 | 53.73 | 55.06 | 55.06 | 55.06 | 55.06 |

### Lossless Encoding by Method (gradient images)

| Size | M0 | M1 | M2 | M3 | M4 | M5 | M6 |
|------|------|------|------|------|------|------|------|
| 512×512 | 6.98 | 9.51 | 9.51 | 16.09 | 16.09 | 16.22 | 16.09 |
| 1024×1024 | 24.51 | 35.53 | 35.54 | 35.54 | 35.54 | 36.59 | 36.06 |
| 2048×2048 | 97.66 | 137.52 | 137.52 | 137.52 | 137.52 | 139.62 | 137.52 |

---

## References

- [libwebp documentation](https://developers.google.com/speed/webp/docs/api)
- [heaptrack](https://github.com/KDE/heaptrack) - heap memory profiler
- Measurements: `examples/mem_formula.rs`, `examples/real_image_test.rs`
