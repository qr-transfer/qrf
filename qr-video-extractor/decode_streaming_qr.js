#!/usr/bin/env node

const fs = require('fs');
const path = require('path');
const readline = require('readline');

/**
 * JSONL QR Code Decoder
 *
 * Supports both formats:
 * 1. Standard JSON: {"sequenced_qr_codes": [...]}
 * 2. Streaming JSONL: One JSON object per line
 *
 * JSONL Format:
 * {"type":"header","video_info":{"duration_seconds":2221.32,"fps":30.01,"width":1440,"height":1440}}
 * {"type":"qr_code","frame_number":0,"timestamp_ms":0.0,"data":"M:3.0:A.part32-51.7z:..."}
 * {"type":"qr_code","frame_number":40,"timestamp_ms":1341.07,"data":"D:1:..."}
 * {"type":"footer","summary":{"frames_processed":18000,"qr_codes_found":15303,"processing_time_ms":1850050}}
 */

// Reuse existing fountain decoder classes from decode_qr_files.js
class FountainDecoder {
    constructor() {
        this.initialized = false;
        this.metaData = null;
        this.totalChunks = 0;
        this.sourceChunks = {};
        this.recoveredChunkCount = 0;
        this.codedPackets = [];
    }

    initialize(metadata) {
        this.metaData = metadata;
        this.totalChunks = metadata.chunksCount;
        this.sourceChunks = {};
        this.recoveredChunkCount = 0;
        this.codedPackets = [];
        this.initialized = true;

        console.log(`📄 Initialized decoder for ${metadata.fileName} (${metadata.chunksCount} chunks, ${metadata.fileSize} bytes)`);
        this.printProgress();
    }

    addPacket(packet) {
        if (!this.initialized) return false;

        if (packet.systematicDataChunks && packet.systematicDataChunks.length > 0) {
            // Process systematic chunks directly
            packet.systematicDataChunks.forEach(chunk => {
                if (!this.sourceChunks[chunk.chunkIndex]) {
                    this.sourceChunks[chunk.chunkIndex] = Buffer.from(chunk.chunkData);
                    this.recoveredChunkCount++;
                    this.printProgress();
                }
            });
        } else if (packet.xorData) {
            // Store fountain packet for later processing
            this.codedPackets.push(packet);
            this.processCoded();
        }

        return true;
    }

    processCoded() {
        let progress = true;
        while (progress) {
            progress = false;
            for (let i = this.codedPackets.length - 1; i >= 0; i--) {
                const packet = this.codedPackets[i];
                const missing = packet.sourceChunks.filter(idx => !this.sourceChunks[idx]);

                if (missing.length === 1) {
                    // Can recover exactly one chunk
                    const missingIdx = missing[0];
                    let result = Buffer.from(packet.xorData);

                    // XOR with known chunks
                    for (const idx of packet.sourceChunks) {
                        if (idx !== missingIdx && this.sourceChunks[idx]) {
                            const chunk = this.sourceChunks[idx];
                            for (let j = 0; j < Math.min(result.length, chunk.length); j++) {
                                result[j] ^= chunk[j];
                            }
                        }
                    }

                    this.sourceChunks[missingIdx] = result;
                    this.recoveredChunkCount++;
                    console.log(`🔧 Fountain recovered chunk ${missingIdx}`);

                    this.codedPackets.splice(i, 1);
                    progress = true;
                    this.printProgress();
                } else if (missing.length === 0) {
                    this.codedPackets.splice(i, 1);
                }
            }
        }
    }

    isComplete() {
        return this.recoveredChunkCount >= this.totalChunks;
    }

    printProgress() {
        const percentage = Math.round((this.recoveredChunkCount / this.totalChunks) * 100);
        const progressBar = '🟩'.repeat(Math.floor(percentage / 2)) + '⬜'.repeat(50 - Math.floor(percentage / 2));
        process.stdout.write(`\r🔄 Progress: ${this.recoveredChunkCount}/${this.totalChunks} (${percentage}%) [${progressBar}]`);
    }

