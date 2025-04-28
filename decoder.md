# QRCoder Decoder Documentation

## Overview

The QRCoder Decoder is a sophisticated web-based tool for reconstructing files from sequences of QR codes. It supports both real-time camera capture and video file processing, employing fountain coding for resilient file recovery even when some QR codes are missed or corrupted.

This document provides a comprehensive guide to the decoder's architecture, components, and usage.

## Table of Contents

1. [Features](#features)
2. [Architecture](#architecture)
3. [Core Components](#core-components)
4. [Fountain Coding Implementation](#fountain-coding-implementation)
5. [User Interface](#user-interface)
6. [Usage Guide](#usage-guide)
7. [Advanced Settings](#advanced-settings)
8. [Performance Considerations](#performance-considerations)
9. [Troubleshooting](#troubleshooting)
10. [API Reference](#api-reference)

## Features

- **Video File Processing**: Process pre-recorded videos containing QR code sequences
- **Frame-by-Frame Analysis**: Extract and analyze individual frames for QR codes
- **Parallel Processing**: Multi-threaded design for optimal performance
- **Fountain Code Recovery**: Error-resistant file reconstruction using LT codes
- **Visualization**: Real-time visual feedback of chunk recovery progress
- **Configurable Settings**: Adjustable processing parameters
- **Debug Logging**: Comprehensive logging for troubleshooting
- **Automatic Download**: Files are downloaded automatically upon completion
- **Progressive Recovery**: Files can be recovered before all chunks are received

## Architecture

The decoder follows a modular architecture with distinct components that work together to process QR codes and reconstruct files.

### High-Level Architecture

```
┌─────────────────┐       ┌───────────────┐        ┌───────────────────┐
│  Video Processor │──────▶│ QR Processor  │───────▶│  Packet Processor │
└─────────────────┘       └───────────────┘        └───────────────────┘
                                                            │
┌─────────────────┐                                         ▼
│       UI        │◀────────────────────────────────┐ ┌─────────────────┐
└─────────────────┘                                 │ │Fountain Decoder │
         ▲                                          │ └─────────────────┘
         │                                          │         │
         └──────────────────────────────────────────┘         │
                                                              ▼
                                                    ┌─────────────────┐
                                                    │    File Data    │
                                                    └─────────────────┘
```

### Data Flow

1. Video frames are extracted by the Video Processor
2. Frames are analyzed by the QR Processor to detect QR codes
3. Detected QR codes are parsed by the Packet Processor
4. Decoded packets are processed by the Fountain Decoder
5. The UI is updated with progress information
6. The reconstructed file is assembled and downloaded

## Core Components

The decoder consists of several key components, each responsible for a specific part of the processing pipeline.

### EnhancedVideoProcessor

Responsible for processing video files frame by frame.

- **Features**:
  - Frame extraction and delivery
  - Progress tracking
  - Performance monitoring
  - Adaptive frame rate adjustment

- **Key Methods**:
  - `initialize(videoFile)`: Prepares the processor with the selected video
  - `startProcessing()`: Begins extracting frames from the video
  - `processCurrentFrame()`: Processes a single frame from the video
  - `stopProcessing()`: Halts the processing operation

### EnhancedQRProcessor

Analyzes video frames to detect and decode QR codes.

- **Features**:
  - QR code detection
  - Duplicate detection
  - Position estimation
  - Multi-format support

- **Key Methods**:
  - `initialize()`: Sets up the QR code detector
  - `processFrame(imageData, frameIndex)`: Analyzes a frame for QR codes
  - `estimateBounds(decodedResult)`: Determines QR code position in the frame
  - `checkDuplicate(qrData, frameIndex)`: Avoids processing duplicate QR codes

### PacketProcessor

Parses raw QR code data into structured packets.

- **Features**:
  - Protocol parsing
  - Metadata extraction
  - Data packet handling
  - Error detection

- **Key Methods**:
  - `processQRData(qrData, frameIndex)`: Processes raw QR code data
  - `processMetadataPacket(metaString, frameIndex)`: Handles metadata frames
  - `processDataPacket(dataString, frameIndex)`: Handles data frames
  - `createPRNG(seed)`: Creates a pseudo-random number generator for packet processing

### EnhancedFountainDecoder

Implements Luby Transform (LT) coding for resilient file reconstruction.

- **Features**:
  - Progressive chunk recovery
  - Packet propagation
  - Base64 decoding
  - File reconstruction

- **Key Methods**:
  - `initialize(metadata)`: Sets up the decoder with file metadata
  - `addPacket(packet)`: Processes a data packet
  - `propagateAndDecode()`: Attempts to recover more chunks using received packets
  - `finalizeFile()`: Reconstructs the complete file from recovered chunks

### Logger

Provides comprehensive logging for debugging and monitoring.

- **Features**:
  - Multiple log levels
  - Filtering capabilities
  - Timestamp recording
  - Entry limiting

- **Key Methods**:
  - `debug/info/warn/error(message)`: Logs messages at different severity levels
  - `setFilter(filter)`: Filters log entries by level
  - `clear()`: Clears all log entries

### UI

Manages the user interface and visual feedback.

- **Features**:
  - Chunk visualization
  - Progress indicators
  - QR code highlighting
  - Error messaging

- **Key Methods**:
  - `initializeChunkGrid(chunksCount)`: Sets up the chunk visualization grid
  - `updateProgress(progress, currentFrame, totalFrames, remainingTime)`: Updates progress indicators
  - `drawQRHighlight(x, y, width, height)`: Highlights detected QR codes
  - `markChunkAsRecovered(chunkIndex)`: Updates the visualization when chunks are recovered

## Fountain Coding Implementation

The decoder uses Luby Transform (LT) coding to enable robust file recovery even when some QR codes are missed.

### Key Concepts

1. **Chunks**: The file is divided into equal-sized chunks
2. **Packets**: Encoded data packets that may contain one or more chunks
3. **Systematic Packets**: Packets containing a single chunk (degree 1)
4. **Fountain Packets**: Packets containing multiple chunks XORed together (degree > 1)
5. **Degree**: The number of chunks combined in a packet
6. **Seed**: Value used to determine which chunks are combined in a packet

### Recovery Process

1. **Direct Recovery**: Systematic packets (degree 1) provide direct chunk recovery
2. **Elimination**: When a packet has only one unknown chunk, it can be recovered by XORing with known chunks
3. **Propagation**: Newly recovered chunks are used to solve other packets
4. **Recursion**: The process continues until all chunks are recovered or no more progress can be made

### Example

```
// Given:
// - Chunk 1: 10101010
// - Chunk 2: 11001100
// - Chunk 3: 00110011

// Packet A (Systematic): Chunk 1 = 10101010
// Packet B (Fountain): Chunks 1+2 = 10101010 XOR 11001100 = 01100110
// Packet C (Fountain): Chunks 2+3 = 11001100 XOR 00110011 = 11111111

// Recovery:
// 1. From Packet A, recover Chunk 1 directly: 10101010
// 2. Using Chunk 1 and Packet B, recover Chunk 2:
//    01100110 XOR 10101010 = 11001100
// 3. Using Chunk 2 and Packet C, recover Chunk 3:
//    11111111 XOR 11001100 = 00110011
```

## User Interface

The decoder provides a comprehensive interface with several panels for monitoring and control.

### Control Panel

Located at the top of the page, provides access to the main controls:

- **Video Input**: Upload pre-recorded videos containing QR codes
- **Start Scan**: Begin the scanning process
- **Stop Scan**: Pause the scanning process
- **Reset Contents**: Clear all data and start fresh
- **Advanced Settings**: Access configuration options

### Video Display

Shows the current video being processed with QR code highlighting overlay.

### Chunk Recovery Progress

Visual representation of recovered chunks:

- **Pending (Gray)**: Chunks not yet recovered
- **Received (Green)**: Recovered chunks
- **Blinking (Yellow to Green)**: Recently recovered chunks

### File Information

Displays details about the file being reconstructed:

- **File Name**: Original name of the file
- **File Type**: MIME type of the file
- **File Size**: Size in bytes/KB/MB
- **Chunks**: Total number of chunks
- **Protocol Version**: Version of the QRCoder protocol

### Debug Log

Comprehensive logging window with filtering options:

- **All**: Show all log entries
- **Debug**: Low-level diagnostic information
- **Info**: General operational information
- **Warnings**: Potential issues that don't prevent operation
- **Errors**: Critical issues affecting operation

## Usage Guide

### Basic Usage

1. **Prepare a Video**:
   - Record a video of a QRCoder transmission
   - Ensure good lighting and stable framing

2. **Load the Video**:
   - Click the "Choose File" button in the control panel
   - Select the recorded video file
   - Wait for the video to load

3. **Start Scanning**:
   - Click the "Start Scan" button
   - The video will begin playing and processing

4. **Monitor Progress**:
   - Watch the chunk recovery grid for progress
   - Check the file information panel for details
   - View the debug log for detailed information

5. **Download the File**:
   - When reconstruction is complete, the file will download automatically
   - Alternatively, click the "Download Recovered File" button that appears

### Best Practices

- **Video Quality**: Use high-quality video recordings with good lighting
- **Frame Rate**: Higher frame rates improve recognition reliability
- **Stability**: Keep the camera steady during recording
- **Distance**: Maintain a consistent distance from the QR codes
- **Coverage**: Ensure the QR codes are fully visible in the frame
- **Duration**: Record the entire sequence from start to finish

## Advanced Settings

The decoder provides several configurable parameters accessible via the "Advanced Settings" button.

### QR Processing Workers

The number of parallel workers for QR code processing.

- **Default**: 4
- **Range**: 1-16
- **Effect**: Higher values increase throughput but consume more resources

### Packet Processing Workers

The number of parallel workers for packet processing.

- **Default**: 2
- **Range**: 1-8
- **Effect**: Higher values increase throughput but consume more resources

### Frame Processing Interval

The time interval (in milliseconds) between processed frames.

- **Default**: 20ms
- **Range**: 0-1000ms
- **Effect**: Lower values process more frames but consume more resources

### QR Detection Confidence

The confidence threshold for QR code detection.

- **Default**: 0.5
- **Range**: 0-1
- **Effect**: Higher values reduce false positives but may miss valid codes

## Performance Considerations

### Hardware Requirements

- **CPU**: Multi-core processor recommended
- **Memory**: Minimum 4GB RAM
- **GPU**: Not required but can improve performance
- **Camera**: HD camera with good low-light performance (for live scanning)

### Browser Compatibility

- **Chrome/Edge**: Full support
- **Firefox**: Full support
- **Safari**: Partial support (WebCodecs API limitations)
- **Mobile Browsers**: Limited support based on device capabilities

### Optimization Techniques

1. **Reduce Resolution**: Processing lower resolution videos reduces CPU load
2. **Increase Frame Interval**: Processing fewer frames per second reduces CPU load
3. **Reduce Worker Count**: Fewer workers consume less memory
4. **Close Other Tabs**: Dedicate browser resources to the decoder

## Troubleshooting

### Common Issues

#### No QR Codes Detected

- **Possible Causes**:
  - Poor video quality
  - QR codes too small in frame
  - Poor lighting conditions
  - Incorrect QR code format

- **Solutions**:
  - Improve lighting conditions
  - Record closer to the QR codes
  - Ensure QR codes are properly generated
  - Check that the video contains valid QR codes

#### Incomplete File Recovery

- **Possible Causes**:
  - Too many missed frames
  - Corrupted QR codes
  - Incomplete recording
  - Incompatible QR code format

- **Solutions**:
  - Record the entire sequence
  - Ensure stable recording conditions
  - Verify the QR codes are properly generated
  - Try processing at a lower frame interval

#### High CPU Usage

- **Possible Causes**:
  - Too many worker threads
  - Frame interval too low
  - High-resolution video
  - Background browser tasks

- **Solutions**:
  - Reduce the number of workers
  - Increase the frame processing interval
  - Close other resource-intensive tabs
  - Process a lower resolution video

### Debug Tools

The decoder includes built-in debugging tools for advanced troubleshooting:

- **Log Filtering**: View specific types of log entries
- **Debug Console**: Access the `window.debugTools` object in the browser console
- **State Analysis**: Use `window.debugTools.analyzeDecoderState()` to view internal state
- **Data Export**: Use `window.debugTools.exportRecoveredData()` to examine recovered data

## API Reference

### QRFileDecoder

The main class that orchestrates the entire decoding process.

```javascript
class QRFileDecoder {
  constructor()
  handleVideoInput(event)
  startProcessing()
  stopProcessing()
  resetContents()
  handleVideoFrame(imageData, timestamp, frameIndex)
  handleQRCodeResult(result)
  handlePacketResult(result)
  handleMetadataPacket(metadata)
  handleDataPacket(packet)
  handleFileComplete(fileData)
  downloadFile()
  saveSettings()
}
```

### EnhancedVideoProcessor

```javascript
class EnhancedVideoProcessor {
  constructor(options)
  static isWebCodecsSupported()
  async initialize(videoFile)
  startProcessing()
  processCurrentFrame()
  stopProcessing()
}
```

### EnhancedQRProcessor

```javascript
class EnhancedQRProcessor {
  constructor(options)
  async initialize()
  processFrame(imageData, frameIndex)
  checkDuplicate(qrData, frameIndex)
  estimateBounds(decodedResult)
  dispose()
}
```

### PacketProcessor

```javascript
class PacketProcessor {
  constructor()
  processQRData(qrData, frameIndex)
  processMetadataPacket(metaString, frameIndex)
  processDataPacket(dataString, frameIndex)
  createPRNG(seed)
  selectChunksLT(rng, degree, numChunks)
}
```

### EnhancedFountainDecoder

```javascript
class EnhancedFountainDecoder {
  constructor()
  initialize(metadata)
  setCompleteCallback(callback)
  addPacket(packet)
  propagateAndDecode()
  storeSourceChunk(index, data)
  getNewlyRecoveredChunks()
  getRecoveryProgress()
  finalizeFile()
}
```

## Conclusion

The QRCoder Decoder represents a sophisticated implementation of fountain coding principles applied to QR code-based file transmission. By leveraging parallel processing, error-resistant coding, and progressive reconstruction, it provides a robust solution for offline file transfers between devices.

The modular architecture allows for future enhancements and customization, while the comprehensive user interface provides clear visibility into the recovery process.