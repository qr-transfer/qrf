# QRF CLI

Command-line tools for encoding and decoding files using QR codes with fountain encoding.

## Installation

```bash
npm install
npm link  # Optional: install globally
```

## Usage

### Encoding

Encode a file into a QR code video:

```bash
node encoder-simple.js encode <input-file> <output-video> [options]

# Example
node encoder-simple.js encode document.pdf output.mp4 --fps 10 --redundancy 1.5
```

Options:
- `-f, --fps <rate>` - Video frame rate (default: 10)
- `-c, --chunk-size <size>` - Chunk size in bytes (default: 1024)
- `-r, --redundancy <factor>` - Redundancy factor for fountain codes (default: 1.5)
- `-d, --density <level>` - QR code density: low/medium/high/ultra (default: high)
- `-e, --error-correction <level>` - Error correction level: L/M/Q/H (default: L)
- `-w, --width <pixels>` - Video width (default: 1080)
- `-h, --height <pixels>` - Video height (default: 1080)

### Decoding

Decode QR codes from a video file:

```bash
node index.js decode <video-file> [options]

# Example
node index.js decode input.mp4 --fps 1 --output ./decoded
```

Options:
- `-f, --fps <rate>` - Frame processing rate (default: 1)
- `--fast` - Fast scan mode (metadata only)
- `-o, --output <dir>` - Output directory for decoded files (default: ./decoded)
- `--json <file>` - Import scan data from JSON

## Features

- **Fountain Encoding**: Uses fountain codes for redundancy and error recovery
- **Progressive Decoding**: Files can be recovered even from partial video
- **Multi-file Support**: Can encode and decode multiple files in a single video
- **Fast Scanning**: Quick metadata discovery mode
- **Error Resilience**: Handles damaged or missing QR codes

## Requirements

- Node.js 18+
- FFmpeg (for video processing)
- Canvas dependencies (cairo, pango, etc.)

## License

MIT