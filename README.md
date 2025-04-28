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
M:1.0:filename.ext:image/jpeg:1024000:100:150:8:2:base64:1024
```

Where:
- `M`: Indicates this is a metadata frame
- `1.0`: Protocol version
- `filename.ext`: Original filename (URL encoded)
- `image/jpeg`: File MIME type (URL encoded)
- `1024000`: File size in bytes
- `100`: Number of chunks
- `150`: Maximum number of packets
- `8`: Maximum degree (for fountain coding)
- `2`: Density parameter
- `base64`: Encoding format
- `1024`: Chunk size in bytes

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

1. Open the encoder page in a browser
2. Select a file to transfer
3. Configure transfer settings
4. Start transmission
5. Position the receiving device to scan the QR codes

### Decoder (vdf-qr-decoder.html)

1. Open the decoder page in a browser
2. Select a video file containing QR codes
3. Click "Start Scan" to begin processing
4. Monitor progress through the visual indicators
5. Use "Stop Scan" to pause or "Reset Contents" to start over
6. Download the file when reconstruction is complete

## Technical Requirements

- Modern browser with HTML5 support
- JavaScript enabled
- For optimal performance:
  - Good lighting conditions for camera-based scanning
  - Steady video with clear QR codes
  - Moderate distance between QR codes and camera

## Dependencies

- HTML5-QRCode: QR code scanning library
- LTFountainCodes: Custom implementation of Luby Transform codes

## Documentation

For more detailed information about each component:

- [Encoder Documentation](encoder.md): Comprehensive guide to the QR code encoder including encoding strategies and configuration options
- [Decoder Documentation](decoder-documentation.md): Comprehensive guide to the QR code decoder including processing pipeline and recovery mechanisms
- API Reference (Coming Soon): Detailed API documentation for developers
