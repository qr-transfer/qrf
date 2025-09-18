#!/usr/bin/env node
import { spawn } from 'child_process';
import jsQR from 'jsqr';
import sharp from 'sharp';
import fs from 'fs/promises';
import path from 'path';
import crypto from 'crypto';
import ffmpeg from 'fluent-ffmpeg';

class WorkingDecoder {
  constructor() {
    // Match HTML structure exactly
    this.discoveredFiles = new Map(); // Map of filename -> {metadata, firstSeenAt, lastSeenAt, completed, chunks}
    this.currentActiveFile = null;
    this.downloadedFiles = new Set();
    this.qrCodesDetected = 0;
    this.packetsProcessed = 0;
    this.startTime = Date.now();
    this.frameCount = 0;

    // Initialize packet processor
    this.packetProcessor = new PacketProcessor();
  }

  // Process QR data exactly like HTML
  async handleQRDetection(qrData, frameIndex, timestamp) {
    this.qrCodesDetected++;

    // Process the QR data using HTML logic
    const packetResult = this.packetProcessor.processQRData(qrData, frameIndex);

    if (packetResult.success) {
      this.packetsProcessed++;

      if (packetResult.packetType === 'metadata') {
        this.handleMetadataPacket(packetResult.packetData, timestamp);
      } else if (packetResult.packetType === 'data') {
        this.handleDataPacket(packetResult.packetData, timestamp);
      }
    }
  }

  // Handle metadata packet exactly like HTML
  handleMetadataPacket(metadata, currentTime = 0) {
    const fileName = metadata.fileName;

    // Handle file discovery
    const isNewFileDiscovered = !this.discoveredFiles.has(fileName);

    if (isNewFileDiscovered) {
      // Calculate fileId like HTML
      const fileId = metadata.fileChecksum ?
        metadata.fileChecksum.substring(0, 8) :
        crypto.createHash('md5').update(metadata.fileName).digest('hex').substring(0, 8);

      this.discoveredFiles.set(fileName, {
        metadata: metadata,
        fileId: fileId,
        firstSeenAt: currentTime,
        lastSeenAt: currentTime,
        completed: false,
        chunks: new Map(),
        recoveredChunks: 0
      });

      console.log(`\n✅ METADATA FOUND at ${currentTime.toFixed(1)}s: ${fileName}`);
      console.log(`   Type: ${metadata.fileType}`);
      console.log(`   Size: ${(metadata.fileSize / 1024).toFixed(1)} KB`);
      console.log(`   Chunks: ${metadata.chunksCount}`);
      console.log(`   FileID: ${fileId}\n`);

      // Switch to processing this file
      this.switchToFile(metadata, fileName);
    } else {
      // Existing file - update timing
      const fileInfo = this.discoveredFiles.get(fileName);
      fileInfo.lastSeenAt = currentTime;
    }
  }

  // Handle data packet with chunk processing
  handleDataPacket(packetData, timestamp) {
    // Find target file based on packet data
    let targetFile = null;
    let fileInfo = null;

    // Method 1: Direct fileId match (new format)
    if (packetData.fileId) {
      for (const [fileName, info] of this.discoveredFiles) {
        if (info.fileId === packetData.fileId) {
          targetFile = fileName;
          fileInfo = info;
          break;
        }
      }
    }

    // Method 2: Match by chunk count (legacy format)
    if (!targetFile && packetData.numChunks) {
      for (const [fileName, info] of this.discoveredFiles) {
        if (info.metadata.chunksCount === packetData.numChunks) {
          targetFile = fileName;
          fileInfo = info;
          break; // Take first match
        }
      }
    }

    // Method 3: Store for unknown files
    if (!targetFile && packetData.numChunks) {
      const unknownKey = `unknown_${packetData.numChunks}chunks`;
      if (!this.discoveredFiles.has(unknownKey)) {
        console.log(`\n⚠️  Data packets found for unknown file (${packetData.numChunks} chunks)`);

        this.discoveredFiles.set(unknownKey, {
          metadata: {
            fileName: unknownKey,
            fileSize: 0,
            chunksCount: packetData.numChunks,
            fileType: 'application/octet-stream'
          },
          fileId: null,
          firstSeenAt: timestamp,
          lastSeenAt: timestamp,
          completed: false,
          chunks: new Map(),
          recoveredChunks: 0
        });
      }
      targetFile = unknownKey;
      fileInfo = this.discoveredFiles.get(unknownKey);
    }

    if (fileInfo && !fileInfo.completed) {
      this.addChunkToFile(targetFile, fileInfo, packetData, timestamp);
    }
  }

