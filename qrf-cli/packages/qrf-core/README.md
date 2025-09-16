# @qrf/core

Core libraries for QRF encoder and decoder - fountain codes, QR generation, and video processing.

## Installation

```bash
npm install @qrf/core
```

## Usage

```javascript
import {
  FileProcessor,
  FountainEncoder,
  FountainDecoder,
  QRGenerator,
  QRDecoder,
  VideoEncoder,
  VideoProcessor
} from '@qrf/core';

// File processing
const processor = new FileProcessor();
const fileData = await processor.readFile('document.pdf');
const chunks = await processor.splitIntoChunks(fileData.buffer);

// Fountain encoding
const encoder = new FountainEncoder();
const packets = await encoder.encode(chunks, { redundancy: 1.5 });

// QR generation
const qrGen = new QRGenerator({ density: 'high' });
const qrCode = await qrGen.generateDataPacket(packet, metadata);

// Video encoding
const videoEnc = new VideoEncoder({ fps: 10 });
await videoEnc.createVideo(qrFrames);
```

## Modules

### FileProcessor
- `readFile(path)` - Read file with metadata
- `splitIntoChunks(buffer, options)` - Split data into chunks
- `calculateChecksum(buffer)` - Calculate SHA256 checksum
- `combineChunks(chunks)` - Combine chunks back to file

### FountainEncoder
- `encode(chunks, options)` - Generate fountain-coded packets
- Options: `redundancy`, `systematic`

### FountainDecoder
- `initialize(metadata)` - Initialize with file metadata
- `addPacket(packet)` - Add received packet
- `getRecoveryProgress()` - Get recovery status
- `finalizeFile()` - Reconstruct complete file

### QRGenerator
- `generateMetadata(metadata)` - Create metadata QR code
- `generateDataPacket(packet, metadata)` - Create data QR code
- Options: `density`, `errorCorrection`

### QRDecoder
- `decode(frameData)` - Decode QR from image frame
- Returns parsed metadata or data packet

### VideoEncoder
- `createVideo(frames, progressCallback)` - Encode frames to video
- Options: `fps`, `width`, `height`, `codec`

### VideoProcessor
- `start()` - Begin processing video
- Events: `frame`, `progress`, `complete`, `error`
- Options: `frameRate`, `fastScan`

## License

MIT