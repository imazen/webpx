# libwebp API Reference

Downloaded from https://developers.google.com/speed/webp/docs/api

## Core Headers

The library installs three main headers:
- `decode.h` - decoding functions
- `encode.h` - encoding functions
- `types.h` - type definitions

## Decoding APIs

### Simple Decoding Functions

**WebPGetInfo()** - Validates header and retrieves dimensions:
- Parameters: `const uint8_t* data`, `size_t data_size`, `int* width`, `int* height`
- Returns: boolean (true on success)
- Width/height range: 1 to 16383 pixels

**WebPGetFeatures()** - Extracts bitstream metadata:
- Populates `WebPBitstreamFeatures` structure with width, height, alpha channel presence, animation flag, and format (lossy/lossless)
- Returns: `VP8StatusCode` (e.g., `VP8_STATUS_OK`, `VP8_STATUS_NOT_ENOUGH_DATA`)

### Color Format Decoders

Six variants handle different channel orderings:
- `WebPDecodeRGBA()`, `WebPDecodeARGB()`, `WebPDecodeBGRA()`, `WebPDecodeRGB()`, `WebPDecodeBGR()`
- All return `uint8_t*` pointer to decoded samples
- Caller must invoke `WebPFree()` to release memory

### In-Buffer Variants (Zero-Copy Target)

Decode directly into pre-allocated memory:
- `WebPDecodeRGBAInto()`, `WebPDecodeARGBInto()`, `WebPDecodeBGRAInto()`, `WebPDecodeRGBInto()`, `WebPDecodeBGRInto()`
- Parameters: `uint8_t* output_buffer`, `int output_buffer_size`, `int output_stride`
- `output_stride` specifies bytes between scanlines
- Returns `NULL` on failure; otherwise returns the output buffer

### Advanced Decoding with WebPDecoderConfig

**WebPDecoderConfig** structure manages:
- Input bitstream features
- Output colorspace selection (MODE_BGRA, MODE_RGBA, MODE_YUV, MODE_YUVA, etc.)
- Cropping parameters: `crop_left`, `crop_top`, `crop_width`, `crop_height`
- Scaling options: `scaled_width`, `scaled_height`
- Additional processing: multi-threading, dithering, vertical flip

**External Buffer Support:**
- Set `config.output.is_external_memory = 1`
- Provide buffer via `config.output.u.RGBA.rgba`, `.size`, `.stride`

**Decoding Methods:**
1. Full decode: `WebPDecode(data, data_size, &config)`
2. Incremental: `WebPINewDecoder()` → `WebPIAppend()` → `WebPIDelete()`

## Encoding APIs

### Simple Encoding Functions

**Lossy encoders** - Accept quality parameter (0-100):
- `WebPEncodeRGB()`, `WebPEncodeBGR()`, `WebPEncodeRGBA()`, `WebPEncodeBGRA()`
- Parameters: pixel data, width, height, **stride**, quality_factor, output pointer
- Returns: compressed byte count (0 on failure)

**Lossless encoders**:
- `WebPEncodeLosslessRGB()`, `WebPEncodeLosslessBGR()`, `WebPEncodeLosslessRGBA()`, `WebPEncodeLosslessBGRA()`
- Note: "RGB values in fully transparent areas will be modified" unless `WebPConfig::exact` is enabled

### Advanced Encoding with WebPPicture

**WebPPicture** structure - holds input samples:
- `use_argb` flag selects format (1 = ARGB/BGRA, 0 = YUV)
- ARGB input: `uint32_t* argb` with `argb_stride` (stride in **pixels**, not bytes!)
- YUV input: `uint8_t *y, *u, *v` with `y_stride`, `uv_stride`
- Optional alpha plane: `uint8_t* a`, `a_stride`
- Writer callback: `WebPWriterFunction writer`, `void* custom_ptr`

### Import Functions (Support Stride)

```c
int WebPPictureImportRGBA(WebPPicture* picture, const uint8_t* rgba, int rgba_stride);
int WebPPictureImportRGBX(WebPPicture* picture, const uint8_t* rgbx, int rgbx_stride);
int WebPPictureImportBGRA(WebPPicture* picture, const uint8_t* bgra, int bgra_stride);
int WebPPictureImportBGRX(WebPPicture* picture, const uint8_t* bgrx, int bgrx_stride);
int WebPPictureImportRGB(WebPPicture* picture, const uint8_t* rgb, int rgb_stride);
int WebPPictureImportBGR(WebPPicture* picture, const uint8_t* bgr, int bgr_stride);
```

These functions:
- Copy pixel data into the picture's internal buffer
- Handle stride (bytes between rows) correctly
- Convert to internal ARGB format

**Note:** Import functions DO copy data. For true zero-copy, you'd need to set `picture.argb` directly, but:
- `argb_stride` is in pixels, not bytes
- Data must be in ARGB format (not RGBA/BGRA)

### Encoding Workflow

1. Initialize: `WebPConfigPreset(&config, preset, quality)` + `WebPValidateConfig()`
2. Setup picture: `WebPPictureInit()` → `WebPPictureImport*()` or direct assignment
3. Configure writer: `WebPMemoryWriterInit()` + assign to `pic.writer`
4. Encode: `WebPEncode(&config, &pic)`
5. Cleanup: `WebPPictureFree(&pic)`, `WebPMemoryWriterClear()`

## Memory Management

- `WebPFree()` - releases decoder output buffers
- `WebPFreeDecBuffer()` - safely reclaims config output memory (works with external buffers)
- `WebPMemoryWriterClear()` - releases memory writer buffer
- External buffer support via `config.output.is_external_memory = 1`

## Key Constraints

- Maximum dimensions: 16383 × 16383 pixels
- **ARGB stride measured in pixels (not bytes)**
- YUV/YUVA recommended for lossy; ARGB for lossless compression
- Import functions always copy; direct argb assignment possible but requires ARGB format
