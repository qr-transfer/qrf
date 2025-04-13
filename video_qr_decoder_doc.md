# Video QR Code Decoder Documentation

## Overview

The Video QR Code Decoder is a high-performance web application that records QR code sequences from a Video QR Encoder and processes them offline to reconstruct the original file. By separating the capture and processing steps, it improves reliability and enables high-speed transfers with thousands of QR codes.

### Supported Video and File Formats

**Video Input Formats:**
- WebM (preferred for browser recording)
- MP4 / H.264
- MOV
- AVI
- Other formats supported by the browser's video element

**Output File Types:**
The decoder can reconstruct any file type that was encoded, including:
- Documents (PDF, DOC, DOCX, TXT, etc.)
- Images (JPG, PNG, GIF, SVG, etc.)
- Audio (MP3, WAV, FLAC, etc.)
- Video (MP4, AVI, MOV, etc.)
- Archives (ZIP, RAR, 7Z, etc.)
- Executables (EXE, MSI, etc.)
- Any other binary or text file

Since the system processes binary data, there are no file type limitations for reconstruction.

## Key Features

1. **Video Recording**: Captures QR code sequences directly from the camera.
2. **Multiple Input Sources**: Process videos from camera recording, local files, or remote URLs.
3. **Offline Processing**: Analyzes recorded video frame-by-frame without real-time constraints.
4. **Multi-QR Detection**: Support for grid layouts with 1-32 QR codes per frame for faster transfer.
5. **High-Density QR Processing**: Efficiently processes videos with thousands of QR codes.
6. **Preliminary Scan Analysis**: Quick analysis to determine frame rate and data density upfront.
7. **Adaptive Frame Rate**: Automatically adjusts processing based on QR code density.
8. **Smart Search Strategy**: Prioritizes areas where QR codes have been detected.
9. **File-Continuation Support**: Identifies the same file across sessions to focus on missing frames.
10. **Missing Frame Visualization**: Visual indicators showing which frames need to be captured.
11. **Set-Based Recovery**: Organizes recovered data into sets for better management.
12. **Partial Recovery**: Can save partially recovered files when possible.
13. **Detailed Statistics**: Provides metrics on frames processed, QR codes found, and chunks retrieved.

## User Interface

The interface is divided into several sections:

1. **Capture Tab**: Camera view and recording controls
2. **Process Tab**: Video processing, frame viewing, and statistics
3. **File Information Panel**: Details about the recovered file and set status
4. **Log Panel**: Technical logs and operation history

## How It Works

### Recording Phase

1. **Camera Access**: The application requests access to the device camera.
2. **Video Recording**: The QR code sequence is recorded as a WebM video file.
3. **Video Storage**: The recording can be downloaded or processed immediately.

### Processing Phase

1. **Video Loading**: A recorded video file is loaded into the application.
2. **Metadata Search**: The beginning of the video is intensively scanned to locate the metadata frame.
3. **Preliminary Analysis**: A quick scan determines:
   - Optimal frame rate for processing
   - Detection of grid layout (1x1, 2x2, 4x4, etc.)
   - Total expected frames
   - Efficient processing strategy

4. **Smart Frame Extraction**: Individual frames are extracted from the video using adaptive strategies:
   - High-density scanning of regions where QR codes were found
   - Adaptive frame rate based on QR code density in the video
   - Dynamic batch processing for optimal performance
   
5. **Enhanced QR Code Detection**: Each frame is analyzed using optimized parameters:
   - Grid-based detection for multiple QR codes per frame (up to 32)
   - Support for high-capacity QR codes (up to version 40)
   - Both normal and inverted QR code detection
   - Optimized canvas operations with `willReadFrequently` flag
   - Image enhancement for better QR detection in sub-optimal conditions
6. **Parallel Set Processing**: The system leverages the set-based architecture:
   - Each set contains independent metadata (set ID, chunk count, file ID)
   - Processing can occur in parallel for different sets
   - Sets can be processed across different sessions
   - No practical limit to file size (supports gigabyte-sized files)

7. **Data Integrity Verification**: Each QR code contains integrity checks:
   - Position identifiers (set index, chunk index)
   - Metadata references (file ID, total chunk count)
   - JSON structure validation
   - This ensures that chunks are correctly placed during reconstruction

8. **Data Organization**: Decoded data is organized based on set and chunk indices.

9. **File History Tracking**: The system identifies if the same file has been seen before:
   - Stores file metadata and frame status across sessions
   - Focuses on acquiring missing frames only
   - Provides visual indicators for frames still needed

10. **Real-time Progress Visualization**: A grid display shows:
   - Received chunks in green
   - Missing chunks in red (with animated highlighting)
   - For large files, progress is shown in grouped indicators
   