  // Add chunk to file with HTML-like logic
  addChunkToFile(fileName, fileInfo, packet, timestamp) {
    // Determine chunk index - handle both formats
    let chunkIndex;
    if (packet.sourceIndices && packet.sourceIndices.length > 0) {
      // Use first source index for simplicity
      chunkIndex = packet.sourceIndices[0];
    } else {
      chunkIndex = packet.packetId % fileInfo.metadata.chunksCount;
    }

    // Check if we already have this chunk
    if (fileInfo.chunks.has(chunkIndex)) {
      return; // Duplicate
    }

    // Decode base64 data
    try {
      const chunkData = Buffer.from(packet.data, 'base64');

      fileInfo.chunks.set(chunkIndex, chunkData);
      fileInfo.recoveredChunks++;
      fileInfo.lastSeenAt = timestamp;

      // Show progress
      if (fileInfo.recoveredChunks % 10 === 0 || fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
        const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);
        console.log(`📦 ${fileName}: ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount} chunks (${progress}%)`);
      }

      // Check if file is complete
      if (fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
        fileInfo.completed = true;
        console.log(`\n✅ File complete: ${fileName} at ${timestamp.toFixed(1)}s\n`);
      }
    } catch (error) {
      console.error(`Failed to decode chunk: ${error.message}`);
    }
  }

  // Switch to processing a file
  switchToFile(metadata, fileName) {
    this.currentActiveFile = fileName;
    console.log(`🔄 Switched to processing file: ${fileName}`);
  }

  // Save completed files
  async saveCompletedFiles(outputDir) {
    const results = [];

    for (const [fileName, fileInfo] of this.discoveredFiles) {
      if (!fileInfo.completed || fileName.startsWith('unknown_')) continue;

      try {
        // Combine chunks in order
        const chunks = [];
        for (let i = 0; i < fileInfo.metadata.chunksCount; i++) {
          if (fileInfo.chunks.has(i)) {
            chunks.push(fileInfo.chunks.get(i));
          } else {
            console.log(`⚠️  Missing chunk ${i} for ${fileName}`);
            return; // Skip incomplete files
          }
        }

        if (chunks.length === fileInfo.metadata.chunksCount) {
          const fileData = Buffer.concat(chunks);

          // Verify checksum if available
          if (fileInfo.metadata.fileChecksum) {
            const hash = crypto.createHash('sha256').update(fileData).digest('hex');
            if (hash !== fileInfo.metadata.fileChecksum) {
              console.log(`⚠️  Checksum mismatch for ${fileName}`);
            } else {
              console.log(`✓ Checksum verified for ${fileName}`);
            }
          }

          const outputPath = path.join(outputDir, fileName);
          await fs.writeFile(outputPath, fileData);

          console.log(`💾 Saved: ${outputPath} (${(fileData.length / 1024).toFixed(1)} KB)`);
          results.push({ fileName, success: true, size: fileData.length });
        }
      } catch (error) {
        console.error(`Failed to save ${fileName}: ${error.message}`);
        results.push({ fileName, success: false, error: error.message });
      }
    }

    return results;
  }

  showProgress() {
    const elapsed = (Date.now() - this.startTime) / 1000;
    const fps = this.frameCount / elapsed;

    let completedCount = 0;
    let totalChunks = 0;
    let recoveredChunks = 0;

    for (const fileInfo of this.discoveredFiles.values()) {
      if (fileInfo.completed) completedCount++;
      totalChunks += fileInfo.metadata.chunksCount;
      recoveredChunks += fileInfo.recoveredChunks;
    }

    process.stdout.write(`\r⚡ Frames: ${this.frameCount} | QRs: ${this.qrCodesDetected} | Files: ${this.discoveredFiles.size} (${completedCount} done) | Chunks: ${recoveredChunks}/${totalChunks} | Speed: ${fps.toFixed(0)} fps`);
  }
}

// Packet processor that matches HTML exactly
class PacketProcessor {
  processQRData(qrData, frameIndex) {
    try {
      if (qrData.startsWith('M:')) {
        return this.processMetadataPacket(qrData, frameIndex);
      } else if (qrData.startsWith('D:')) {
        return this.processDataPacket(qrData, frameIndex);
      } else {
        throw new Error(`Unknown packet format: ${qrData.substring(0, 10)}...`);
      }
    } catch (error) {
      return {
        success: false,
        error: error.message,
        frameIndex
      };
    }
  }