    finalize(outputDir) {
        if (!this.isComplete()) {
            console.log(`\n❌ File incomplete: ${this.recoveredChunkCount}/${this.totalChunks} chunks`);
            return null;
        }

        console.log('\n🔧 Reconstructing file from chunks...');

        // Verify all chunks exist
        for (let i = 0; i < this.totalChunks; i++) {
            if (!this.sourceChunks[i]) {
                console.log(`❌ Missing chunk ${i} during reconstruction`);
                return null;
            }
        }

        // Combine chunks in order
        let fileData = Buffer.alloc(this.metaData.fileSize);
        let offset = 0;

        for (let i = 0; i < this.totalChunks; i++) {
            const chunk = this.sourceChunks[i];
            const copyLength = Math.min(chunk.length, fileData.length - offset);
            chunk.copy(fileData, offset, 0, copyLength);
            offset += copyLength;
        }

        // Verify checksum if available
        if (this.metaData.fileChecksum) {
            const calculated = this.calculateChecksum(fileData);
            if (calculated === this.metaData.fileChecksum) {
                console.log(`✅ File integrity verified: checksum ${calculated}`);
            } else {
                console.log(`❌ Checksum failed: expected ${this.metaData.fileChecksum}, got ${calculated}`);
                return null;
            }
        }

        // Write file to output directory
        const outputPath = path.join(outputDir, this.metaData.fileName);
        fs.writeFileSync(outputPath, fileData);

        console.log(`✅ File saved: ${outputPath} (${fileData.length} bytes)`);
        return fileData;
    }

    calculateChecksum(data) {
        let hash = 2166136261; // FNV-1a offset basis
        for (let i = 0; i < data.length; i++) {
            hash ^= data[i];
            hash = Math.imul(hash, 16777619); // FNV-1a prime
        }
        return (hash >>> 0).toString(36).substring(0, 8);
    }
}

class StreamingQRDecoder {
    constructor() {
        this.fileDecoders = new Map();
        this.currentActiveDecoder = null;
        this.outputDir = './decoded_files';
        this.stats = {
            qrCodesProcessed: 0,
            metadataPackets: 0,
            dataPackets: 0,
            filesDiscovered: 0,
            filesCompleted: 0
        };
    }

    async processJSONLFile(inputFile) {
        console.log(`📖 Processing JSONL file: ${inputFile}`);

        // Create output directory
        fs.mkdirSync(this.outputDir, { recursive: true });

        // Read file line by line for memory efficiency
        const fileStream = fs.createReadStream(inputFile);
        const rl = readline.createInterface({
            input: fileStream,
            crlfDelay: Infinity
        });

        let lineCount = 0;
        let videoInfo = null;

        for await (const line of rl) {
            lineCount++;

            if (line.trim() === '') continue;

            try {
                const entry = JSON.parse(line);

                switch (entry.type) {
                    case 'header':
                        videoInfo = entry.video_info;
                        console.log(`📺 Video: ${videoInfo.width}x${videoInfo.height} @ ${videoInfo.fps.toFixed(1)}fps, ${(videoInfo.duration_seconds/60).toFixed(1)}min`);
                        break;

                    case 'qr_code':
                        this.processQRCode(entry.data, entry.frame_number);
                        this.stats.qrCodesProcessed++;

                        if (this.stats.qrCodesProcessed % 1000 === 0) {
                            console.log(`\n📊 Processed ${this.stats.qrCodesProcessed} QR codes...`);
                        }
                        break;

                    case 'footer':
                        console.log(`\n📋 Processing complete: ${entry.summary.qr_codes_found} QR codes from ${entry.summary.frames_processed} frames`);
                        break;

                    default:
                        console.log(`⚠️ Unknown entry type: ${entry.type}`);
                }

            } catch (error) {
                console.log(`❌ Error parsing line ${lineCount}: ${error.message}`);
            }
        }

        // Finalize any remaining files
        console.log(`\n🎯 Finalizing ${this.fileDecoders.size} discovered files...`);
        for (const [fileName, decoder] of this.fileDecoders) {
            if (decoder.isComplete()) {
                console.log(`\n🎉 Finalizing complete file: ${fileName}`);
                decoder.finalize(this.outputDir);
                this.stats.filesCompleted++;
            } else {
                const percentage = Math.round((decoder.recoveredChunkCount / decoder.totalChunks) * 100);
                console.log(`\n⚠️ File incomplete: ${fileName} - ${decoder.recoveredChunkCount}/${decoder.totalChunks} chunks (${percentage}%)`);
            }
        }

        console.log(`\n📊 Final Results:`);
        console.log(`   📱 QR codes processed: ${this.stats.qrCodesProcessed}`);
        console.log(`   📄 Files discovered: ${this.stats.filesDiscovered}`);
        console.log(`   ✅ Files completed: ${this.stats.filesCompleted}`);
        console.log(`   📁 Output directory: ${this.outputDir}`);
    }

