#!/usr/bin/env node
import { spawn } from 'child_process';
import jsQR from 'jsqr';
import sharp from 'sharp';
import fs from 'fs/promises';
import path from 'path';
import crypto from 'crypto';
import ffmpeg from 'fluent-ffmpeg';

class MemoryDecoder {
  constructor() {
    this.discoveredFiles = new Map();
    this.currentActiveFile = null;
    this.downloadedFiles = new Set();
    this.qrCodesDetected = 0;
    this.packetsProcessed = 0;
    this.startTime = Date.now();
    this.frameCount = 0;
    this.packetProcessor = new PacketProcessor();
  }

  async handleQRDetection(qrData, frameIndex, timestamp) {
    this.qrCodesDetected++;

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

  handleMetadataPacket(metadata, currentTime = 0) {
    const fileName = metadata.fileName;
    const isNewFileDiscovered = !this.discoveredFiles.has(fileName);

    if (isNewFileDiscovered) {
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

      this.switchToFile(metadata, fileName);
    } else {
      const fileInfo = this.discoveredFiles.get(fileName);
      fileInfo.lastSeenAt = currentTime;
    }
  }

  handleDataPacket(packetData, timestamp) {
    let targetFile = null;
    let fileInfo = null;

    // Method 1: Direct fileId match
    if (packetData.fileId) {
      for (const [fileName, info] of this.discoveredFiles) {
        if (info.fileId === packetData.fileId) {
          targetFile = fileName;
          fileInfo = info;
          break;
        }
      }
    }

    // Method 2: Match by chunk count
    if (!targetFile && packetData.numChunks) {
      for (const [fileName, info] of this.discoveredFiles) {
        if (info.metadata.chunksCount === packetData.numChunks) {
          targetFile = fileName;
          fileInfo = info;
          break;
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

  addChunkToFile(fileName, fileInfo, packet, timestamp) {
    let chunkIndex;
    if (packet.sourceIndices && packet.sourceIndices.length > 0) {
      chunkIndex = packet.sourceIndices[0];
    } else {
      chunkIndex = packet.packetId % fileInfo.metadata.chunksCount;
    }

    if (fileInfo.chunks.has(chunkIndex)) {
      return; // Duplicate
    }

    try {
      const chunkData = Buffer.from(packet.data, 'base64');

      fileInfo.chunks.set(chunkIndex, chunkData);
      fileInfo.recoveredChunks++;
      fileInfo.lastSeenAt = timestamp;

      // Show progress every 20 chunks
      if (fileInfo.recoveredChunks % 20 === 0 || fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
        const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);
        console.log(`📦 ${fileName}: ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount} chunks (${progress}%)`);
      }

      // Check if file is complete
      if (fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
        fileInfo.completed = true;
        console.log(`\n✅ File complete: ${fileName} at ${timestamp.toFixed(1)}s\n`);

        // Auto-save completed file immediately
        this.saveCompletedFile(fileName, fileInfo).catch(console.error);
      }
    } catch (error) {
      console.error(`Failed to decode chunk: ${error.message}`);
    }
  }

  switchToFile(metadata, fileName) {
    this.currentActiveFile = fileName;
    console.log(`🔄 Switched to processing file: ${fileName}`);
  }

  // Save individual completed file immediately
  async saveCompletedFile(fileName, fileInfo) {
    if (fileName.startsWith('unknown_') || this.downloadedFiles.has(fileName)) return;

    try {
      // Combine chunks in order
      const chunks = [];
      for (let i = 0; i < fileInfo.metadata.chunksCount; i++) {
        if (fileInfo.chunks.has(i)) {
          chunks.push(fileInfo.chunks.get(i));
        } else {
          console.log(`⚠️  Missing chunk ${i} for ${fileName}`);
          return;
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

        // Ensure output directory exists
        await fs.mkdir('./decoded', { recursive: true });

        const outputPath = path.join('./decoded', fileName);
        await fs.writeFile(outputPath, fileData);

        console.log(`💾 SAVED: ${outputPath} (${(fileData.length / 1024).toFixed(1)} KB)`);
        this.downloadedFiles.add(fileName);

        // Clear chunks from memory to save space
        fileInfo.chunks.clear();
      }
    } catch (error) {
      console.error(`Failed to save ${fileName}: ${error.message}`);
    }
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

    process.stdout.write(`\r⚡ Frames: ${this.frameCount} | QRs: ${this.qrCodesDetected} | Files: ${this.discoveredFiles.size} (${completedCount} done) | Chunks: ${recoveredChunks}/${totalChunks} | Speed: ${fps.toFixed(0)} fps | Saved: ${this.downloadedFiles.size}`);
  }
}

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

    let fileId, packetId, numChunks, degree;
    let sourceIndices = [];

    if (parts.length >= 8 && parts[1] && parts[1].length === 8 && /^[a-fA-F0-9]{8}$/.test(parts[1])) {
      // New format with fileId
      fileId = parts[1];
      packetId = parseInt(parts[2]);
      numChunks = parseInt(parts[5]);
      degree = parseInt(parts[6]);
      if (parts[7] && parts[7].includes(',')) {
        sourceIndices = parts[7].split(',').map(x => parseInt(x));
      }
    } else if (parts.length >= 7) {
      // Legacy format
      fileId = null;
      packetId = parseInt(parts[1]);
      numChunks = parseInt(parts[4]);
      degree = parseInt(parts[5]);
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

// Memory-only video processing - NO file saving
async function processVideoInMemory(videoPath, fps, decoder) {
  return new Promise((resolve, reject) => {
    let frameId = 0;

    console.log('🚀 Processing ENTIRE video in memory (no file I/O overhead)...\n');

    // FFmpeg with rotation correction, output raw JPEG stream
    const args = [
      '-i', videoPath,
      '-threads', '0',
      '-vf', `fps=${fps},transpose=2,transpose=2`, // 180 degree rotation
      '-c:v', 'mjpeg',
      '-q:v', '2',
      '-f', 'image2pipe',
      '-'
    ];

    const ffmpegProcess = spawn('ffmpeg', args, {
      stdio: ['ignore', 'pipe', 'ignore']
    });

    let buffer = Buffer.alloc(0);

    // Process frame directly in memory
    const processFrameInMemory = async (frameData, id) => {
      try {
        // Use Sharp to decode JPEG directly from buffer
        const { data, info } = await sharp(frameData)
          .raw()
          .ensureAlpha()
          .toBuffer({ resolveWithObject: true });

        const qrResult = jsQR(data, info.width, info.height);

        if (qrResult && qrResult.data) {
          const timestamp = id / fps;
          await decoder.handleQRDetection(qrResult.data, id, timestamp);
        }
      } catch (error) {
        // Ignore frame processing errors
      }
    };

    ffmpegProcess.stdout.on('data', async (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);

      let frameStart = 0;
      while (true) {
        const jpegStart = buffer.indexOf(Buffer.from([0xFF, 0xD8]), frameStart);
        if (jpegStart === -1) break;

        const jpegEnd = buffer.indexOf(Buffer.from([0xFF, 0xD9]), jpegStart + 2);
        if (jpegEnd === -1) break;

        const frame = buffer.slice(jpegStart, jpegEnd + 2);

        // Process frame completely in memory
        await processFrameInMemory(frame, frameId);

        decoder.frameCount = frameId++;

        // Update progress every 100 frames
        if (frameId % 100 === 0) {
          decoder.showProgress();
        }

        frameStart = jpegEnd + 2;
      }

      if (frameStart > 0 && frameStart < buffer.length) {
        buffer = buffer.slice(frameStart);
      } else if (frameStart >= buffer.length) {
        buffer = Buffer.alloc(0);
      }
    });

    ffmpegProcess.on('close', (code) => {
      console.log('\n');
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`FFmpeg exited with code ${code}`));
      }
    });

    ffmpegProcess.on('error', reject);
  });
}

// Main function - process ENTIRE video
async function decodeVideo(videoPath, options) {
  const fps = options.fps || 15;
  const outputDir = options.output || './decoded';

  console.log('\n🎬 QRF Memory Decoder (Full Video Processing)\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`⚡ Scan rate: ${fps} FPS`);
  console.log(`📁 Output: ${outputDir}`);
  console.log(`🧠 Memory-only processing (no temp files)`);
  console.log(`♾️  Processing ENTIRE video (no limits)\n`);
  console.log('─'.repeat(60) + '\n');

  // Get video duration
  const metadata = await new Promise((resolve, reject) => {
    ffmpeg.ffprobe(videoPath, (err, data) => {
      if (err) reject(err);
      else resolve(data);
    });
  });

  const duration = metadata.format.duration;
  const estimatedFrames = Math.floor(duration * fps);

  console.log(`📊 Video duration: ${duration.toFixed(1)}s`);
  console.log(`📊 Estimated frames: ~${estimatedFrames.toLocaleString()}`);
  console.log(`⏱️  Estimated time: ${(estimatedFrames / (fps * 3)).toFixed(0)} minutes at 3fps\n`);

  // Create output directory
  await fs.mkdir(outputDir, { recursive: true });

  // Initialize decoder
  const decoder = new MemoryDecoder();

  // Process ENTIRE video in memory
  await processVideoInMemory(videoPath, fps, decoder);

  // Show final results
  decoder.showProgress();

  console.log('\n' + '─'.repeat(60));
  console.log('\n📊 Processing Complete!\n');

  const elapsed = ((Date.now() - decoder.startTime) / 1000).toFixed(1);
  console.log(`   Frames processed: ${decoder.frameCount.toLocaleString()}`);
  console.log(`   QR codes detected: ${decoder.qrCodesDetected.toLocaleString()}`);
  console.log(`   Packets processed: ${decoder.packetsProcessed.toLocaleString()}`);
  console.log(`   Processing time: ${elapsed}s`);
  console.log(`   Average speed: ${(decoder.frameCount / elapsed).toFixed(0)} fps`);
  console.log(`   Files saved: ${decoder.downloadedFiles.size}\n`);

  console.log('📁 Final Results:\n');

  for (const [fileName, fileInfo] of decoder.discoveredFiles) {
    const icon = fileInfo.completed ? '✅' : '⏳';
    const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);
    const saved = decoder.downloadedFiles.has(fileName) ? '💾' : '';

    console.log(`${icon} ${saved} ${fileName}`);
    console.log(`   Progress: ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount} chunks (${progress}%)`);
    if (fileInfo.metadata.fileSize > 0) {
      console.log(`   Size: ${(fileInfo.metadata.fileSize / 1024).toFixed(1)} KB`);
    }
    console.log();
  }

  console.log(`📊 Summary: ${decoder.downloadedFiles.size} files saved to ${outputDir}`);
  console.log(`⏱️  Total time: ${elapsed}s\n`);
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node decoder-memory.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>     Frame extraction rate (default: 15)');
  console.log('  --output <dir>   Output directory (default: ./decoded)');
  console.log('\nMemory-only decoder - processes ENTIRE video without limits');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 15,
  output: './decoded'
};

// Parse options
for (let i = 3; i < process.argv.length; i += 2) {
  if (process.argv[i] === '--fps') {
    options.fps = parseFloat(process.argv[i + 1]);
  } else if (process.argv[i] === '--output') {
    options.output = process.argv[i + 1];
  }
}

decodeVideo(videoPath, options).catch(console.error);