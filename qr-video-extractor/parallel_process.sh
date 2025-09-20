#!/bin/bash

# parallel_process.sh - Complete parallel QR video processing workflow
# Usage: ./parallel_process.sh <video_file> [options]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Help function
show_help() {
    echo -e "${BLUE}🚀 Parallel QR Video Processing Workflow${NC}"
    echo ""
    echo "This script implements a complete parallel processing pipeline:"
    echo "1. 📊 Analyze video structure to detect QR file boundaries"
    echo "2. 🔪 Split video into ~100MB chunks preserving file boundaries"
    echo "3. ⚡ Process chunks in parallel to extract QR codes (JSONL)"
    echo "4. 🔗 Combine or process JSONL files to reconstruct original files"
    echo ""
    echo "Usage: $0 <video_file> [options]"
    echo ""
    echo "Options:"
    echo "  --chunk-size N       Target chunk size in MB (default: 100)"
    echo "  --threads N          Number of parallel threads (default: auto)"
    echo "  --skip N             Frame skip for QR extraction (default: 1 = all frames)"
    echo "  --start-time TIME    Start from time (MM:SS or HH:MM:SS)"
    echo "  --keep-chunks        Keep intermediate video chunks"
    echo "  --combine-jsonl      Combine JSONL files before decoding"
    echo "  --analyze-only       Only perform analysis, no processing"
    echo "  --split-only         Only split video, no QR processing"
    echo ""
    echo "Examples:"
    echo "  $0 large_video.mp4"
    echo "  $0 large_video.mp4 --chunk-size 50 --threads 8"
    echo "  $0 large_video.mp4 --start-time 2:30 --combine-jsonl"
    echo "  $0 large_video.mp4 --analyze-only"
    echo ""
    echo "Workflow Benefits:"
    echo "  🚀 Parallel processing: N chunks processed simultaneously"
    echo "  🎯 Boundary preservation: Never splits QR file sequences"
    echo "  💾 Memory efficient: Each chunk processed independently"
    echo "  ⏰ Start from any time: Skip to specific video position"
    echo "  🔄 Resumable: Can restart from any step"
    exit 0
}

# Check for help
if [[ "$1" == "-h" ]] || [[ "$1" == "--help" ]] || [[ $# -eq 0 ]]; then
    show_help
fi

# Check if video file exists
VIDEO_FILE="$1"
if [[ ! -f "$VIDEO_FILE" ]]; then
    echo -e "${RED}❌ Error: Video file '$VIDEO_FILE' not found${NC}" >&2
    exit 1
fi

shift # Remove video file from arguments

# Default parameters
CHUNK_SIZE=100
THREADS=""
SKIP=1
START_TIME=""
KEEP_CHUNKS=false
COMBINE_JSONL=false
ANALYZE_ONLY=false
SPLIT_ONLY=false

# Process additional arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --chunk-size)
            CHUNK_SIZE="$2"
            shift 2
            ;;
        --threads)
            THREADS="$2"
            shift 2
            ;;
        --skip)
            SKIP="$2"
            shift 2
            ;;
        --start-time)
            START_TIME="$2"
            shift 2
            ;;
        --keep-chunks)
            KEEP_CHUNKS=true
            shift
            ;;
        --combine-jsonl)
            COMBINE_JSONL=true
            shift
            ;;
        --analyze-only)
            ANALYZE_ONLY=true
            shift
            ;;
        --split-only)
            SPLIT_ONLY=true
            shift
            ;;
        *)
            echo -e "${YELLOW}⚠️ Warning: Unknown option $1${NC}" >&2
            shift
            ;;
    esac
done

# Check if binaries exist
if [[ ! -f "./target/release/qr-video-extractor" ]]; then
    echo -e "${RED}❌ Error: qr-video-extractor binary not found. Run 'cargo build --release' first.${NC}" >&2
    exit 1
fi

if [[ ! -f "./target/release/decode_qr_files" ]]; then
    echo -e "${RED}❌ Error: decode_qr_files binary not found. Run 'cargo build --release --bin decode_qr_files' first.${NC}" >&2
    exit 1
fi

# Get video base name for output directory
VIDEO_BASE=$(basename "$VIDEO_FILE" .mp4)
OUTPUT_DIR="parallel_output_${VIDEO_BASE}_$(date +%Y%m%d_%H%M%S)"

echo -e "${BLUE}🚀 Starting Parallel QR Processing Workflow${NC}"
echo -e "${YELLOW}📹 Video: $VIDEO_FILE${NC}"
echo -e "${YELLOW}📂 Output: $OUTPUT_DIR${NC}"
echo -e "${YELLOW}🎯 Chunk size: ${CHUNK_SIZE} MB${NC}"
echo -e "${YELLOW}🔄 Frame skip: $SKIP (1=all frames)${NC}"
[[ -n "$START_TIME" ]] && echo -e "${YELLOW}⏰ Start time: $START_TIME${NC}"
[[ -n "$THREADS" ]] && echo -e "${YELLOW}🔧 Threads: $THREADS${NC}"
echo ""

# Set up signal handling for cleanup
cleanup() {
    echo -e "\n${YELLOW}🛑 Cleaning up processes...${NC}"
    jobs -p | xargs -r kill 2>/dev/null || true
    exit 130
}
trap cleanup INT TERM

