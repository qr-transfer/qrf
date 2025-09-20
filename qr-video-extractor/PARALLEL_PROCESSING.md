# 🚀 Parallel QR Video Processing System

## 🎯 Overview
Complete parallel processing system for large QR-encoded videos with intelligent boundary detection and chunked processing.

## 🏗️ System Architecture

### **1. Intelligent Video Analysis**
```bash
# Analyze video structure to detect QR file boundaries
./target/release/qr-video-extractor analyze video.mp4 --output analysis.json
```

**Features:**
- **Binary search approach** - Efficiently finds file start markers (M: packets)
- **File boundary detection** - Identifies where encoded files begin/end
- **Smart sampling** - Configurable sample intervals for speed vs accuracy
- **Analysis report** - JSON output with complete structure map

### **2. Boundary-Preserving Video Splitting**
```bash
# Split video preserving QR file boundaries
./target/release/qr-video-extractor split video.mp4 --chunk-size-mb 100 --analysis analysis.json
```

**Features:**
- **~100MB chunks** - Configurable target size with actual file size calculation
- **Numbered output** - `001_video.mp4`, `002_video.mp4`, etc.
- **Boundary preservation** - Never splits in middle of QR file sequences
- **FFmpeg integration** - Fast, lossless video segmentation

### **3. Parallel Chunk Processing**
```bash
# Process all chunks in parallel
./target/release/qr-video-extractor split-process video.mp4 --threads 8 --combine-jsonl
```

**Features:**
- **N-thread processing** - Default: same number as chunks created
- **JSONL generation** - Each chunk produces ordered QR codes
- **Real-time progress** - Live updates from all processing threads
- **Memory efficient** - Each chunk processed independently

### **4. Intelligent Result Combination**
```bash
# Two modes for result processing:

# Mode 1: Combined JSONL processing (recommended)
--combine-jsonl  # Merges all JSONL files, then decodes

# Mode 2: Individual chunk processing
# Processes each chunk's JSONL separately
```

**Features:**
- **Frame order preservation** - Maintains temporal sequence across chunks
- **Metadata reconstruction** - Combines statistics from all chunks
- **File integrity** - Same FNV-1a checksum verification
- **Progress tracking** - Continuous monitoring during reconstruction

## 🔧 Complete Workflow Commands

### **All-in-One Parallel Processing**
```bash
# Complete workflow with boundary detection
./parallel_process.sh large_video.mp4

# With custom parameters
./parallel_process.sh large_video.mp4 --chunk-size 50 --threads 16 --start-time 2:30

# Analysis only (for planning)
./parallel_process.sh large_video.mp4 --analyze-only

# Split only (prepare for manual processing)
./parallel_process.sh large_video.mp4 --split-only --keep-chunks
```

### **Manual Step-by-Step Processing**
```bash
# Step 1: Analyze video structure
./target/release/qr-video-extractor analyze video.mp4 --output analysis.json

# Step 2: Split preserving boundaries
./target/release/qr-video-extractor split video.mp4 --analysis analysis.json --chunk-size-mb 100

# Step 3: Parallel processing
./target/release/qr-video-extractor split-process video.mp4 --threads 8 --combine-jsonl
```

## 📊 Performance Benefits

### **Parallel Processing Gains**
- **N-times faster** - Process N chunks simultaneously
- **Memory efficiency** - Each chunk uses independent memory space
- **Scalability** - Performance scales with available CPU cores
- **Resumability** - Can restart failed chunks individually

### **Boundary Detection Intelligence**
- **No broken files** - QR sequences never split across chunks
- **Optimal splits** - Finds natural boundaries between encoded files
- **Quicksort-like efficiency** - Binary search for fast boundary detection
- **Configurable precision** - Balance speed vs accuracy with sample intervals

### **File Size Optimization**
- **Precise calculations** - Uses actual video file size for splitting
- **Flexible targets** - Configurable chunk sizes (50MB, 100MB, 200MB)
- **Size verification** - Reports actual chunk sizes after splitting
- **Storage efficiency** - Optional intermediate file cleanup

## 🎬 Chunk Naming Convention

### **Video Chunks**
```
chunks/001_video_name.mp4  # First chunk
chunks/002_video_name.mp4  # Second chunk
chunks/N_video_name.mp4    # Final chunk
```

### **JSONL Files**
```
jsonl/001_video_name.jsonl  # QR codes from first chunk
jsonl/002_video_name.jsonl  # QR codes from second chunk
jsonl/N_video_name.jsonl    # QR codes from final chunk
```

### **Decoded Results**
```
# Combined mode:
decoded_files/             # All files reconstructed from combined JSONL

# Individual mode:
decoded_files/chunk_001/   # Files from first chunk
decoded_files/chunk_002/   # Files from second chunk
decoded_files/chunk_N/     # Files from final chunk
```

## ⚡ Advanced Usage Examples

### **High-Performance Processing**
```bash
# Maximum performance: small chunks, many threads
./parallel_process.sh video.mp4 --chunk-size 25 --threads 32 --skip 5

# Balance: medium chunks, optimal threads
./parallel_process.sh video.mp4 --chunk-size 100 --threads 16 --skip 15

# Precision: large chunks, fewer threads, every frame
./parallel_process.sh video.mp4 --chunk-size 200 --threads 4 --skip 1
```

### **Targeted Processing**
```bash
# Process specific time range
./parallel_process.sh video.mp4 --start-time 5:30 --chunk-size 50

# Analysis and planning phase
./parallel_process.sh video.mp4 --analyze-only

# Controlled splitting for manual processing
./parallel_process.sh video.mp4 --split-only --keep-chunks --chunk-size 75
```

### **Production Workflows**
```bash
# Large video processing with progress preservation
./parallel_process.sh huge_video.mp4 --chunk-size 100 --threads 16 --combine-jsonl --keep-chunks

# Memory-constrained environments
./parallel_process.sh video.mp4 --chunk-size 50 --threads 4 --skip 20

# High-accuracy reconstruction
./parallel_process.sh video.mp4 --chunk-size 200 --threads 8 --skip 1 --combine-jsonl
```

## 📋 Output Structure

```
parallel_output_video_20250919_123456/
├── analysis.json                    # Video structure analysis
├── chunks/                         # Video chunks (if --keep-chunks)
│   ├── 001_video.mp4
│   ├── 002_video.mp4
│   └── ...
├── jsonl/                          # QR extraction results
│   ├── 001_video.jsonl
│   ├── 002_video.jsonl
│   └── ...
├── combined_qr_codes.jsonl         # Combined JSONL (if --combine-jsonl)
├── decoded_files/                  # Final reconstructed files
│   ├── file1.jpg
│   ├── file2.pdf
│   └── ...
└── splitting_report.json           # Processing statistics
```

## 🎯 Key Innovations

### **Boundary-Aware Splitting**
- Scans video using configurable sample intervals
- Uses binary search to locate QR file start markers (M: packets)
- Calculates optimal split points that respect file boundaries
- Prevents corruption from mid-file splits

### **Parallel Architecture**
- Splits large videos into manageable chunks (~100MB each)
- Processes each chunk independently in parallel threads
- Maintains temporal order through careful frame numbering
- Enables processing of arbitrarily large videos

### **Intelligent Combination**
- **Combined mode**: Merges all JSONL files maintaining frame order
- **Individual mode**: Processes each chunk separately for incremental results
- **Progress preservation**: Continuous saving during long operations
- **Error recovery**: Graceful handling of chunk processing failures

This system transforms large video QR processing from a single-threaded, memory-intensive operation into a scalable, parallel workflow that can handle videos of any size efficiently.