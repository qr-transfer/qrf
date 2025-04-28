# QRCoder Encoder Documentation

## Overview

The QRCoder Encoder is a sophisticated web-based tool for converting files into sequences of QR codes for transmission to receiving devices. It employs a hybrid encoding strategy combining systematic reliability with fountain code redundancy for resilient file transfers, even under challenging conditions.

This document provides comprehensive details about the encoder's architecture, implementation, and usage.

## Table of Contents

1. [Architecture](#architecture)
2. [Encoding Algorithm](#encoding-algorithm)
3. [Data Format](#data-format)
4. [User Interface](#user-interface)
5. [Performance Presets](#performance-presets)
6. [Advanced Features](#advanced-features)
7. [Best Practices](#best-practices)
8. [Technical Implementation](#technical-implementation)

## Architecture

The encoder follows a two-phase encoding strategy to maximize reliability and efficiency:

### Phase 1: Systematic Phase

In this initial phase, the encoder ensures that every chunk of the original file is transmitted at least once in its original form. This provides a baseline level of reliability, as any successfully scanned packet can immediately recover its corresponding chunk without dependencies.

- **Single-Chunk Mode**: When MaxDegree is set to 1, each packet contains exactly one chunk
- **Dual-Chunk Mode**: When MaxDegree is ≥2, the encoder combines chunks from opposite ends of the array to accelerate recovery

### Phase 2: Fountain (LT) Phase

After the systematic phase, the encoder generates additional redundant packets using Luby Transform (LT) coding. These fountain packets contain combinations of multiple chunks, providing resilience against packet loss.

- Uses Robust Soliton Distribution to determine the degree (number of chunks to combine)
- The degree distribution is carefully tuned to optimize decoder efficiency
- Each packet includes information about which chunks were combined

### Data Flow

```
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│  File Input   │──────▶│ Base64 Encoder│──────▶│  Chunk Splitter │
└───────────────┘       └───────────────┘       └───────────────┘
                                                        │
                                                        ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│   QR Display  │◀──────│  QR Generator │◀──────│ SystematicLT  │
└───────────────┘       └───────────────┘       └───────────────┘
```

## Encoding Algorithm

### File Processing

1. **Base64 Conversion**: The file is read and converted to Base64 format
2. **Chunk Division**: The Base64 data is split into equal-sized chunks
3. **Metadata Creation**: File information and encoding parameters are assembled into a metadata packet

### Systematic-LT Hybrid Encoding

1. **Systematic Phase**:
   - Each original chunk is sent at least once
   - When using dual-chunk mode, pairs of chunks are combined to reduce the number of systematic packets
   - Chunks paired strategically (first with last, second with second-to-last, etc.)

2. **Fountain Phase**:
   - Random seed is generated for each packet
   - Degree is selected using Robust Soliton Distribution
   - Chunks are selected based on the seed and degree
   - Selected chunks are combined and encoded into a packet

3. **Redundancy Control**:
   - User-configurable redundancy percentage
   - Automatic calculation of total packets needed
   - Additional safety packets for high-importance chunks

### Robust Soliton Distribution

The fountain phase uses the Robust Soliton Distribution to determine the degree (number of chunks to combine) for each packet:

1. **Parameters**:
   - c = 0.03 (distribution parameter)
   - δ = 0.5 (failure probability)

2. **Distribution Creation**:
   - Calculate R = c * ln(k/δ) * sqrt(k) where k is the number of chunks
   - Special probability for degree 1: (1/k) + (R/k)
   - For degrees 2 to k/R: add R/(i*k) to the standard distribution
   - For degree floor(k/R): add R * ln(R/δ) / k

3. **Degree Selection**:
   - Generate random value r in [0,1)
   - Select degree based on where r falls in the cumulative distribution

## Data Format

### Metadata Packet

```
M:3.0:filename.ext:image/jpeg:1024000:100:150:8:1:10:500:80:H:checksum:ltparams
```

**Format Fields**:
- `M`: Indicates a metadata frame
- `3.0`: Protocol version
- `filename.ext`: Original filename (URL encoded)
- `image/jpeg`: File MIME type (URL encoded)
- `1024000`: File size in bytes
- `100`: Number of chunks
- `150`: Maximum number of packets
- `8`: Maximum degree (for fountain coding)
- `1`: Density parameter (0=normal, 1=high density)
- `10`: FPS (frames per second)
- `500`: Chunk size in characters
- `80`: Redundancy percentage
- `H`: Error correction level
- `checksum`: File checksum
- `ltparams`: LT code parameters (c:delta)

### Data Packet

#### Single-Chunk Packet (Systematic Phase)
```
D:42:1234:10:100:1:57:chunkData
```

#### Multi-Chunk Packet (Dual Systematic or Fountain Phase)
```
D:42:1234:10:100:3:12:chunk1|34:chunk2|78:chunk3
```

**Format Fields**:
- `D`: Indicates a data frame
- `42`: Packet ID
- `1234`: Seed value
- `10`: Seed base
- `100`: Total number of chunks
- `3`: Degree (number of chunks combined)
- `12:chunk1|34:chunk2|78:chunk3`: Combined chunks in format `index:chunk|index:chunk|...`

#### Truncated Packet
For data that exceeds QR code capacity:
```
D:42:1234:10:100:3:truncatedData:t:originalLength
```

## User Interface

The encoder provides a comprehensive UI with the following components:

### Control Panel

- **File Selection**: Upload the file to be transmitted
- **Performance Presets**: Predefined settings for different use cases
- **QR Size**: Control the physical size of the QR code (200-800px)
- **Display Speed**: Frame rate control (1-30 FPS)
- **Chunk Size**: Data size per chunk (100-1000 characters)
- **Redundancy**: Extra packet percentage (20-200%)
- **Error Correction Level**: QR code error correction (L, M, Q, H)
- **Max Combined Chunks**: Maximum number of chunks to combine (1-4)
- **Countdown**: Delay before transmission starts (0-30s)
- **Display Options**: Fullscreen, alignment guides, transitions

### Display Area

- **QR Code Display**: Shows the current QR code
- **Frame Counter**: Shows current frame and total frames
- **Progress Bar**: Visual indication of transmission progress
- **File Information**: Details about the file being transmitted
- **Transmission Details**: Current packet information and statistics

### Fullscreen Mode

- **Maximized QR Code**: Expands the QR code for optimal scanning
- **Progress Indicator**: Shows transmission progress
- **Frame Counter**: Displays current frame information
- **Exit Button**: Returns to normal view

## Performance Presets

The encoder offers three optimized presets for different scenarios:

### Fast Transfer

Optimized for speed when scanning conditions are good:
- **FPS**: 12
- **Chunk Size**: 600 characters
- **Redundancy**: 50%
- **Max Degree**: 2
- **Error Correction**: M (15% recovery)

### Reliable

Balanced settings for general use:
- **FPS**: 10
- **Chunk Size**: 500 characters
- **Redundancy**: 80%
- **Max Degree**: 2
- **Error Correction**: H (30% recovery)

### Mobile Optimized

Designed for scanning with mobile devices:
- **FPS**: 8
- **Chunk Size**: 400 characters
- **Redundancy**: 100%
- **Max Degree**: 1
- **Error Correction**: H (30% recovery)
- **QR Size**: 600px

## Advanced Features

### High Density Mode

Enables larger QR codes with increased data capacity:
- **Standard Mode**: ~2500 characters per QR code
- **High Density Mode**: ~4000 characters per QR code
- Requires advanced scanner capabilities

### Alignment Guides

Visual guides to help position the scanner:
- Shows recommended scanning boundaries
- Assists in maintaining proper distance and alignment

### Smooth Transitions

Subtle transitions between QR codes to improve scanning reliability:
- Prevents motion blur during frame changes
- Improves scanner focus and recognition

### Adaptive Parameters

The encoder automatically recommends optimal settings based on file size:
- Small files (<50KB): Fast transfer settings
- Medium files (50KB-500KB): Reliable settings
- Large files (>500KB): Mobile optimized settings

## Best Practices

### Optimal Scanner Distance

- **Small Files**: 20-25cm
- **Medium Files**: 25-30cm
- **Large Files**: 25-30cm

### Redundancy Settings

- For 30% missed frames, use 60-70% redundancy
- For 50% missed frames, use 100-120% redundancy

### QR Code Error Correction

- **L (Low)**: 7% recovery, maximum data capacity
- **M (Medium)**: 15% recovery, good for clean environments
- **Q (Quartile)**: 25% recovery, good for average conditions
- **H (High)**: 30% recovery, best for difficult scanning conditions

### Max Degree Settings

- **1**: Maximum reliability, recommended for challenging conditions
- **2**: Good balance of reliability and efficiency
- **3-4**: Only for perfect scanning conditions

## Technical Implementation

### SystematicLTEncoder Class

The core of the encoding process is the SystematicLTEncoder class:

```javascript
class SystematicLTEncoder {
    constructor(originalChunks, seedBase = Date.now()) {
        // Original properties
        this.originalChunks = originalChunks;
        this.numChunks = originalChunks.length;
        this.seedBase = seedBase;
        this.packetCounter = 0;
        this.avgChunkSize = this.calculateAverageChunkSize();
        
        // Systematic LT properties
        this.systematicPhase = true;
        this.currentSystematicIndex = 0;
        this.maxSafeDegree = this.calculateMaxSafeDegree();
        
        // LT code parameters
        this.c = 0.03;
        this.delta = 0.5;
    }
    
    // Main packet generation
    generatePacket() {
        // Phase 1: Systematic Phase
        if (this.systematicPhase) {
            // Dual-chunk or single-chunk processing
            // ...
            
            // Check if systematic phase is complete
            if (this.currentSystematicIndex >= this.numChunks) {
                this.systematicPhase = false;
            }
        }
        
        // Phase 2: Fountain Phase
        return this.createLTPacket();
    }
    
    // Systematic packet creation (single chunk)
    createSystematicPacket(chunkIndex) {
        // Generate packet with exactly one chunk
        // ...
    }
    
    // Dual systematic packet creation
    createDualSystematicPacket(firstChunkIndex, secondChunkIndex) {
        // Generate packet with two chunks from opposite ends
        // ...
    }
    
    // LT packet creation
    createLTPacket() {
        // Generate seed
        // Select degree using Robust Soliton
        // Select chunks
        // Combine chunks
        // ...
    }
    
    // Calculate max safe degree based on QR capacity
    calculateMaxSafeDegree() {
        // Calculate based on QR capacity, chunk size, etc.
        // ...
    }
    
    // Calculate total packets needed
    calculateTotalPackets() {
        // Based on chunks and redundancy settings
        // ...
    }
    
    // Generate metadata packet
    generateMetadataPacket() {
        // Create metadata with file and encoding information
        // ...
    }
    
    // Robust Soliton degree selection
    getRobustSolitonDegree(rng) {
        // Implement the Robust Soliton Distribution
        // ...
    }
}
```

### QR Code Generation

The encoder uses the QRCode.js library to generate QR codes with the following parameters:

```javascript
const options = {
    errorCorrectionLevel: selectedLevel, // L, M, Q, or H
    margin: 1,
    width: qrSize,
    color: {
        dark: '#000000',
        light: '#FFFFFF'
    }
};

QRCode.toDataURL(packetData, options, callback);
```

### Display Handling

The encoder manages the display loop with a configurable frame rate:

```javascript
// Set up display interval
const interval = 1000 / parseInt(speedSlider.value);
displayInterval = setInterval(async () => {
    // Update frame index
    currentFrame = (currentFrame + 1) % totalFrames;
    
    // Generate new QR code
    await generateQRCodeForFrame(currentFrame);
    updateProgress();
    
    // Apply transitions
    if (enableTransitionsCheckbox.checked) {
        applyTransitionEffects();
    }
}, interval);
```

## Conclusion

The QRCoder Encoder provides a robust and flexible solution for file transmission via QR codes. Its hybrid approach combining systematic reliability with fountain code redundancy ensures successful transfers even in challenging scanning conditions. The comprehensive user interface and adaptive parameters make it accessible for a wide range of use cases, from small quick transfers to larger files requiring maximum reliability.