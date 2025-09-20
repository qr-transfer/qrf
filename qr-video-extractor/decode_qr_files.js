#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

// Simple JavaScript decoder based on the HTML script logic
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

    // Check if file is nearly complete (for debugging)
    isNearlyComplete(threshold = 0.95) {
        return (this.recoveredChunkCount / this.totalChunks) >= threshold;
    }

    printProgress() {
        const percentage = Math.round((this.recoveredChunkCount / this.totalChunks) * 100);
        const progressBar = '🟩'.repeat(Math.floor(percentage / 2)) + '⬜'.repeat(50 - Math.floor(percentage / 2));
        process.stdout.write(`\r🔄 Progress: ${this.recoveredChunkCount}/${this.totalChunks} (${percentage}%) [${progressBar}]`);
    }

    finalize(outputDir) {
        if (!this.isComplete()) {
            console.log(`\n❌ File incomplete: ${this.recoveredChunkCount}/${this.totalChunks} chunks`);

            // Debug: show which chunks are missing
            const missing = [];
            for (let i = 0; i < this.totalChunks; i++) {
                if (!this.sourceChunks[i]) {
                    missing.push(i);
                }
            }
            console.log(`Missing chunks: ${missing.slice(0, 10).join(', ')}${missing.length > 10 ? ` ... and ${missing.length - 10} more` : ''}`);
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

class QRFileDecoder {
    constructor() {
        this.fileDecoders = new Map(); // Track multiple files
        this.currentActiveDecoder = null; // Current active decoder (temporal routing)
        this.outputDir = './decoded_files';
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
        }

        // Set as current active decoder (temporal routing)
        this.currentActiveDecoder = this.fileDecoders.get(metadata.fileName);
        console.log(`🎯 Switched to processing: ${metadata.fileName}`);

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

        // Parse enhanced format - CORRECTED to match HTML script exactly
        if (parts.length >= 7) {
            const chunkCount = parseInt(parts[5]);
            const dataFieldOffset = 6; // Standard format offset

            // Reconstruct data part by joining from dataFieldOffset onwards (critical fix!)
            const allDataPart = parts.slice(dataFieldOffset).join(':');

            if (allDataPart.includes('|')) {
                // Systematic packet format: chunkIndex:base64Data|chunkIndex:base64Data
                const records = allDataPart.split('|');

                // Debug: log packet structure for first few packets
                if (packet.packetId <= 5) {
                    console.log(`\n🔍 DEBUG Packet ${packet.packetId}: chunkCount=${chunkCount}, records=${records.length}`);
                    console.log(`  AllDataPart length: ${allDataPart.length}`);
                    records.forEach((record, idx) => {
                        const colonIndex = record.indexOf(':');
                        if (colonIndex > 0) {
                            console.log(`  Record ${idx}: chunk ${record.substring(0, colonIndex)}, data length ${record.length - colonIndex - 1}`);
                        } else {
                            console.log(`  Record ${idx}: no colon, length ${record.length}`);
                        }
                    });
                }

                for (const record of records) {
                    const chunkParts = record.split(':', 2); // Split into exactly 2 parts

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

                                if (packet.packetId <= 5) {
                                    console.log(`    ✅ Decoded chunk ${chunkIndex}: ${chunkData.length} bytes`);
                                }
                            } catch (e) {
                                console.log(`❌ Failed to decode chunk ${chunkIndex}: ${e.message}`);
                            }
                        }
                    }
                }
            } else if (allDataPart.includes(',')) {
                // Fountain packet: comma-separated indices
                packet.sourceChunks = allDataPart.split(',').map(s => parseInt(s));
                // XOR data would be in next field for fountain packets
                if (parts.length >= 8) {
                    try {
                        packet.xorData = Buffer.from(parts[7], 'base64');
                    } catch (e) {
                        console.log(`Failed to decode fountain XOR data: ${e.message}`);
                    }
                }
            }
        }

        // Route to current active decoder (temporal routing - CRITICAL FIX!)
        if (!this.currentActiveDecoder) {
            console.log(`⚠️ No active decoder for data packet ${packet.packetId}`);
            return { isValid: false, type: 'data' };
        }

        // Add packet to current active decoder
        const success = this.currentActiveDecoder.addPacket(packet);

        // Check if file is complete
        if (this.currentActiveDecoder.isComplete()) {
            console.log('\n🎉 File complete! Finalizing...');
            this.currentActiveDecoder.finalize(this.outputDir);
        }

        return { isValid: success, type: 'data' };
    }
}