    processQRCode(qrData, frameIndex) {
        try {
            if (qrData.startsWith('M:')) {
                return this.processMetadataPacket(qrData, frameIndex);
            } else if (qrData.startsWith('D:')) {
                return this.processDataPacket(qrData, frameIndex);
            }
            return { isValid: false, reason: 'Unknown packet type' };
        } catch (error) {
            return { isValid: false, reason: error.message };
        }
    }

    processMetadataPacket(metaString, frameIndex) {
        const parts = metaString.split(':');
        if (parts.length < 10) {
            throw new Error('Invalid metadata format');
        }

        const metadata = {
            version: parts[1],
            fileName: decodeURIComponent(parts[2]),
            fileType: decodeURIComponent(parts[3]),
            fileSize: parseInt(parts[4]),
            chunksCount: parseInt(parts[5]),
            fileChecksum: parts[13] || null
        };

        // Initialize new file decoder if not exists
        if (!this.fileDecoders.has(metadata.fileName)) {
            const decoder = new FountainDecoder();
            decoder.initialize(metadata);
            this.fileDecoders.set(metadata.fileName, decoder);
            this.stats.filesDiscovered++;
        }

        // Set as current active decoder (temporal routing)
        this.currentActiveDecoder = this.fileDecoders.get(metadata.fileName);
        this.stats.metadataPackets++;

        return { isValid: true, type: 'metadata' };
    }

    processDataPacket(dataString, frameIndex) {
        const parts = dataString.split(':');
        if (parts.length < 6) {
            throw new Error('Invalid data packet format');
        }

        const packet = {
            packetId: parseInt(parts[1]),
            sourceChunks: [],
            systematicDataChunks: [],
            xorData: null
        };

        // Parse enhanced format with proper field reconstruction
        if (parts.length >= 7) {
            const dataFieldOffset = 6;
            const allDataPart = parts.slice(dataFieldOffset).join(':');

            if (allDataPart.includes('|')) {
                // Systematic packet: chunkIndex:base64Data|chunkIndex:base64Data
                const records = allDataPart.split('|');

                for (const record of records) {
                    const chunkParts = record.split(':', 2);

                    if (chunkParts.length === 2) {
                        const chunkIndex = parseInt(chunkParts[0]);
                        const chunkDataB64 = chunkParts[1];

                        if (!isNaN(chunkIndex) && chunkDataB64) {
                            try {
                                const chunkData = Buffer.from(chunkDataB64, 'base64');
                                packet.sourceChunks.push(chunkIndex);
                                packet.systematicDataChunks.push({
                                    chunkIndex: chunkIndex,
                                    chunkData: chunkData
                                });
                            } catch (e) {
                                console.log(`❌ Failed to decode chunk ${chunkIndex}: ${e.message}`);
                            }
                        }
                    }
                }
            } else if (allDataPart.includes(',')) {
                // Fountain packet: comma-separated indices
                packet.sourceChunks = allDataPart.split(',').map(s => parseInt(s));
                if (parts.length >= 8) {
                    try {
                        packet.xorData = Buffer.from(parts[7], 'base64');
                    } catch (e) {
                        console.log(`Failed to decode fountain XOR data: ${e.message}`);
                    }
                }
            }
        }

        // Route to current active decoder (temporal routing)
        if (!this.currentActiveDecoder) {
            return { isValid: false, type: 'data' };
        }

        // Add packet to current active decoder
        const success = this.currentActiveDecoder.addPacket(packet);
        this.stats.dataPackets++;

        // Check if file is complete
        if (this.currentActiveDecoder.isComplete()) {
            console.log('\n🎉 File complete! Finalizing...');
            this.currentActiveDecoder.finalize(this.outputDir);
            this.stats.filesCompleted++;
        }

        return { isValid: success, type: 'data' };
    }
}

