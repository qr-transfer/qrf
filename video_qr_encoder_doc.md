# Video QR Code Encoder Documentation

## Overview

The Video QR Code Encoder is a web application that converts files into a sequence of QR codes displayed at adjustable speeds. It's designed to enable faster and more reliable file transfers through QR codes by allowing users to record the sequence and process it later.

### Supported File Types

The system can encode and transfer any file type, including:
- Documents (PDF, DOC, DOCX, TXT, etc.)
- Images (JPG, PNG, GIF, SVG, etc.)
- Audio (MP3, WAV, FLAC, etc.)
- Video (MP4, AVI, MOV, etc.)
- Archives (ZIP, RAR, 7Z, etc.)
- Executables (EXE, MSI, etc.)
- Any other binary or text file

Since the system treats files as binary data, there are no file type limitations. Files are chunked and encoded in Base64 format, allowing for complete integrity during transfer.

## Key Features

1. **Adjustable Display Speed**: Control the frames per second (1-30 fps) to optimize for different recording devices.
2. **Configurable QR Code Size**: Adjust the size of QR codes (150-500px) for better visibility and scanning.
3. **Multiple QR Code Grid Layouts**: Support for 1x1, 2x1, 2x2, 4x2, 4x4, and 8x4 grid layouts for higher data throughput.
4. **Customizable Chunk Size**: Set the size of data chunks (100-1000 bytes) to balance between QR code complexity and number of frames.
5. **Set-Based Organization**: Data is organized into sets of chunks for better management and recovery.
6. **Set Selection**: Choose specific sets to transmit, enabling targeted retransmission of missing data.
7. **Multiple File Support**: Queue and transfer multiple files in sequence.
8. **Countdown Timer**: Configurable countdown before starting QR display.
9. **Variable Frame Timing**: Critical frames (metadata, headers) stay visible longer for better capture.
10. **Progress Tracking**: Real-time indicators show elapsed time, remaining time, and percentage complete.
11. **Dual Display Modes**: Use either normal view or fullscreen presentation mode without requiring restart.
12. **Time Estimation**: Precise calculation of expected transfer time based on file size and parameters.
13. **Adaptive Layout**: QR display automatically sized based on grid layout to match the actual QR size.
14. **Debug View**: Detailed logging for troubleshooting.

## User Interface

The interface is divided into several sections:

1. **QR Display Area**: Shows the current QR code(s) with frame counter and progress
2. **Control Panel**: File selection, display settings, and control buttons
3. **File Queue Panel**: Manage multiple files for transfer
4. **Set Selection Panel**: Choose which sets to transmit
5. **Info Panel**: File details and transfer statistics
6. **Time Estimate Panel**: Shows transfer time, data rate, and progress
7. **Presentation Mode**: Full-screen display with additional information
8. **Debug View**: Technical logs and information (hidden by default)

## How It Works

### File Preparation

1. **File Loading**: The application reads the selected file as an ArrayBuffer.
2. **Base64 Conversion**: The binary data is converted to a Base64 string for QR encoding.
3. **Chunking**: The Base64 data is split into configurable-sized chunks (default 500 bytes).
4. **Set Organization**: Chunks are grouped into sets (default 50 chunks per set) for better management.

The set-based architecture is a critical feature that enables:

1. **Scalability to Unlimited File Sizes**: By breaking files into independently processable sets, there's no practical upper limit to file size. Files of many gigabytes can be transferred in stages.

2. **Parallel Processing**: Each set contains all the metadata needed for independent processing:
   - Set identifier
   - Total set count
   - Chunk count within the set
   - File identifier
   
3. **Selective Retransmission**: If specific sets are missing or corrupted, only those sets need to be retransmitted.

4. **Memory Efficiency**: The decoder can process one set at a time, keeping memory requirements constant regardless of file size.

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
4. For grid layouts (2x1, 2x2, etc.), multiple QR codes are displayed simultaneously in each frame.
5. Metadata and set headers are displayed longer for better capture reliability.
6. Real-time time estimation and progress tracking are shown during display.
7. The end frame is displayed last.
8. The sequence loops continuously until stopped.

## Usage Instructions

1. **File Selection and Queue Management**:
   - Click "Browse" to select one or more files to encode.
   - Use the file queue controls to prioritize files if needed.
   - Files are processed one at a time in queue order.

2. **Configure Settings**:
   - Adjust QR display speed based on your recording device's capabilities
   - Set QR code size according to your display and recording setup
   - Select grid layout (1x1, 2x1, 2x2, 4x2, 4x4, or 8x4) to optimize transfer speed
   - Modify chunk size if needed (smaller chunks create more frames but easier to decode)
   - Set countdown timer duration (1-10 seconds)
   - Note the estimated transfer time that updates based on your settings

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
   - The QR display will be automatically sized based on your grid layout selection.
   - A positioning frame with red border will guide camera placement during countdown.
   - After the countdown completes, the QR code sequence will start automatically.
   - Metadata and set header frames will display longer for better capture.
   - Real-time progress indicators show elapsed time, remaining time, and completion percentage.

6. **Record the Sequence**: 
   - Use the Video QR Decoder's recording feature to capture the sequence.
   - Position your recording device during the countdown period.

7. **Display Modes**:
   - By default, QR codes display in normal view within the webpage.
   - Click "Enter Full Screen Mode" to switch to presentation mode with larger display.
   - Toggle between modes at any time without disrupting the QR sequence.
   - Both modes show time estimates and progress information.

8. **Stop Display**: 
   - Click "Stop Display" when finished recording.

## Performance Considerations

- **Speed vs. Reliability**: Higher frame rates transfer data faster but may reduce reliability. Start with 10fps and adjust based on results.
- **Size vs. Capacity**: Larger QR codes can hold more data but require better camera focus. 300px is a good starting point.
- **Grid Layout Optimization**: Use grid layouts to drastically increase transfer speed:
  - A 2x2 grid (4 QR codes per frame) effectively quadruples your transfer rate
  - An 8x4 grid (32 QR codes per frame) can transfer data up to 32 times faster
  - Higher density grids require better camera positioning and focus
- **Chunk Size Trade-offs**: Smaller chunks (100-300 bytes) are more reliable but create more frames. Larger chunks (700-1000 bytes) reduce frame count but may be harder to decode.
- **Optimal Recording Distance**: Position the recording device at a distance where the red positioning frame is fully visible during countdown.
- **Display Mode Selection**: 
  - Use normal view for quick transfers or when recording with mobile devices
  - Use presentation mode for maximum visibility and when using a tripod for recording

## Technical Notes

- The encoder uses the QRCode.js library to generate codes.
- QR codes use correction level L for maximum data capacity.
- JSON is used for structured data to enable better error handling and recovery.
- Grid layouts significantly increase data throughput:
  - Single QR (1x1): ~15KB/s at 30fps
  - 2x2 grid: ~60KB/s at 30fps
  - 4x4 grid: ~240KB/s at 30fps
  - 8x4 grid: ~480KB/s at 30fps
- Real-time transfer rate calculation provides accurate time estimates.
- Dynamic QR sizing ensures optimal display regardless of grid layout.
- The application is designed to work entirely client-side with no server dependencies.
- Smooth transition between display modes preserves current QR frame and position.