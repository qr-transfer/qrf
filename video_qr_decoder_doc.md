# Video QR Code Decoder Documentation

## Overview

The Video QR Code Decoder is a web application that records QR code sequences from a Video QR Encoder and processes them offline to reconstruct the original file. By separating the capture and processing steps, it improves reliability and enables higher speed transfers.

## Key Features

1. **Video Recording**: Captures QR code sequences directly from the camera.
2. **Multiple Input Sources**: Process videos from camera recording, local files, or remote URLs.
3. **Offline Processing**: Analyzes recorded video frame-by-frame without real-time constraints.
4. **Progress Tracking**: Visual indicators for processing progress and chunk recovery.
5. **Set-Based Recovery**: Organizes recovered data into sets for better management.
6. **Partial Recovery**: Can save partially recovered files when possible.
7. **Detailed Statistics**: Provides metrics on frames processed, QR codes found, and chunks retrieved.

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
2. **Frame Extraction**: Individual frames are extracted from the video at regular intervals.
3. **QR Code Detection**: Each frame is analyzed to detect and decode QR codes.
4. **Data Organization**: Decoded data is organized based on set and chunk indices.
5. **File Reconstruction**: When all chunks are received (or enough for partial recovery), the original file is reconstructed.

### Data Types

The decoder processes several types of QR frames:

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
   
3. **Start Processing**: Click "Start Processing" to begin analyzing the video.
4. **Monitor Progress**: Watch the progress bar and statistics as frames are processed.
5. **Pause/Resume**: You can pause processing if needed and resume later.
6. **Save File**: When enough chunks are recovered, click "Save File" to reconstruct and download the original file.

## Performance Considerations

- **Video Quality**: Higher resolution recordings generally improve QR code detection.
- **Processing Time**: Expect processing to take roughly 1/3 to 1/2 of the video duration.
- **Memory Usage**: For extremely large files, the decoder processes the video in chunks to manage memory.
- **Partial Recovery**: Files can often be recovered even with some missing chunks, depending on the file type.
- **Optimal Frame Sampling**: The processor samples frames at approximately 10fps regardless of the actual video frame rate.

## Technical Notes

- The decoder uses jsQR library for QR code detection.
- Canvas is used for frame extraction and analysis.
- IndexedDB storage is used internally for large files (over browser memory limits).
- The application works entirely client-side with no server dependencies.
- Missing chunks will result in corruption in the final file, but many file formats (especially media files) can still be partially usable.

## Troubleshooting

- **Low QR Detection Rate**: Try recording with better lighting, less camera movement, and proper focus.
- **Missing Chunks**: Record multiple passes of the QR sequence to increase chances of capturing all chunks.
- **Browser Compatibility**: This application works best in Chrome, Edge, or Firefox. Safari may have limited recording capabilities.
- **Performance Issues**: Close other tabs and applications when processing large videos.
- **Corrupted Files**: If the recovered file is unusable, try re-recording with a lower QR display speed.
- **URL Video Issues**: 
  - Ensure the URL points directly to a video file (typically ending in .mp4, .webm, etc.)
  - Some servers may block direct access due to CORS policies
  - For protected videos, download them locally first and then use the "Load Local Video" option
- **Partial Set Recovery**: If only some sets were transmitted properly, note which ones are missing and use the encoder's set selection feature to retransmit only those sets.