# libwebp Container API Reference

Downloaded from https://developers.google.com/speed/webp/docs/container-api

## Mux API Overview

The Mux API enables manipulation of WebP container images with support for color profiles, metadata, animation, and fragmented images.

### Core Mux Functions

**Creation & Lifecycle:**
- `WebPMuxNew()` - Creates an empty mux object
- `WebPMuxCreate(const WebPData* bitstream, int copy_data)` - Constructs mux from WebP RIFF data
- `WebPMuxDelete(WebPMux* mux)` - Deallocates mux object

**Non-Image Chunk Management:**
- `WebPMuxSetChunk()` - Adds/replaces chunks (ICCP, XMP, EXIF, etc.)
- `WebPMuxGetChunk()` - Retrieves chunk data reference
- `WebPMuxDeleteChunk()` - Removes specified chunk

**Image & Frame Handling:**
- `WebPMuxSetImage(WebPMux* mux, const WebPData* bitstream, int copy_data)` - Sets single image
- `WebPMuxPushFrame()` - Appends animated/fragmented frame
- `WebPMuxGetFrame()` - Retrieves frame
- `WebPMuxDeleteFrame()` - Removes frame

**Animation Control:**
- `WebPMuxSetAnimationParams()` - Configures animation
- `WebPMuxGetAnimationParams()` - Fetches animation settings

**Utilities:**
- `WebPMuxGetCanvasSize()` - Retrieves dimensions
- `WebPMuxGetFeatures()` - Obtains feature flags
- `WebPMuxNumChunks()` - Counts chunks by type
- `WebPMuxAssemble()` - Generates final WebP bitstream

### Data Structures

**WebPMuxFrameInfo:**
```c
WebPData bitstream;      // VP8/VP8L image data
int x_offset, y_offset;  // Positioning
int duration;            // Display time in milliseconds
uint32_t id;             // Frame type (ANMF, FRGM, IMAGE)
WebPMuxAnimDispose dispose_method;
WebPMuxAnimBlend blend_method;
```

**WebPMuxAnimParams:**
```c
uint32_t bgcolor;   // Canvas background color (ARGB in MSB order)
int loop_count;     // Repeat iterations (0 = infinite)
```

### Error Codes

`WEBP_MUX_OK`, `WEBP_MUX_NOT_FOUND`, `WEBP_MUX_INVALID_ARGUMENT`, `WEBP_MUX_BAD_DATA`, `WEBP_MUX_MEMORY_ERROR`, `WEBP_MUX_NOT_ENOUGH_DATA`

## WebPAnimEncoder API

Enables encoding animated WebP sequences.

**Key Functions:**
- `WebPAnimEncoderOptionsInit()` - Initializes encoder config
- `WebPAnimEncoderNew(width, height, enc_options)` - Creates encoder
- `WebPAnimEncoderAdd(enc, frame, timestamp_ms, config)` - Adds frame
- `WebPAnimEncoderAssemble(enc, webp_data)` - Produces final bitstream
- `WebPAnimEncoderGetError(enc)` - Retrieves error message
- `WebPAnimEncoderDelete(enc)` - Deallocates encoder

**WebPAnimEncoderOptions:**
```c
WebPMuxAnimParams anim_params;  // Animation configuration
int minimize_size;              // Enables compression optimization
int kmin, kmax;                 // Keyframe distance constraints
int allow_mixed;                // Permits lossy/lossless frame mixing
int verbose;                    // Stderr diagnostic output
```

## Demux API

Extracts image and metadata from WebP files.

**Core Functions:**
- `WebPDemux(const WebPData* data)` - Parses complete WebP file
- `WebPDemuxPartial(data, state)` - Handles incomplete streams
- `WebPDemuxDelete(dmux)` - Frees demuxer resources

**Information Retrieval:**
- `WebPDemuxGetI(dmux, feature)` - Fetches format properties
  - `WEBP_FF_FORMAT_FLAGS` - Feature flags
  - `WEBP_FF_CANVAS_WIDTH`, `WEBP_FF_CANVAS_HEIGHT`
  - `WEBP_FF_LOOP_COUNT`
  - `WEBP_FF_BACKGROUND_COLOR`
  - `WEBP_FF_FRAME_COUNT`

**Frame Iteration:**
- `WebPDemuxGetFrame(dmux, frame_number, iter)` - Accesses specific frame
- `WebPDemuxNextFrame(iter)` / `WebPDemuxPrevFrame(iter)`
- `WebPDemuxReleaseIterator(iter)` - Deallocates iterator

**Chunk Iteration:**
- `WebPDemuxGetChunk(dmux, fourcc, chunk_number, iter)`
- `WebPDemuxNextChunk(iter)` / `WebPDemuxPrevChunk(iter)`
- `WebPDemuxReleaseChunkIterator(iter)`

## WebPAnimDecoder API

Decodes animated WebP images with frame-by-frame access.

**Primary Functions:**
- `WebPAnimDecoderOptionsInit(dec_options)` - Prepares decoder configuration
- `WebPAnimDecoderNew(webp_data, dec_options)` - Instantiates decoder
- `WebPAnimDecoderGetInfo(dec, info)` - Obtains global animation metadata
- `WebPAnimDecoderGetNext(dec, &buf, &timestamp)` - Retrieves rendered frame
- `WebPAnimDecoderHasMoreFrames(dec)` - Tests for remaining frames
- `WebPAnimDecoderReset(dec)` - Restarts decoding sequence
- `WebPAnimDecoderDelete(dec)` - Destroys decoder

**WebPAnimDecoderOptions:**
```c
WEBP_CSP_MODE color_mode;  // Output colorspace (MODE_RGBA, MODE_BGRA, MODE_rgbA, MODE_bgrA)
int use_threads;           // Multi-threaded decoding
```

**WebPAnimInfo:**
```c
uint32_t canvas_width, canvas_height;
uint32_t loop_count;
uint32_t bgcolor;
uint32_t frame_count;
```

## Buffer Management

**Data Ownership:**
- Functions returning pointers allocate via `malloc()` and require caller deallocation
- `WebPDataClear(WebPData* webp_data)` deallocates contents via `free()`
- References obtained from mux/demux objects should NOT be manually freed

**WebPData structure:**
```c
typedef struct {
  const uint8_t* bytes;
  size_t size;
} WebPData;
```

## Important Notes

- Chunk APIs exclude image-related chunks (ANMF, FRGM, VP8, VP8L, ALPH) - use image functions instead
- Odd frame offsets automatically snap to even boundaries
- Animation timestamps are END times, not START times
- `WebPAnimDecoderGetNext()` returns pointer to internal buffer - valid until next call or delete
