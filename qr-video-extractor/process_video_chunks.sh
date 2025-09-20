#!/bin/bash

# Video processing orchestrator for memory-efficient extraction
VIDEO_FILE="/Users/qr-transfer/Downloads/20250915_135509.mp4"
CHUNK_SIZE=18000  # 10 minutes at 30fps
TOTAL_FRAMES=66654  # 37 minutes at 30fps
OUTPUT_DIR="./chunked_results"

# Create output directory
mkdir -p "$OUTPUT_DIR"

echo "🚀 Starting chunked video processing..."
echo "📺 Video: 37 minutes, ~66,654 frames"
echo "📦 Chunk size: $CHUNK_SIZE frames (~10 minutes)"
echo "💾 Memory-efficient streaming extraction"

# Calculate number of chunks needed
CHUNKS=$(( (TOTAL_FRAMES + CHUNK_SIZE - 1) / CHUNK_SIZE ))
echo "🔢 Processing $CHUNKS chunks..."

# Process each chunk
for ((i=0; i<CHUNKS; i++)); do
    START_FRAME=$((i * CHUNK_SIZE))
    CHUNK_NUM=$((i + 1))

    echo ""
    echo "📋 Processing chunk $CHUNK_NUM/$CHUNKS (frames $START_FRAME - $((START_FRAME + CHUNK_SIZE - 1)))"

    # Extract chunk with streaming
    ./target/release/qr-video-extractor extract "$VIDEO_FILE" \
        --output "$OUTPUT_DIR/chunk_${CHUNK_NUM}.json" \
        --sequenced \
        --skip 1 \
        --threads 1 \
        --start-frame $START_FRAME \
        --max-frames $CHUNK_SIZE

    if [ $? -eq 0 ]; then
        echo "✅ Chunk $CHUNK_NUM completed successfully"

        # Get QR count for this chunk
        QR_COUNT=$(jq '.total_unique // .sequenced_qr_codes | length' "$OUTPUT_DIR/chunk_${CHUNK_NUM}.json")
        echo "   📊 Found $QR_COUNT QR codes in chunk $CHUNK_NUM"
    else
        echo "❌ Chunk $CHUNK_NUM failed"
    fi
done

echo ""
echo "🎯 All chunks processed! Now merging..."

# Merge all chunks into complete dataset
echo "🔗 Merging chunked results..."
node - << 'EOF'
const fs = require('fs');
const path = require('path');

// Read all chunk files
const chunkDir = './chunked_results';
const chunkFiles = fs.readdirSync(chunkDir)
    .filter(f => f.startsWith('chunk_') && f.endsWith('.json'))
    .sort((a, b) => {
        const numA = parseInt(a.match(/chunk_(\d+)\.json/)[1]);
        const numB = parseInt(b.match(/chunk_(\d+)\.json/)[1]);
        return numA - numB;
    });

console.log(`📁 Found ${chunkFiles.length} chunk files`);

let allQrCodes = [];
let totalProcessingTime = 0;
let videoInfo = null;

// Merge all chunks in order
for (const chunkFile of chunkFiles) {
    const chunkPath = path.join(chunkDir, chunkFile);
    const chunkData = JSON.parse(fs.readFileSync(chunkPath, 'utf8'));

    if (chunkData.sequenced_qr_codes) {
        allQrCodes.push(...chunkData.sequenced_qr_codes);
        totalProcessingTime += chunkData.processing_time_ms || 0;

        if (!videoInfo && chunkData.video_info) {
            videoInfo = chunkData.video_info;
        }

        console.log(`   ✅ Merged ${chunkData.sequenced_qr_codes.length} QR codes from ${chunkFile}`);
    }
}

// Remove duplicates while preserving order
const seen = new Set();
const uniqueQrCodes = allQrCodes.filter(qr => {
    if (seen.has(qr.data)) {
        return false;
    }
    seen.add(qr.data);
    return true;
});

// Create final merged dataset
const mergedData = {
    sequenced_qr_codes: uniqueQrCodes,
    total_unique: uniqueQrCodes.length,
    video_info: videoInfo,
    processing_time_ms: totalProcessingTime,
    chunks_processed: chunkFiles.length,
    source: 'chunked_extraction'
};

// Save merged result
fs.writeFileSync('./qr_codes_complete_merged.json', JSON.stringify(mergedData, null, 2));

console.log(`\n🎉 Merge complete!`);
console.log(`   📊 Total unique QR codes: ${uniqueQrCodes.length}`);
console.log(`   ⏱️  Total processing time: ${Math.round(totalProcessingTime/1000)}s`);
console.log(`   📁 Saved to: qr_codes_complete_merged.json`);
EOF

echo ""
echo "🎉 Complete! Ready for fountain decoding:"
echo "   node decode_qr_files.js qr_codes_complete_merged.json"