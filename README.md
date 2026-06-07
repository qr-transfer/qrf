# QRCoder: File Transmission via QR Codes

QRCoder is a robust browser-based system for transferring files between devices using animated QR codes, requiring no internet connection or direct file transfer protocols.

## Features

- **Offline Operation**: Transfer files without network connectivity
- **Device Agnostic**: Works between any devices with a camera and browser
- **Error-Resistant**: Fountain coding ensures reliable transfers even with missed frames
- **Parallel Processing**: Multi-threaded design with web workers for improved performance
- **Web-Based**: No app installation required
- **Video File Processing**: The decoder can now process video files containing QR code sequences
- **Automatic Recovery**: Progressive file reconstruction during transfer
- **Visual Progress Tracking**: Real-time visualization of chunk recovery
- **Adaptive Encoding**: Configurable encoding parameters with presets for different use cases
- **High Density Mode**: Support for larger QR codes with increased data capacity
- **Systematic-LT Hybrid**: Two-phase encoding combines systematic reliability with fountain code redundancy
- **Built-in Compression (airgap-safe)**: Files are compressed before encoding using **zstd** (embedded WebAssembly) with a native **gzip** fallback, so fewer QR frames are needed. The codec is embedded in the HTML — no network access required

## Components

QRCoder consists of two main components:

1. **Encoder (Transmitter)**: Converts files into a sequence of QR codes
2. **Decoder (Receiver)**: Captures QR codes and reconstructs the original file

### Encoder (Transmitter)

The encoder converts files into a sequence of QR codes that are displayed on screen:

1. File is read and converted to base64
2. Data is split into equal-sized chunks
3. Fountain coding creates redundant packets
4. QR codes are generated and displayed sequentially
5. Special metadata frame provides transfer details

### Decoder (Receiver)

The decoder captures QR codes using a camera or processes pre-recorded videos containing QR codes:

1. Input video file is selected or camera is initialized
2. QR codes are detected and processed in parallel
3. Metadata frame initializes file reconstruction
4. Fountain decoder processes incoming packets
5. File is reconstructed and automatically downloaded when complete

## Data Format

### Message Format

QRCoder uses a text-based protocol with two types of frames:

#### Metadata Frame

```
M:3.0:filename.ext:text%2Fplain:6499:26:40:2:0:10:250:30:M:abcd1234:tr4nsm1t:5.0:0.03:0.5:5:zstd:132347:1u112ny
```

Where (encoder version `5.0`):
- `M`: Indicates this is a metadata frame
- `3.0`: Protocol version
- `filename.ext`: Original filename (URL encoded)
- `text/plain`: File MIME type (URL encoded)
- `6499`: **Transmitted** payload size in bytes (post-compression)
- `26`: Number of chunks (of the transmitted payload)
- `40`: Maximum number of packets
- `2`: Systematic chunks per QR
- `0`: High-density mode flag
- `10`: FPS
- `250`: Chunk size in bytes
- `30`: Redundancy percent
- `M`: QR error-correction level
- `abcd1234`: Metadata checksum
- `tr4nsm1t`: **Transmitted** payload checksum (post-compression)
- `5.0`: Encoder version
- `0.03:0.5`: LT parameters (c, delta)
- `5`: Fountain max degree
- `zstd`: **Compression algorithm** (`zstd` | `gzip` | `none`) — appended in v5
- `132347`: **Original** file size in bytes (pre-compression) — appended in v5
- `1u112ny`: **Original** file checksum (FNV-1a) — appended in v5

> **Backward compatibility:** decoders default `compression` to `none` when the
> v5 fields are absent, so videos made with older encoders still decode.

### Compression (negotiated, airgap-safe)

The encoder picks an algorithm per file type and announces it in the metadata
frame; the decoder reads it, decompresses the reconstructed payload, and verifies
the **original** file checksum before delivery.

| File type | Algorithm |
|---|---|
| Text / logs / source code | `zstd` level 19 (level 12 for large/binary), `gzip` fallback |
| Web assets (HTML/JS/CSS/JSON/SVG) | `zstd` level 19, `gzip` fallback |
| Other binaries | `zstd` level 12, `gzip` fallback |
| Already-compressed (JPG/MP4/ZIP/PDF/…) | `none` (stored as-is — recompression gains ~0) |
| Incompressible result (no real gain) | `none` (stored as-is) |

