# Video QR Code Encoder Documentation

## Overview

The Video QR Code Encoder is a web application that converts files into a sequence of QR codes displayed at adjustable speeds. It's designed to enable faster and more reliable file transfers through QR codes by allowing users to record the sequence and process it later.

## Key Features

1. **Adjustable Display Speed**: Control the frames per second (1-30 fps) to optimize for different recording devices.
2. **Configurable QR Code Size**: Adjust the size of QR codes (150-500px) for better visibility and scanning.
3. **Customizable Chunk Size**: Set the size of data chunks (100-1000 bytes) to balance between QR code complexity and number of frames.
4. **Set-Based Organization**: Data is organized into sets of chunks for better management and recovery.
5. **Set Selection**: Choose specific sets to transmit, enabling targeted retransmission of missing data.
6. **Multiple File Support**: Queue and transfer multiple files in sequence.
7. **Countdown Timer**: Configurable countdown before starting QR display.
8. **Variable Frame Timing**: Critical frames (metadata, headers) stay visible longer for better capture.
9. **Progress Tracking**: Visual indicators show current frame and overall progress.
10. **Debug View**: Detailed logging for troubleshooting.

## User Interface

The interface is divided into several sections:

1. **Control Panel**: File selection, display settings, and control buttons
2. **File Queue Panel**: Manage multiple files for transfer
3. **Set Selection Panel**: Choose which sets to transmit
4. **Info Panel**: File details and transfer statistics
5. **QR Display**: The current QR code and progress indicators
6. **Debug View**: Technical logs and information (hidden by default)

## How It Works

### File Preparation

1. **File Loading**: The application reads the selected file as an ArrayBuffer.
2. **Base64 Conversion**: The binary data is converted to a Base64 string for QR encoding.
3. **Chunking**: The Base64 data is split into configurable-sized chunks (default 500 bytes).
4. **Set Organization**: Chunks are grouped into sets (default 50 chunks per set) for better management.

### Frame Types

The encoder generates several types of QR codes:

1. **Metadata Frame**: Contains file information and transfer parameters
   ```json
   {
     "type": "metadata",
     "file_name": "example.jpg",
     "file_size": 1000000,
     "total_sets": 10,
     "chunks_per_set": 50,
     "total_chunks": 200,
     "timestamp": 1649289600000
   }
   ```

2. **Set Header Frame**: Marks the beginning of a set of chunks
   ```json
   {
     "type": "set_header",
     "set_index": 1,
     "total_sets": 10,
     "chunks_in_set": 50
   }
   ```

3. **Chunk Frame**: Contains actual data
   ```json
   {
     "type": "chunk",
     "set_index": 1,
     "chunk_index": 1,
     "total_chunks": 200,
     "data": "base64_encoded_chunk"
   }
   ```

4. **End Frame**: Signals the end of transmission
   ```json
   {
     "type": "end",
     "file_name": "example.jpg",
     "file_size": 1000000,
     "total_chunks": 200,
     "timestamp": 1649289600000
   }
   ```

### Display Process

1. The QR codes are displayed in sequence, starting with the metadata frame.
2. Set headers are displayed before their associated chunks.
3. Chunks are displayed in order, with configurable frame rate.
4. The end frame is displayed last.
5. The sequence loops continuously until stopped.

## Usage Instructions

1. **File Selection and Queue Management**:
   - Click "Browse" to select one or more files to encode.
   - Use the file queue controls to prioritize files if needed.
   - Files are processed one at a time in queue order.

2. **Configure Settings**:
   - Adjust QR display speed based on your recording device's capabilities
   - Set QR code size according to your display and recording setup
   - Modify chunk size if needed (smaller chunks create more frames but easier to decode)
   - Set countdown timer duration (1-10 seconds)

3. **Generate QR Codes**: 
   - Click the "Generate QR Codes" button to prepare the QR sequence.
   - The set selection panel will appear showing all sets for the current file.

4. **Set Selection** (optional):
   - By default, all sets are selected for transmission.
   - Click individual sets to toggle their selection status.
   - Use "Select All" or "Deselect All" for quick selection.
   - This feature is especially useful for retransmitting only missing sets.

5. **Start Display**: 
   - Click "Start Display" to begin the countdown timer.
   - After the countdown completes, the QR code sequence will start automatically.
   - Metadata and set header frames will display longer for better capture.

6. **Record the Sequence**: 
   - Use the Video QR Decoder's recording feature to capture the sequence.
   - Position your recording device during the countdown period.

7. **Stop Display**: 
   - Click "Stop Display" when finished recording.

## Performance Considerations

- **Speed vs. Reliability**: Higher frame rates transfer data faster but may reduce reliability. Start with 10fps and adjust based on results.
- **Size vs. Capacity**: Larger QR codes can hold more data but require better camera focus. 300px is a good starting point.
- **Chunk Size Trade-offs**: Smaller chunks (100-300 bytes) are more reliable but create more frames. Larger chunks (700-1000 bytes) reduce frame count but may be harder to decode.
- **Optimal Recording Distance**: Position the recording device at a distance where the QR code fills a significant portion of the frame without being cut off.

## Technical Notes

- The encoder uses the QRCode.js library to generate codes.
- QR codes use correction level L for maximum data capacity.
- JSON is used for structured data to enable better error handling and recovery.
- The application is designed to work entirely client-side with no server dependencies.
- Maximum theoretical transfer rate at 30fps with 500-byte chunks: ~15KB/s (or ~900KB/minute).