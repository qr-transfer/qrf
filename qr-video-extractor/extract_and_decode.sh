#!/bin/bash

# extract_and_decode.sh - Stream video QR extraction directly to decoder
# Usage: ./extract_and_decode.sh <video_file> [start_time] [additional_args...]

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Help function
show_help() {
    echo -e "${BLUE}🎬 QR Video Extract & Decode Pipeline${NC}"
    echo ""
    echo "Usage: $0 <video_file> [options]"
    echo ""
    echo "Options:"
    echo "  --start-time TIME    Start processing from time (MM:SS or HH:MM:SS)"
    echo "  --skip N             Process every Nth frame (default: 15)"
    echo "  --threads N          Number of processing threads"
    echo "  --max-frames N       Process only first N frames"
    echo "  --timeout N          Timeout in seconds"
    echo ""
    echo "Examples:"
    echo "  $0 video.mp4"
    echo "  $0 video.mp4 --start-time 1:30"
    echo "  $0 video.mp4 --start-time 1:30 --skip 10 --threads 8"
    echo ""
    echo "Features:"
    echo "  🌊 Real-time streaming: Files generated as soon as ready"
    echo "  ⏰ Start from any time: Skip to specific video position"
    echo "  🚀 Parallel processing: Extraction and decoding run together"
    echo "  💾 Progress saving: Continuous progress updates"
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

# Build extractor command
EXTRACTOR_CMD="./target/release/qr-video-extractor extract \"$VIDEO_FILE\" --stream --only-text"

# Process additional arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --start-time)
            EXTRACTOR_CMD="$EXTRACTOR_CMD --start-time \"$2\""
            shift 2
            ;;
        --skip)
            EXTRACTOR_CMD="$EXTRACTOR_CMD --skip $2"
            shift 2
            ;;
        --threads)
            EXTRACTOR_CMD="$EXTRACTOR_CMD --threads $2"
            shift 2
            ;;
        --max-frames)
            EXTRACTOR_CMD="$EXTRACTOR_CMD --max-frames $2"
            shift 2
            ;;
        --timeout)
            EXTRACTOR_CMD="$EXTRACTOR_CMD --timeout $2"
            shift 2
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

# Create output directory
mkdir -p decoded_files

echo -e "${BLUE}🚀 Starting streaming extraction and decoding pipeline...${NC}"
echo -e "${YELLOW}📹 Video: $VIDEO_FILE${NC}"
echo -e "${YELLOW}💻 Extractor: $EXTRACTOR_CMD${NC}"
echo -e "${YELLOW}🔄 Decoder: Processing stdin with real-time file generation${NC}"
echo ""

# Set up signal handling for cleanup
cleanup() {
    echo -e "\n${YELLOW}🛑 Cleaning up processes...${NC}"
    # Kill any remaining background processes
    jobs -p | xargs -r kill 2>/dev/null || true
    exit 130
}
trap cleanup INT TERM

# Start the pipeline with proper error handling
{
    # Run extractor and pipe to decoder
    eval "$EXTRACTOR_CMD" 2>&1 | ./target/release/decode_qr_files dummy --stdin 2>&1
} || {
    EXIT_CODE=$?
    echo -e "\n${RED}❌ Pipeline failed with exit code $EXIT_CODE${NC}" >&2
    exit $EXIT_CODE
}

echo -e "\n${GREEN}✅ Pipeline completed successfully!${NC}"
echo -e "${GREEN}📁 Check './decoded_files' directory for extracted files${NC}"

# Show summary of extracted files
if [[ -d "decoded_files" ]] && [[ -n "$(ls -A decoded_files 2>/dev/null)" ]]; then
    echo -e "\n${BLUE}📊 Extracted files:${NC}"
    ls -la decoded_files/ | grep -v "^total" | while read -r line; do
        if [[ "$line" =~ \.streaming\.json$ ]]; then
            echo -e "  ${YELLOW}📝 $line (in progress)${NC}"
        elif [[ "$line" =~ \.partial\.json$ ]]; then
            echo -e "  ${YELLOW}📊 $line (partial)${NC}"
        else
            echo -e "  ${GREEN}📄 $line${NC}"
        fi
    done
else
    echo -e "\n${YELLOW}⚠️ No files were extracted. Check the video contains QR codes.${NC}"
fi