#!/usr/bin/env node
import { spawn } from 'child_process';
import jsQR from 'jsqr';
import sharp from 'sharp';
import fs from 'fs/promises';
import path from 'path';
import crypto from 'crypto';
import ffmpeg from 'fluent-ffmpeg';

class CorrectedDecoder {
  constructor() {
    // Match HTML structure exactly
    this.discoveredFiles = new Map(); // Map of filename -> {metadata, firstSeenAt, lastSeenAt, completed, recoveredData}
    this.currentActiveFile = null;
    this.downloadedFiles = new Set(); // Track already downloaded files to prevent duplicates
    this.qrCodesDetected = 0;
    this.packetsProcessed = 0;
    this.startTime = Date.now();
    this.frameCount = 0;

    // Initialize packet processor (matches HTML)
    this.packetProcessor = new PacketProcessor();
  }

  // Safe URI decode like HTML version
  decodeURIComponentSafe(str) {
    try {
      return decodeURIComponent(str);
    } catch (e) {
      console.warn(`Failed to decode URI component: ${str}`);
      return str;
    }
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

    // Handle file discovery (matches HTML logic)
    const isNewFileDiscovered = !this.discoveredFiles.has(fileName);

    if (isNewFileDiscovered) {
      // Completely new file discovered
      this.discoveredFiles.set(fileName, {
        metadata: metadata,
        firstSeenAt: currentTime,
        lastSeenAt: currentTime,
        completed: false,
        recoveredData: null
      });

      console.log(`\n✅ METADATA FOUND at ${currentTime.toFixed(1)}s: ${fileName}`);
      console.log(`   Type: ${metadata.fileType}`);
      console.log(`   Size: ${(metadata.fileSize / 1024).toFixed(1)} KB`);
      console.log(`   Chunks: ${metadata.chunksCount}`);

      // Calculate fileId like HTML
      const fileId = metadata.fileChecksum ?
        metadata.fileChecksum.substring(0, 8) :
        crypto.createHash('md5').update(metadata.fileName).digest('hex').substring(0, 8);
      console.log(`   FileID: ${fileId}\n`);

      // Switch to processing this file
      this.switchToFile(metadata, fileName);
    } else {
      // Existing file - update timing
      const fileInfo = this.discoveredFiles.get(fileName);
      fileInfo.lastSeenAt = currentTime;
    }
  }

  // Handle data packet (simplified from HTML)
  handleDataPacket(packetData, timestamp) {
    // For now, just track that we received data packets
    // In full implementation, this would feed into fountain decoder
    console.log(`📦 Data packet received: ${JSON.stringify(packetData).substring(0, 100)}...`);
  }

  // Switch to processing a file (from HTML)
  switchToFile(metadata, fileName) {
    this.currentActiveFile = fileName;
    console.log(`🔄 Switched to processing file: ${fileName}`);

    // In full implementation, this would initialize fountain decoder
    // and start tracking chunks for this specific file
  }

  // Get status summary
  getStatus() {
    const files = [];
    for (const [fileName, fileInfo] of this.discoveredFiles) {
      files.push({
        fileName,
        completed: fileInfo.completed,
        firstSeen: fileInfo.firstSeenAt,
        lastSeen: fileInfo.lastSeenAt,
        size: fileInfo.metadata.fileSize
      });
    }

    return {
      files,
      totalFiles: this.discoveredFiles.size,
      qrCodes: this.qrCodesDetected,
      packets: this.packetsProcessed,
      frameCount: this.frameCount
    };
  }

  showProgress() {
    const elapsed = (Date.now() - this.startTime) / 1000;
    const fps = this.frameCount / elapsed;

    process.stdout.write(`\r⚡ Frames: ${this.frameCount} | QRs: ${this.qrCodesDetected} | Files: ${this.discoveredFiles.size} | Speed: ${fps.toFixed(0)} fps`);
  }
}

// Packet processor that matches HTML exactly
class PacketProcessor {
  constructor() {
    // No initialization needed for packet processing
  }