# Step 1: Analyze video structure (always needed)
echo -e "${CYAN}📊 Step 1: Analyzing video structure...${NC}"
ANALYSIS_FILE="$OUTPUT_DIR/analysis.json"
mkdir -p "$OUTPUT_DIR"

ANALYZE_CMD="./target/release/qr-video-extractor analyze \"$VIDEO_FILE\" --output \"$ANALYSIS_FILE\""
echo -e "${YELLOW}💻 Running: $ANALYZE_CMD${NC}"

if ! eval "$ANALYZE_CMD"; then
    echo -e "${RED}❌ Video analysis failed${NC}" >&2
    exit 1
fi

echo -e "${GREEN}✅ Analysis complete: $ANALYSIS_FILE${NC}"

if [[ "$ANALYZE_ONLY" == true ]]; then
    echo -e "${GREEN}🎉 Analysis-only mode complete!${NC}"
    echo -e "${GREEN}📋 Check analysis report: $ANALYSIS_FILE${NC}"
    exit 0
fi

# Step 2: Split video or use full parallel processing
if [[ "$SPLIT_ONLY" == true ]]; then
    echo -e "${CYAN}🔪 Step 2: Splitting video (split-only mode)...${NC}"
    SPLIT_CMD="./target/release/qr-video-extractor split \"$VIDEO_FILE\" --output \"$OUTPUT_DIR/chunks\" --chunk-size-mb $CHUNK_SIZE --analysis \"$ANALYSIS_FILE\""

    echo -e "${YELLOW}💻 Running: $SPLIT_CMD${NC}"
    if ! eval "$SPLIT_CMD"; then
        echo -e "${RED}❌ Video splitting failed${NC}" >&2
        exit 1
    fi

    echo -e "${GREEN}🎉 Split-only mode complete!${NC}"
    echo -e "${GREEN}📁 Check chunks: $OUTPUT_DIR/chunks/${NC}"
    exit 0
fi

# Step 3: Full parallel processing workflow
echo -e "${CYAN}⚡ Step 2: Running full parallel processing workflow...${NC}"

# Build the split-process command
SPLIT_PROCESS_CMD="./target/release/qr-video-extractor split-process \"$VIDEO_FILE\" --output \"$OUTPUT_DIR\" --chunk-size-mb $CHUNK_SIZE --skip $SKIP"

[[ -n "$THREADS" ]] && SPLIT_PROCESS_CMD="$SPLIT_PROCESS_CMD --threads $THREADS"
[[ -n "$START_TIME" ]] && SPLIT_PROCESS_CMD="$SPLIT_PROCESS_CMD --start-time \"$START_TIME\""
[[ "$KEEP_CHUNKS" == true ]] && SPLIT_PROCESS_CMD="$SPLIT_PROCESS_CMD --keep-chunks"
[[ "$COMBINE_JSONL" == true ]] && SPLIT_PROCESS_CMD="$SPLIT_PROCESS_CMD --combine-jsonl"

echo -e "${YELLOW}💻 Running: $SPLIT_PROCESS_CMD${NC}"

if ! eval "$SPLIT_PROCESS_CMD"; then
    echo -e "${RED}❌ Parallel processing failed${NC}" >&2
    exit 1
fi

echo -e "\n${GREEN}🎉 Parallel processing workflow complete!${NC}"
echo -e "${GREEN}📁 Results directory: $OUTPUT_DIR${NC}"

# Show summary of results
echo -e "\n${BLUE}📊 Results Summary:${NC}"

# Show extracted files
if [[ -d "$OUTPUT_DIR/decoded_files" ]]; then
    EXTRACTED_FILES=$(find "$OUTPUT_DIR/decoded_files" -type f ! -name "*.json" | wc -l)
    PARTIAL_FILES=$(find "$OUTPUT_DIR/decoded_files" -name "*.partial.json" | wc -l)
    STREAMING_FILES=$(find "$OUTPUT_DIR/decoded_files" -name "*.streaming.json" | wc -l)

    echo -e "${GREEN}   ✅ Files extracted: $EXTRACTED_FILES${NC}"
    [[ $PARTIAL_FILES -gt 0 ]] && echo -e "${YELLOW}   📝 Partial files: $PARTIAL_FILES${NC}"
    [[ $STREAMING_FILES -gt 0 ]] && echo -e "${CYAN}   🔄 In progress: $STREAMING_FILES${NC}"
fi

# Show JSONL files
if [[ -d "$OUTPUT_DIR/jsonl" ]]; then
    JSONL_FILES=$(find "$OUTPUT_DIR/jsonl" -name "*.jsonl" | wc -l)
    echo -e "${BLUE}   📄 JSONL files: $JSONL_FILES${NC}"
fi

# Show chunks if kept
if [[ "$KEEP_CHUNKS" == true ]] && [[ -d "$OUTPUT_DIR/chunks" ]]; then
    CHUNK_FILES=$(find "$OUTPUT_DIR/chunks" -name "*.mp4" | wc -l)
    TOTAL_CHUNK_SIZE=$(du -sm "$OUTPUT_DIR/chunks" 2>/dev/null | cut -f1)
    echo -e "${PURPLE}   🎬 Video chunks: $CHUNK_FILES (${TOTAL_CHUNK_SIZE} MB total)${NC}"
fi

echo -e "\n${GREEN}🎯 Processing complete! Check '$OUTPUT_DIR' for all results.${NC}"