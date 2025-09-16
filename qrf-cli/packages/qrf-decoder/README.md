# qrf-decoder

QR Code File Decoder - Decode QR code videos back to original files using fountain codes.

## Installation

```bash
npm install -g qrf-decoder
```

## Usage

### Basic Decoding

```bash
qrf-decoder decode video.mp4
```

### With Options

```bash
qrf-decoder decode video.mp4 \
  --output ./recovered \
  --fps 2 \
  --verbose
```

## Commands

### `decode <video>`
Decode QR codes from video file

Options:
- `-f, --fps <rate>` - Frame processing rate (default: 1)
- `--fast` - Fast scan mode
- `-o, --output <dir>` - Output directory (default: ./decoded)
- `-v, --verbose` - Verbose output

### `scan <video>`
Fast scan to discover files in video

Options:
- `-o, --output <file>` - Output JSON file (default: scan.json)

### `extract <video> <file>`
Extract specific file from video (coming soon)

Options:
- `-o, --output <dir>` - Output directory
- `-j, --json <file>` - Use scan data from JSON

## Features

- **Progressive Decoding**: Recovers files as soon as enough chunks are received
- **Multi-file Support**: Decode multiple files from a single video
- **Fast Scanning**: Quickly discover all files without full decoding
- **Error Recovery**: Uses fountain codes to recover from missing QR codes
- **Real-time Progress**: Live progress tracking during decoding

## How It Works

1. Video frames are extracted at specified FPS
2. QR codes are detected and decoded from each frame
3. Metadata packets identify files
4. Data packets are collected using fountain decoding
5. Files are reconstructed once enough packets are received
6. Recovered files are saved to output directory

## Example

```bash
# Decode a video at 2 FPS
qrf-decoder decode encoded_video.mp4 --fps 2 --output ./files

# Output:
# 🎬 QRF Decoder v1.0.0
# 📹 Input:  encoded_video.mp4
# 📁 Output: ./files
# ⚙️  Settings: FPS=2, Mode=normal
# ...
# 📄 Discovered: document.pdf (450 chunks)
# ✓ Recovered: document.pdf
# ✅ Decoding complete!

# Fast scan for metadata
qrf-decoder scan video.mp4 -o files.json
```

## Requirements

- Node.js 18+
- FFmpeg installed on system

## License

MIT