  processMetadataPacket(metaString, frameIndex) {
    const parts = metaString.split(':');

    if (parts.length < 10) {
      throw new Error(`Invalid metadata packet format: ${metaString.substring(0, 30)}...`);
    }

    const metadata = {
      protocolVersion: parts[1],
      fileName: this.decodeURIComponentSafe(parts[2]),
      fileType: this.decodeURIComponentSafe(parts[3]),
      fileSize: parseInt(parts[4]),
      chunksCount: parseInt(parts[5]),
      packetCount: parseInt(parts[6] || '0'),
      maxDegree: parseInt(parts[7] || '1'),
      density: parseFloat(parts[8] || '1.0'),
      fps: parts[9] || '30',
      chunkSize: parseInt(parts[10] || '1024'),
      redundancy: parseInt(parts[11] || '0'),
      ecl: parts[12] || 'L',
      metaChecksum: parts[13] || '',
      fileChecksum: parts[14] || '',
      encoderVersion: parts[15] || '3.0',
      ltParams: parts.slice(16).join(':')
    };

    if (isNaN(metadata.chunksCount) || metadata.chunksCount <= 0) {
      throw new Error(`Invalid chunk count: ${parts[5]}`);
    }

    return {
      success: true,
      packetType: 'metadata',
      packetData: metadata,
      frameIndex
    };
  }

  processDataPacket(dataString, frameIndex) {
    const parts = dataString.split(':');

    // Auto-detect packet format
    let fileId, packetId, numChunks, degree;
    let sourceIndices = [];

    if (parts.length >= 8 && parts[1] && parts[1].length === 8 && /^[a-fA-F0-9]{8}$/.test(parts[1])) {
      // New format with fileId
      fileId = parts[1];
      packetId = parseInt(parts[2]);
      numChunks = parseInt(parts[5]);
      degree = parseInt(parts[6]);
      // parts[7] might contain source indices like "182,41"
      if (parts[7] && parts[7].includes(',')) {
        sourceIndices = parts[7].split(',').map(x => parseInt(x));
      }
    } else if (parts.length >= 7) {
      // Legacy format
      fileId = null;
      packetId = parseInt(parts[1]);
      numChunks = parseInt(parts[4]);
      degree = parseInt(parts[5]);
      // parts[6] might contain source indices
      if (parts[6] && parts[6].includes(',')) {
        sourceIndices = parts[6].split(',').map(x => parseInt(x));
      }
    } else {
      throw new Error(`Invalid data packet format: ${dataString.substring(0, 50)}...`);
    }

    const packet = {
      fileId,
      packetId,
      numChunks,
      degree,
      sourceIndices,
      data: parts.slice(fileId ? 8 : 7).join(':')
    };

    return {
      success: true,
      packetType: 'data',
      packetData: packet,
      frameIndex
    };
  }

  decodeURIComponentSafe(str) {
    try {
      return decodeURIComponent(str);
    } catch (e) {
      return str;
    }
  }
}

// Frame extraction and processing
async function processVideoByFrames(videoPath, fps, maxFrames, decoder) {
  const tempDir = './tmp/processing_frames';
  await fs.mkdir(tempDir, { recursive: true });

  console.log(`🎬 Extracting frames to ${tempDir}...`);

  // Extract frames with rotation correction
  await new Promise((resolve, reject) => {
    const args = [
      '-i', videoPath,
      '-vf', `fps=${fps},transpose=2,transpose=2`, // 180 degree rotation
      '-frames:v', maxFrames.toString(),
      '-q:v', '2',
      path.join(tempDir, 'frame_%06d.jpg')
    ];

    const ffmpegProcess = spawn('ffmpeg', args, {
      stdio: ['ignore', 'ignore', 'ignore']
    });

    ffmpegProcess.on('close', (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`FFmpeg exited with code ${code}`));
      }
    });
  });

  // Get list of extracted frames
  const files = await fs.readdir(tempDir);
  const frameFiles = files
    .filter(f => f.startsWith('frame_') && f.endsWith('.jpg'))
    .sort()
    .map(f => path.join(tempDir, f));

  console.log(`✅ Extracted ${frameFiles.length} frames`);
  console.log(`🔍 Processing frames for QR codes...\n`);

  // Process each frame
  for (let i = 0; i < frameFiles.length; i++) {
    const frameFile = frameFiles[i];
    const timestamp = i / fps;

    try {
      // Load frame and detect QR
      const { data, info } = await sharp(frameFile)
        .raw()
        .ensureAlpha()
        .toBuffer({ resolveWithObject: true });

      const qrResult = jsQR(data, info.width, info.height);

      if (qrResult && qrResult.data) {
        await decoder.handleQRDetection(qrResult.data, i, timestamp);
      }

      decoder.frameCount = i + 1;

      // Update progress every 50 frames
      if (i % 50 === 0) {
        decoder.showProgress();
      }

    } catch (error) {
      // Ignore frame processing errors
    }
  }

  // Cleanup
  try {
    await fs.rm(tempDir, { recursive: true });
  } catch (error) {
    // Ignore cleanup errors
  }
}