11. **File Reconstruction**: When all chunks are received (or enough for partial recovery), the original file is reconstructed.

### Data Types and Frame Structure

The decoder processes several types of QR frames, each carefully designed for integrity and parallel processing capabilities:

1. **Metadata Frame**: Contains file information and transfer parameters
   ```json
   {
     "type": "metadata",
     "file_name": "example.jpg",
     "file_size": 1000000,
     "total_sets": 10,
     "chunks_per_set": 50,
     "total_chunks": 200,
     "timestamp": 1649289600000,
     "qr_grid_size": 4,
     "file_id": "example.jpg_1000000" 
   }
   ```
   - The `file_id` allows recognition of the same file across sessions
   - Grid size information enables auto-detection of multi-QR layouts

2. **Set Header Frame**: Marks the beginning of a set of chunks
   ```json
   {
     "type": "set_header",
     "set_index": 1,
     "total_sets": 10,
     "chunks_in_set": 50,
     "file_id": "example.jpg_1000000"
   }
   ```
   - Each set header contains a complete reference to the file
   - This enables independent processing of sets regardless of order
   - Allows selective transmission of specific sets

3. **Chunk Frame**: Contains actual data
   ```json
   {
     "type": "chunk",
     "set_index": 1,
     "chunk_index": 1,
     "chunks_in_set": 50,
     "data": "base64_encoded_chunk"
   }
   ```
   - Each chunk knows its exact position within its set
   - Self-contained information allows for out-of-order processing
   - Enables correct reassembly even with mixed chunk order

4. **End Frame**: Signals the end of transmission
   ```json
   {
     "type": "end",
     "file_name": "example.jpg",
     "file_size": 1000000,
     "selected_sets": [1,2,3,4,5,6,7,8,9,10],
     "timestamp": 1649289600000
   }
   ```
   - Contains a summary of transmitted sets
   - Provides verification data for integrity checks

The combination of these structured frame types ensures:
- Complete data integrity through position tracking
- Support for unlimited file sizes through set-based chunking
- Ability to process sets in parallel or across multiple sessions
- Efficient targeting of missing chunks for retransmission

## Usage Instructions

### Capturing QR Code Sequence

1. **Start Camera**: Click "Start Camera" to access your device camera.
2. **Start Recording**: Position the camera to view the QR code sequence and click "Start Recording".
3. **Record Complete Cycle**: Ensure you record at least one complete cycle of all QR codes.
4. **Stop Recording**: Click "Stop Recording" when finished.
5. **Download Recording**: Optionally save the recording for later processing.

### Processing Recorded Video

1. **Load Video**: You have three options:
   - Click "Load Local Video" and select a recording file from your device.
   - Click "Load URL Video" to enter a URL for a remote video file.
   - Use a video previously recorded within the application.
   
2. **For URL Videos**:
   - Enter the complete URL to the video file in the input field.
   - Click "Fetch" to download and prepare the video.
   - The application will validate and load the video from the remote source.
   
3. **Quick Analysis**: Click "Quick Analysis" to perform a preliminary scan of the video.
   - This will determine frame rate, grid layout, and total expected frames
   - Based on this information, the optimal processing strategy will be selected

4. **Start Processing**: Click "Start Processing" to begin analyzing the video.
   - If the file has been partially processed before, missing frames will be highlighted
   - Processing will focus on frames that haven't been successfully captured yet

5. **Monitor Progress**: Watch the progress bar and statistics as frames are processed.
   - The grid visualization shows which frames have been captured
   - Missing frames are highlighted in red with animation

6. **Pause/Resume**: You can pause processing if needed and resume later.

7. **Save File**: When enough chunks are recovered, click "Save File" to reconstruct and download the original file.

## Performance Considerations

- **Video Quality**: Higher resolution recordings generally improve QR code detection.
- **Processing Time**: The decoder now utilizes adaptive processing strategies:
  - Quick analysis scan to determine optimal processing parameters
  - Initial intensive scanning of the video beginning to find metadata (0-10 seconds)
  - Dynamic frame rate selection (2-25 fps) based on detected QR code density
  - Focus on regions where QR codes were previously found
  - Detection of grid layouts with multiple QR codes per frame
- **Multi-QR Processing**: Support for various grid layouts:
  - 1x1: Single QR code per frame (standard mode)
  - 2x1, 2x2, 4x2, 4x4, and 8x4: Multiple QR codes per frame
  - Automatic detection of grid layout from metadata
  - Increased transfer rates of up to 32x with 8x4 grid layouts