// Auto-detect input format and process accordingly
async function main() {
    const args = process.argv.slice(2);
    if (args.length < 1) {
        console.log(`
🎯 JSONL QR Code Decoder

Usage: node decode_streaming_qr.js <input_file>

Supported formats:
  📄 Standard JSON: {"sequenced_qr_codes": [...]}
  🌊 Streaming JSONL: One JSON object per line

JSONL Format:
  {"type":"header","video_info":{...}}
  {"type":"qr_code","frame_number":0,"data":"M:..."}
  {"type":"qr_code","frame_number":40,"data":"D:..."}
  {"type":"footer","summary":{...}}

Examples:
  node decode_streaming_qr.js qr_codes.json        # Standard format
  node decode_streaming_qr.js qr_codes.jsonl       # Streaming format
        `);
        process.exit(1);
    }

    const inputFile = args[0];
    console.log(`📖 Loading QR codes from: ${inputFile}`);

    try {
        // Auto-detect format by reading first line
        const firstLine = fs.readFileSync(inputFile, 'utf8').split('\n')[0];

        let isJSONL = false;
        try {
            const firstEntry = JSON.parse(firstLine);
            isJSONL = firstEntry.type === 'header' || firstEntry.type === 'qr_code';
        } catch (e) {
            isJSONL = false;
        }

        const decoder = new StreamingQRDecoder();

        if (isJSONL) {
            console.log(`🌊 Detected streaming JSONL format`);
            await decoder.processJSONLFile(inputFile);
        } else {
            console.log(`📄 Detected standard JSON format`);
            // Process standard JSON format (existing logic)
            const data = JSON.parse(fs.readFileSync(inputFile, 'utf8'));

            const qrCodes = data.sequenced_qr_codes?.map(item => item.data) || data.unique_qr_codes || [];
            console.log(`Found ${qrCodes.length} QR codes in temporal order`);

            // Process QR codes sequentially
            for (let i = 0; i < qrCodes.length; i++) {
                if (i % 1000 === 0) {
                    console.log(`\nProcessing QR code ${i + 1}/${qrCodes.length}...`);
                }
                decoder.processQRCode(qrCodes[i], i);
            }

            // Finalize files
            for (const [fileName, fontainDecoder] of decoder.fileDecoders) {
                if (fontainDecoder.isComplete()) {
                    console.log(`\n🎉 Finalizing complete file: ${fileName}`);
                    fontainDecoder.finalize('./decoded_files');
                } else {
                    const percentage = Math.round((fontainDecoder.recoveredChunkCount / fontainDecoder.totalChunks) * 100);
                    console.log(`\n⚠️ File incomplete: ${fileName} - ${fontainDecoder.recoveredChunkCount}/${fontainDecoder.totalChunks} chunks (${percentage}%)`);
                }
            }
        }

    } catch (error) {
        console.error(`❌ Error: ${error.message}`);
        process.exit(1);
    }
}

main();