  processQRData(qrData, frameIndex) {
    try {
      // Detect packet type
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
    // Format: M:<version>:<filename>:<filetype>:<filesize>:<chunks>:<packets>:<maxdegree>:<density>:<fps>:<chunksize>:<redund>:<ecl>:<checksum>:<fileChecksum>:<encoderVersion>:<ltparams>
    const parts = metaString.split(':');

    if (parts.length < 10) {
      throw new Error(`Invalid metadata packet format: ${metaString.substring(0, 30)}...`);
    }

    let fileSize = -1;
    try {
      fileSize = parseInt(parts[4]);
    } catch (e) {
      console.error(`Invalid file size: ${parts[4]}`);
    }

    // Extract metadata (matches HTML format exactly)
    const metadata = {
      protocolVersion: parts[1],
      fileName: this.decodeURIComponentSafe(parts[2]),
      fileType: this.decodeURIComponentSafe(parts[3]),
      fileSize: fileSize,
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

    // Validate metadata (matches HTML)
    if (isNaN(metadata.fileSize) || metadata.fileSize <= 0) {
      metadata.fileSize = -1;
      console.error(`Invalid file size: ${parts[4]}`);
    }

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
    // Match HTML auto-detection logic exactly
    const parts = dataString.split(':');

    // Auto-detect packet format based on length and content
    let fileId, packetId, seed, seedBase, numChunks, chunkCount;
    let isNewFormat = false;

    if (parts.length >= 8 && parts[1] && parts[1].length === 8 && /^[a-fA-F0-9]{8}$/.test(parts[1])) {
      // New format with 8-char hex fileId: D:<fileId>:<packetId>:<seed>:<seedBase>:<numChunks>:<degree>:<data>
      isNewFormat = true;
      fileId = parts[1];
      packetId = parseInt(parts[2]);
      seed = parseInt(parts[3]);
      seedBase = parseInt(parts[4]);
      numChunks = parseInt(parts[5]);
      chunkCount = parseInt(parts[6]);

      // Reduced logging for new format (matches HTML)
      if (packetId % 50 === 1 || packetId <= 5) {
        console.log(`✅ NEW FORMAT DETECTED: fileId=${fileId}, packetId=${packetId}, degree=${chunkCount}`);
      }
    } else if (parts.length >= 7) {
      // Legacy format: D:<packetId>:<seed>:<seedBase>:<numChunks>:<degree>:<data>
      isNewFormat = false;
      fileId = null;
      packetId = parseInt(parts[1]);
      seed = parseInt(parts[2]);
      seedBase = parseInt(parts[3]);
      numChunks = parseInt(parts[4]);
      chunkCount = parseInt(parts[5]);

      console.log(`🔄 LEGACY FORMAT: packetId=${packetId}, degree=${chunkCount}`);
    } else {
      throw new Error(`Invalid data packet format: ${dataString.substring(0, 50)}...`);
    }

    // Validate packet data
    if (isNaN(packetId) || isNaN(numChunks) || isNaN(chunkCount)) {
      throw new Error(`Invalid packet parameters in: ${dataString.substring(0, 50)}...`);
    }

    const packet = {
      isNewFormat,
      fileId,
      packetId,
      seed,
      seedBase,
      numChunks,
      degree: chunkCount,
      data: parts.slice(isNewFormat ? 7 : 6).join(':')
    };

    return {
      success: true,
      packetType: 'data',
      packetData: packet,
      frameIndex
    };
  }

  // Safe URI decode helper (matches HTML)
  decodeURIComponentSafe(str) {
    try {
      return decodeURIComponent(str);
    } catch (e) {
      console.warn(`Failed to decode URI component: ${str}`);
      return str;
    }
  }
}

// Video processing with rotation correction
async function processVideoWithCorrectLogic(videoPath, fps, decoder) {
  return new Promise((resolve, reject) => {
    let frameId = 0;

    // FFmpeg with 180-degree rotation correction
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

    // Process frame with QR detection
    const processFrame = async (frameData, id) => {
      try {
        // Use Sharp + jsQR for QR detection
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
        // Ignore decode errors silently
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

        // Process frame immediately
        await processFrame(frame, frameId);

        decoder.frameCount = frameId++;

        // Update progress every 20 frames
        if (frameId % 20 === 0) {
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

// Main function
async function decodeVideo(videoPath, options) {
  const fps = options.fps || 20;
  const outputDir = options.output || './decoded';

  console.log('\n🎬 QRF Corrected Decoder (HTML Logic)\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`⚡ Scan rate: ${fps} FPS`);
  console.log(`📁 Output: ${outputDir}`);
  console.log(`✅ Using exact HTML decoder logic\n`);
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
  console.log(`📊 Expected frames: ~${Math.floor(duration * fps)} at ${fps} FPS\n`);

  // Create output directory
  await fs.mkdir(outputDir, { recursive: true });

  // Initialize decoder
  const decoder = new CorrectedDecoder();

  console.log('🔥 Processing video with corrected HTML logic...\n');

  // Process video
  await processVideoWithCorrectLogic(videoPath, fps, decoder);

  // Show final results
  decoder.showProgress();

  const status = decoder.getStatus();
  console.log('\n' + '─'.repeat(60));
  console.log('\n📊 Scan Complete!\n');

  const elapsed = ((Date.now() - decoder.startTime) / 1000).toFixed(1);
  console.log(`   Frames processed: ${status.frameCount}`);
  console.log(`   QR codes detected: ${status.qrCodes}`);
  console.log(`   Packets processed: ${status.packets}`);
  console.log(`   Processing time: ${elapsed}s`);
  console.log(`   Average speed: ${(status.frameCount / elapsed).toFixed(0)} fps\n`);

  console.log('📁 Discovered Files:\n');

  for (const file of status.files) {
    const icon = file.completed ? '✅' : '📄';
    console.log(`${icon} ${file.fileName}`);
    console.log(`   Size: ${(file.size / 1024).toFixed(1)} KB`);
    console.log(`   Seen: ${file.firstSeen.toFixed(1)}s - ${file.lastSeen.toFixed(1)}s`);
    console.log();
  }

  console.log(`📊 Summary: Found ${status.totalFiles} files\n`);
  console.log(`⏱️  Total time: ${elapsed}s`);
  console.log(`📈 Average speed: ${(status.frameCount / elapsed).toFixed(0)} fps\n`);
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node decoder-corrected.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>     Frame extraction rate (default: 20)');
  console.log('  --output <dir>   Output directory (default: ./decoded)');
  console.log('\nCorrected decoder using exact HTML logic');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 20,
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