// Main processing function
async function main() {
    const args = process.argv.slice(2);
    if (args.length < 1) {
        console.log('Usage: node decode_qr_files.js <qr_codes.json>');
        process.exit(1);
    }

    const inputFile = args[0];
    console.log(`📖 Loading QR codes from: ${inputFile}`);

    try {
        // Create output directory
        fs.mkdirSync('./decoded_files', { recursive: true });

        // Load QR codes (support both formats)
        const data = JSON.parse(fs.readFileSync(inputFile, 'utf8'));

        let qrCodes;
        if (data.sequenced_qr_codes) {
            // Sequenced format - already perfectly ordered by frame number
            qrCodes = data.sequenced_qr_codes.map(item => item.data);
            console.log(`📊 Using sequenced format with frame-perfect ordering`);
            console.log(`📺 Video info: ${Math.round(data.video_info.duration_seconds/60)}min, ${Math.round(data.video_info.fps)}fps, ${data.video_info.total_frames} total frames`);
        } else {
            // Legacy format
            qrCodes = data.unique_qr_codes || [];
            console.log(`📊 Using legacy format (temporal order)`);
        }

        console.log(`Found ${qrCodes.length} QR codes in temporal order`);

        // Initialize decoder
        const decoder = new QRFileDecoder();

        // Process QR codes
        let processed = 0;
        let successful = 0;

        for (let i = 0; i < qrCodes.length; i++) {
            if (i % 100 === 0) {
                console.log(`\nProcessing QR code ${i + 1}/${qrCodes.length}...`);
            }

            try {
                const result = decoder.processQRCode(qrCodes[i], i);
                if (result.isValid) {
                    successful++;
                }
                processed++;
            } catch (error) {
                console.log(`Warning: Failed to process QR ${i + 1}: ${error.message}`);
            }
        }

        // Finalize any remaining files and save partial progress
        let completedFiles = 0;
        let partialFiles = 0;

        for (const [fileName, fountainDecoder] of decoder.fileDecoders) {
            if (fountainDecoder.isComplete()) {
                console.log(`\n🎉 Finalizing complete file: ${fileName}`);
                fountainDecoder.finalize('./decoded_files');
                completedFiles++;
            } else {
                const percentage = Math.round((fountainDecoder.recoveredChunkCount / fountainDecoder.totalChunks) * 100);
                console.log(`\n⚠️ File incomplete: ${fileName} - ${fountainDecoder.recoveredChunkCount}/${fountainDecoder.totalChunks} chunks (${percentage}%)`);

                // Show missing chunks
                const missing = [];
                for (let i = 0; i < fountainDecoder.totalChunks; i++) {
                    if (!fountainDecoder.sourceChunks[i]) {
                        missing.push(i);
                    }
                }
                if (missing.length <= 10) {
                    console.log(`   Missing chunks: [${missing.join(', ')}]`);
                } else {
                    console.log(`   Missing chunks: [${missing.slice(0, 5).join(', ')}, ..., ${missing.slice(-5).join(', ')}] (${missing.length} total)`);
                }

                // For nearly complete files, show more details
                if (fountainDecoder.isNearlyComplete(0.95)) {
                    console.log(`   🔍 NEARLY COMPLETE: ${percentage}% - only ${missing.length} chunks missing!`);

                    // Show if this could be completed with available fountain packets
                    console.log(`   📊 Available fountain packets: ${fountainDecoder.codedPackets.length}`);
                }

                // Save partial file progress for potential merging later
                if (percentage >= 10) { // Only save if significant progress
                    const partialData = {
                        fileName: fileName,
                        metadata: fountainDecoder.metaData,
                        recoveredChunks: Object.keys(fountainDecoder.sourceChunks).length,
                        totalChunks: fountainDecoder.totalChunks,
                        percentage: percentage,
                        missingChunks: missing,
                        availableFountainPackets: fountainDecoder.codedPackets.length
                    };

                    const partialPath = `./decoded_files/${fileName}.partial.json`;
                    fs.writeFileSync(partialPath, JSON.stringify(partialData, null, 2));
                    console.log(`   💾 Saved partial progress to: ${partialPath}`);
                    partialFiles++;
                }
            }
        }

        console.log(`\n📊 Final Results:`);
        console.log(`   ✅ Complete files: ${completedFiles}`);
        console.log(`   📝 Partial files: ${partialFiles}`);
        console.log(`   🎯 Success rate: ${Math.round(completedFiles/(completedFiles + partialFiles)*100)}%`);

        if (completedFiles > 0) {
            console.log(`\n🎉 SUCCESS: ${completedFiles} files fully reconstructed with integrity verification!`);
        }

        console.log(`\n✅ Processing complete: ${successful}/${processed} QR codes successfully processed`);
        console.log(`📁 Check './decoded_files' directory for extracted files`);

    } catch (error) {
        console.error(`❌ Error: ${error.message}`);
        process.exit(1);
    }
}

main();