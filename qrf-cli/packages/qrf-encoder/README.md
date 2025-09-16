# qrf-encoder

QR Code File Encoder - Encode any file into a QR code video using fountain codes for redundancy.

## Installation

```bash
npm install -g qrf-encoder
```

## Usage

### Basic Encoding

```bash
qrf-encoder encode input.pdf output.mp4
```

### With Options

```bash
qrf-encoder encode document.pdf video.mp4 \
  --fps 10 \
  --redundancy 2.0 \
  --density high \
  --chunk-size 1024
```

## Options

- `-f, --fps <rate>` - Video frame rate (default: 10)
- `-c, --chunk-size <size>` - Chunk size in bytes (default: 1024)
- `-r, --redundancy <factor>` - Redundancy factor for fountain codes (default: 1.5)
- `-d, --density <level>` - QR code density: low/medium/high/ultra (default: high)
- `-e, --error-correction <level>` - QR error correction: L/M/Q/H (default: L)
- `-w, --width <pixels>` - Video width (default: 1080)
- `-h, --height <pixels>` - Video height (default: 1080)
- `--codec <codec>` - Video codec: libx264/libx265/libvpx-vp9 (default: libx264)
- `-v, --verbose` - Verbose output

## Features

- **Fountain Encoding**: Uses LT fountain codes for error recovery
- **Configurable Redundancy**: Adjust redundancy factor for reliability
- **Multiple Densities**: Choose QR density based on your needs
- **Progress Tracking**: Real-time encoding progress display
- **Batch Processing**: Encode multiple files (coming soon)

## How It Works

1. File is read and split into chunks
2. Fountain encoding generates redundant packets
3. Each packet is encoded as a QR code
4. QR codes are combined into an MP4 video
5. Metadata QR codes are repeated for reliability

## Requirements

- Node.js 18+
- FFmpeg installed on system
- Cairo graphics library (for canvas)

## Example

```bash
# Encode a 5MB PDF with 2x redundancy at 5 FPS
qrf-encoder encode report.pdf report_qr.mp4 --fps 5 --redundancy 2.0

# Output:
# 🎬 QRF Encoder v1.0.0
# 📄 Input:  report.pdf
# 📹 Output: report_qr.mp4
# ⚙️  Settings: FPS=5, Density=high, Redundancy=2.0
# ...
# ✅ Encoding complete!
```

## License

MIT