- **Scaling to Virtually Unlimited Sizes**: The decoder can handle:
  - Set-based architecture supporting millions of QR codes
  - Each set can be processed independently and in parallel
  - No practical limit to file size with set-based chunking
  - High framerate QR transmission (up to 30fps)
  - Maximum QR code size (500px) with high-capacity codes
  - Multiple QR codes per frame in various grid configurations
- **Memory Management**: For extremely large files, the decoder:
  - Processes video in adaptive batch sizes (10-50 frames per batch)
  - Uses optimized canvas operations for better performance
  - Implements memory-efficient grid visualization for thousands of frames
- **Partial Recovery**: Files can often be recovered even with some missing chunks, depending on the file type.
- **Progress Visualization**: Real-time visual feedback shows:
  - Chunks received and missing
  - Processing progress with percentage completion
  - Estimated time remaining based on detected QR density

## Technical Notes

- **QR Detection Engine**: Uses the jsQR library with optimized parameters:
  - `inversionAttempts: 'attemptBoth'` - Handles both normal and inverted QR codes
  - `canOverwriteImage: true` - Improves performance when processing large numbers of frames
  - `maxModuleCount: 177` - Supports high-capacity QR codes (up to version 40)
- **Canvas Optimizations**:
  - Uses the `willReadFrequently` attribute for better performance with frequent pixel access
  - Dynamically adjusts canvas size based on video dimensions
  - Efficiently reuses canvas contexts to minimize memory consumption
- **Adaptive Processing**:
  - `requestAnimationFrame` for smoother, more efficient frame processing
  - Dynamic batch sizes that adjust based on detected QR code density
  - Smart time-based seeking to prioritize areas with detected QR codes
- **DOM Efficiency**:
  - Uses DocumentFragment for batch DOM updates
  - Implements grouped visualization for videos with thousands of frames
  - Throttles UI updates to maintain performance
- **Metadata Detection**:
  - Enhanced parsing of metadata formats with fallback calculations
  - Multiple unit conversion methods for determining total chunks
  - Support for both modern (JSON-typed) and legacy (marker-based) QR formats
- **Client-Side Only**: The application works entirely in the browser with no server dependencies
- **Partial File Recovery**: Missing chunks will result in corruption in the final file, but many file formats (especially media files) can still be partially usable

## Troubleshooting

- **First Frames Not Detected**: 
  - Use the "Quick Analysis" feature to determine optimal processing parameters
  - The decoder performs intensive scanning of the first 10 seconds (100ms resolution)
  - If metadata is still missed, try recording a few seconds before the actual QR sequence starts
  - Ensure the video begins with a clear, steady view of the first QR code (metadata frame)

- **QR Detection Issues with High-Density Videos**:
  - For 30fps QR code sequences, ensure adequate lighting and camera focus
  - Hold the recording device steady, ideally using a tripod or stabilizer
  - Position the camera to capture the entire red frame shown during countdown
  - Record in higher resolution (1080p or better) for improved QR recognition
  - For grid layouts (multiple QRs per frame), ensure all QR codes are clearly visible

- **Grid Layout Considerations**:
  - Higher density grid layouts (4x4, 8x4) require better camera positioning
  - The red frame during countdown shows the exact area that needs to be captured
  - If experiencing issues with high-density grids, try using a lower density (2x2 instead of 4x4)
  - Ensure the recording device is held steady and perpendicular to the screen

- **Handling Thousands of QR Codes**:
  - The decoder now efficiently supports up to 50,000 frames
  - Use the enhanced grid visualization to identify missing chunks
  - For very large files, consider processing the video in segments if memory issues occur
  - Look for patterns in missing chunks - they often occur in clusters

- **Browser Compatibility**: 
  - This application works best in Chrome, Edge, or Firefox
  - Safari has limited performance with large frame counts
  - For processing extremely large videos, use a desktop browser rather than mobile

- **Performance Optimization**:
  - Close other tabs and applications when processing large videos
  - If processing slows down, try the "Pause" button, wait a few seconds, then "Resume"
  - For very large files, consider segmenting the recording into smaller videos
  
- **Corrupted Files**: 
  - If the recovered file is unusable, check the visual grid to identify missing chunks
  - Note which sections are missing and use the encoder's set selection feature to retransmit only those sets
  - For critical files, consider using a lower framerate (10-15 fps) during encoding for better reliability

- **URL Video Issues**: 
  - Ensure the URL points directly to a video file (typically ending in .mp4, .webm, etc.)
  - Some servers may block direct access due to CORS policies
  - For protected videos, download them locally first and then use the "Load Local Video" option

- **Missing Chunks Pattern Analysis**:
  - If chunks are missing in a regular pattern, it may indicate synchronization issues between encoder and camera
  - Try adjusting the encoder's frame rate to match your camera's capabilities (often 15fps works best)
  - For large missing sections, use the set visualization to identify which sets need retransmission