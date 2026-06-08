# Release Notes

## v1.3.0 (encoder) / v2.3.0 (decoder)

- Fully self-contained / airgap (all libs inlined: QRCode, html5-qrcode, zstd, fzstd, jsQR)
- Cross-browser capture: Chrome rVFC; Firefox jsQR Web-Worker pipeline; Safari rAF
- Professional calm UI with IBM Carbon icons

## Compression release — encoder v1.2.0 / decoder v2.2.0

### 🗜️ Built-in compression (airgap-safe)

Files are now compressed **before** QR encoding, so fewer frames are transmitted
and transfers are faster — with no network access required.

- **Algorithms:** `zstd` (embedded WebAssembly, `@bokuweb/zstd-wasm`) with a
  native **gzip** fallback (`CompressionStream`). The decoders embed the pure-JS
  `fzstd` decompressor; gzip uses native `DecompressionStream`.
- **Per-file-type selection:** text/logs/code/web → zstd-19 (zstd-12 for
  large/binary) → gzip → store raw; already-compressed formats (JPG/MP4/ZIP/PDF/…)
  and incompressible results are stored as-is.
- **Negotiation:** the algorithm, original size, and original checksum are
  appended to the `M:` metadata frame (encoder version bumped to `5.0`). The
  decoder decompresses and verifies the **original** file checksum before
  delivery.
- **Backward compatible:** videos from older encoders (no compression fields)
  still decode — `compression` defaults to `none`.
- **Fully embedded / offline:** codecs are inlined as base64; the HTML files run
  from `file://` with zero external requests. Reproducible embed via
  `.build-codecs/build.py`.

### ✅ Verified (Node, deterministic)

Round-trips of the exact embedded codecs pass byte-for-byte (zstd→fzstd and gzip),
with encoder/decoder FNV-1a checksum parity, incompressible-data handling, and
metadata field-alignment incl. legacy v4 backward-compat.

---

# Release Notes - v1.0.0

## 🎉 Initial Release

QRF (QR File) is a complete solution for encoding files into QR code videos and decoding them back using fountain codes for error recovery.

### 📦 Packages

This release includes three npm packages:

- **@qrf/core** - Core libraries for QR generation, fountain coding, and video processing
- **qrf-encoder** - CLI tool for encoding files into QR videos
- **qrf-decoder** - CLI tool for decoding QR videos back to files

### ✨ Features

- 🎬 Encode any file into QR code video (MP4)
- 📹 Decode QR videos to recover original files
- 💧 Fountain codes for redundancy and error recovery
- 📊 Real-time progress tracking
- 🎨 Colorful CLI interface
- ⚡ Fast scanning mode for metadata discovery
- 📁 Multi-file support in single video
- 🔧 Configurable parameters (FPS, density, redundancy)

### 📥 Installation

```bash
# Install encoder
npm install -g qrf-encoder

# Install decoder
npm install -g qrf-decoder

# Or install core library for development
npm install @qrf/core
```

### 🚀 Quick Start

```bash
# Encode a file
qrf-encoder encode document.pdf output.mp4

# Decode a video
qrf-decoder decode output.mp4 --output ./recovered
```

### 🔧 Requirements

- Node.js 18+
- FFmpeg
- Cairo graphics library (for QR generation)

### 📝 Documentation

See individual package READMEs for detailed documentation:
- [qrf-encoder README](./qrf-cli/packages/qrf-encoder/README.md)
- [qrf-decoder README](./qrf-cli/packages/qrf-decoder/README.md)
- [@qrf/core README](./qrf-cli/packages/qrf-core/README.md)

### 🙏 Acknowledgments

Uses fountain codes (LT codes) for robust error recovery and redundancy.

---

## How to Publish to NPM

1. **Login to npm:**
```bash
npm login
```

2. **Publish packages in order:**
```bash
# From qrf-cli directory
cd packages/qrf-core && npm publish --access public
cd ../qrf-encoder && npm publish
cd ../qrf-decoder && npm publish
```

3. **Or use workspace scripts:**
```bash
npm run publish:all
```

## Creating GitHub Release

1. Go to https://github.com/qr-transfer/qrf/releases
2. Click "Draft a new release"
3. Choose tag: v1.0.0
4. Title: "QRF v1.0.0 - QR Code File Encoder/Decoder"
5. Copy the content above into the description
6. Publish release