Codecs are **embedded inline** as base64 (`zstd` WebAssembly in the encoder,
`fzstd` pure-JS decompressor in the decoders) so the tool works with **zero
network access**. `gzip` uses the browser-native `CompressionStream` /
`DecompressionStream` APIs. The reproducible embed script lives in
`.build-codecs/build.py`.

#### Data Frame

```
D:42:1234:10:100:3:base64EncodedData
```

Where:
- `D`: Indicates this is a data frame
- `42`: Packet ID
- `1234`: Seed value (for fountain coding)
- `10`: Seed base
- `100`: Total number of chunks
- `3`: Degree (number of chunks XORed together)
- `base64EncodedData`: The actual encoded data

## Fountain Coding Algorithm

QRCoder uses Luby Transform (LT) coding with Robust Soliton Distribution:

1. **Encoding**:
   - Creates redundant packets by XORing multiple chunks together
   - Each packet specifies which chunks were combined (via seed and degree)
   - Allows reconstruction with slightly more packets than original chunks

2. **Decoding**:
   - When a packet has only one unknown chunk, that chunk can be recovered
   - Newly recovered chunks are propagated to solve other packets
   - Recursively continues until all chunks are recovered

## Usage

### Encoder (vde-qr-encoder.html)

1. Open the encoder page in a browser (works offline via `file://` — no server needed)
2. Select a file (or several) to transfer
3. Configure transfer settings (chunk size, FPS, redundancy, density)
4. Start transmission — the file is compressed automatically before encoding.
   Watch the browser console for a line like
   `🗜️ app.js: zstd 30600 → 84 bytes (99.7% saved)` confirming the algorithm used
5. Position the receiving device to scan the QR codes (or screen-record for the decoder)

### Decoder (vdf-qr-decoder.html)

1. Open the decoder page in a browser (also works offline via `file://`)
2. Select a video file containing QR codes (or use the live camera)
3. Click "Start Scan" to begin processing
4. Monitor progress through the visual indicators
5. Use "Stop Scan" to pause or "Reset Contents" to start over
6. The file is decompressed and its **original checksum verified** before the
   automatic download (console shows `✅ Original file integrity verified`)

> **Browser support:** **Chrome / Chromium (or Edge) is recommended** for both
> the encoder and decoder — its camera and video-frame handling are the most
> reliable, and `vdf-qr-decoder.html` targets it. The `vdf-qr-decoder-firefox.html`
> and `vdf-qr-decoder-safari.html` variants carry browser-specific tweaks but were
> **not perfect** in testing; prefer Chrome/Chromium when you can.

### Other useful tools in this repo

- **`file_splitter.js` / `smart_file_splitter.js`** — split a large file into
  parts before encoding (and join them after decoding). Use when a single file is
  too large for one comfortable QR session.
- **`hierarchical_integrity_checker.js` / `file_integrity_checker.py`** — verify
  recovered files against expected checksums; `compare_integrity_reports.py`
  diffs two integrity runs.
- **`video-wakekeep.html` / `video-bunnykeep.html`** — keep the transmitting
  screen awake during long QR playback sessions.
- **`.build-codecs/build.py`** — reproducible script that re-embeds the zstd/fzstd
  codecs into the HTML files (run from the repo root after updating a codec).

## Technical Requirements

- Modern browser with HTML5 support (Chrome 80+, Firefox 113+, Safari 16.4+ for
  native gzip; zstd uses embedded WebAssembly)
- JavaScript enabled
- For optimal performance:
  - Good lighting conditions for camera-based scanning
  - Steady video with clear QR codes
  - Moderate distance between QR codes and camera

## Dependencies

All runtime dependencies are bundled — the tool needs **no network access**:

- HTML5-QRCode: QR code scanning library (decoder)
- QRCode.js: QR generation library (encoder)
- LTFountainCodes: Custom implementation of Luby Transform codes
- zstd-wasm (embedded) + fzstd (embedded): compression codecs

## Documentation

For more detailed information about each component:

- [Encoder Documentation](encoder.md): Comprehensive guide to the QR code encoder including encoding strategies and configuration options
- [Decoder Documentation](decoder.md): Comprehensive guide to the QR code decoder including processing pipeline and recovery mechanisms
- [Browser & Format Compatibility](COMPATIBILITY.md): Per-browser frame-capture strategy (Chrome rVFC vs Firefox/Safari rAF) and why **mp4** is the recommended input format
- API Reference (Coming Soon): Detailed API documentation for developers