// Main function
async function decodeVideo(videoPath, options) {
  const fps = options.fps || 15;
  const maxFrames = options.maxFrames || 5000;
  const outputDir = options.output || './decoded';

  console.log('\n🎬 QRF Working Decoder (Frame Extraction)\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`⚡ Extract rate: ${fps} FPS`);
  console.log(`📁 Output: ${outputDir}`);
  console.log(`🔄 Max frames: ${maxFrames}`);
  console.log(`✅ Frame-based processing (proven to work)\n`);
  console.log('─'.repeat(60) + '\n');

  // Get video duration
  const metadata = await new Promise((resolve, reject) => {
    ffmpeg.ffprobe(videoPath, (err, data) => {
      if (err) reject(err);
      else resolve(data);
    });
  });

  const duration = metadata.format.duration;
  console.log(`📊 Video duration: ${duration.toFixed(1)}s`);
  console.log(`📊 Processing: ${(maxFrames / fps).toFixed(1)}s of video\n`);

  // Create output directory
  await fs.mkdir(outputDir, { recursive: true });

  // Initialize decoder
  const decoder = new WorkingDecoder();

  // Process video by extracting frames
  await processVideoByFrames(videoPath, fps, maxFrames, decoder);

  // Show final results
  decoder.showProgress();

  console.log('\n' + '─'.repeat(60));
  console.log('\n📊 Processing Complete!\n');

  const elapsed = ((Date.now() - decoder.startTime) / 1000).toFixed(1);
  console.log(`   Frames processed: ${decoder.frameCount}`);
  console.log(`   QR codes detected: ${decoder.qrCodesDetected}`);
  console.log(`   Packets processed: ${decoder.packetsProcessed}`);
  console.log(`   Processing time: ${elapsed}s`);
  console.log(`   Average speed: ${(decoder.frameCount / elapsed).toFixed(0)} fps\n`);

  console.log('📁 Discovered Files:\n');

  let completedCount = 0;
  for (const [fileName, fileInfo] of decoder.discoveredFiles) {
    const icon = fileInfo.completed ? '✅' : '⏳';
    const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);
    console.log(`${icon} ${fileName}`);
    console.log(`   Progress: ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount} chunks (${progress}%)`);
    console.log(`   Size: ${(fileInfo.metadata.fileSize / 1024).toFixed(1)} KB`);
    if (fileInfo.completed) {
      console.log(`   Completed at: ${fileInfo.lastSeenAt.toFixed(1)}s`);
      completedCount++;
    }
    console.log();
  }

  // Save completed files
  if (completedCount > 0) {
    console.log('💾 Saving completed files...\n');
    const results = await decoder.saveCompletedFiles(outputDir);

    for (const result of results) {
      if (result.success) {
        console.log(`   ✅ ${result.fileName} - ${(result.size / 1024).toFixed(1)} KB`);
      } else {
        console.log(`   ❌ ${result.fileName} - ${result.error}`);
      }
    }
  }

  console.log(`\n📊 Summary: ${completedCount}/${decoder.discoveredFiles.size} files completed`);
  console.log(`⏱️  Total time: ${elapsed}s\n`);
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node decoder-working.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>       Frame extraction rate (default: 15)');
  console.log('  --maxFrames <n>    Maximum frames to process (default: 5000)');
  console.log('  --output <dir>     Output directory (default: ./decoded)');
  console.log('\nWorking decoder using frame extraction (proven method)');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 15,
  maxFrames: 5000,
  output: './decoded'
};

// Parse options
for (let i = 3; i < process.argv.length; i += 2) {
  if (process.argv[i] === '--fps') {
    options.fps = parseFloat(process.argv[i + 1]);
  } else if (process.argv[i] === '--maxFrames') {
    options.maxFrames = parseInt(process.argv[i + 1]);
  } else if (process.argv[i] === '--output') {
    options.output = process.argv[i + 1];
  }
}

decodeVideo(videoPath, options).catch(console.error);