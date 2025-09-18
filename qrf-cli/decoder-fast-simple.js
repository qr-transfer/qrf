#!/usr/bin/env node
import { spawn } from 'child_process';
import Jimp from 'jimp';
import QrCode from 'qrcode-reader';
import fs from 'fs/promises';
import path from 'path';
import crypto from 'crypto';
import ffmpeg from 'fluent-ffmpeg';

class FastSimpleDecoder {
  constructor() {
    this.discoveredFiles = new Map();
    this.processedChunks = new Map();
    this.totalProcessedPackets = 0;
    this.duplicatePackets = 0;
    this.startTime = Date.now();
    this.frameCount = 0;
    this.qrDecoded = 0;
    this.lastProgressUpdate = Date.now();
  }

  async processQRData(qrString, timestamp) {
    if (!qrString) return;

    try {
      // Parse metadata packet (M:)
      if (qrString.startsWith('M:')) {
        const parts = qrString.split(':');
        if (parts.length < 6) return;

        const metadata = {
          type: 'metadata',
          version: parts[1],
          fileName: decodeURIComponent(parts[2]),
          fileType: decodeURIComponent(parts[3]),
          fileSize: parseInt(parts[4]),
          chunksCount: parseInt(parts[5]),
          packetCount: parts[6] ? parseInt(parts[6]) : 0,
          checksum: parts[13] || '',
          fileChecksum: parts[14] || '',
          encoderVersion: parts[15] || '3.0'
        };

        // Calculate fileId from checksum
        const fileId = metadata.fileChecksum ?
          metadata.fileChecksum.substring(0, 8) :
          crypto.createHash('md5').update(metadata.fileName).digest('hex').substring(0, 8);

        if (!this.discoveredFiles.has(metadata.fileName)) {
          console.log(`\n✅ METADATA FOUND at ${timestamp.toFixed(1)}s: ${metadata.fileName}`);
          console.log(`   Type: ${metadata.fileType}`);
          console.log(`   Size: ${(metadata.fileSize / 1024).toFixed(1)} KB`);
          console.log(`   Chunks: ${metadata.chunksCount}`);
          console.log(`   FileID: ${fileId}\n`);

          this.discoveredFiles.set(metadata.fileName, {
            metadata: metadata,
            fileId: fileId,
            chunks: new Map(),
            recoveredChunks: 0,
            firstSeen: timestamp,
            lastUpdate: timestamp,
            completed: false,
            completedTime: null
          });
        }
      }
      // Parse data packet (D:)
      else if (qrString.startsWith('D:')) {
        const parts = qrString.split(':');

        let packet = null;

        // Format 1: D:fileId:packetId:... (with 8-char hex fileId)
        if (parts[1] && parts[1].length === 8 && /^[a-fA-F0-9]{8}$/.test(parts[1])) {
          packet = {
            fileId: parts[1],
            packetId: parseInt(parts[2]),
            seed: parseInt(parts[3]),
            seedBase: parseInt(parts[4]),
            numChunks: parseInt(parts[5]),
            degree: parseInt(parts[6]),
            data: parts.slice(7).join(':')
          };
        }
        // Format 2: D:packetId:timestamp1:timestamp2:numChunks:degree:index:data
        else if (parts.length >= 7) {
          packet = {
            packetId: parseInt(parts[1]),
            numChunks: parseInt(parts[4]),
            degree: parseInt(parts[5]),
            sourceIndex: parseInt(parts[6]),
            data: parts.slice(7).join(':')
          };
        }

        if (packet && packet.data) {
          this.processDataPacket(packet, timestamp);
        }
      }
    } catch (error) {
      console.error('QR parsing error:', error.message);
    }

    this.qrDecoded++;
  }

  processDataPacket(packet, timestamp) {
    this.totalProcessedPackets++;

    // Find target file(s)
    let processed = false;

    // Method 1: Direct fileId match
    if (packet.fileId) {
      for (const [fileName, fileInfo] of this.discoveredFiles) {
        if (fileInfo.fileId === packet.fileId) {
          this.addChunkToFile(fileName, fileInfo, packet, timestamp);
          processed = true;
          break;
        }
      }
    }

    // Method 2: Match by chunk count
    if (!processed && packet.numChunks) {
      for (const [fileName, fileInfo] of this.discoveredFiles) {
        if (fileInfo.metadata.chunksCount === packet.numChunks) {
          this.addChunkToFile(fileName, fileInfo, packet, timestamp);
          processed = true;
        }
      }
    }

    // Method 3: Store for unknown files
    if (!processed && packet.numChunks) {
      const unknownKey = `unknown_${packet.numChunks}chunks`;
      if (!this.discoveredFiles.has(unknownKey)) {
        console.log(`\n⚠️  Data packets found for unknown file (${packet.numChunks} chunks)`);

        this.discoveredFiles.set(unknownKey, {
          metadata: {
            fileName: unknownKey,
            fileSize: 0,
            chunksCount: packet.numChunks,
            fileType: 'application/octet-stream'
          },
          fileId: null,
          chunks: new Map(),
          recoveredChunks: 0,
          firstSeen: timestamp,
          lastUpdate: timestamp,
          completed: false
        });
      }

      const fileInfo = this.discoveredFiles.get(unknownKey);
      this.addChunkToFile(unknownKey, fileInfo, packet, timestamp);
    }
  }

  addChunkToFile(fileName, fileInfo, packet, timestamp) {
    if (fileInfo.completed) return;

    // Determine chunk index
    const chunkIndex = packet.sourceIndex !== undefined ?
      packet.sourceIndex :
      (packet.packetId || 0) % fileInfo.metadata.chunksCount;

    // Check if we already have this chunk
    if (fileInfo.chunks.has(chunkIndex)) {
      this.duplicatePackets++;
      return;
    }

    // Decode base64 data
    try {
      const chunkData = Buffer.from(packet.data, 'base64');

      fileInfo.chunks.set(chunkIndex, chunkData);
      fileInfo.recoveredChunks++;
      fileInfo.lastUpdate = timestamp;

      // Show progress every 10 chunks
      if (fileInfo.recoveredChunks % 10 === 0 || fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
        const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);
        console.log(`📦 ${fileName}: ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount} chunks (${progress}%)`);
      }

      // Check if file is complete
      if (fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
        fileInfo.completed = true;
        fileInfo.completedTime = timestamp;
        console.log(`\n✅ File complete: ${fileName} at ${timestamp.toFixed(1)}s\n`);
      }
    } catch (error) {
      console.error(`Failed to decode chunk: ${error.message}`);
    }
  }

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
            continue;
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
    const now = Date.now();
    if (now - this.lastProgressUpdate < 200) return; // Update every 200ms max

    const elapsed = (now - this.startTime) / 1000;
    const fps = this.frameCount / elapsed;
    const qrRate = this.qrDecoded / elapsed;

    let completedCount = 0;
    let totalChunks = 0;
    let recoveredChunks = 0;

    for (const fileInfo of this.discoveredFiles.values()) {
      if (fileInfo.completed) completedCount++;
      totalChunks += fileInfo.metadata.chunksCount;
      recoveredChunks += fileInfo.recoveredChunks;
    }

    process.stdout.write(`\r⚡ Frames: ${this.frameCount} | QR: ${this.qrDecoded} | Files: ${this.discoveredFiles.size} (${completedCount} done) | Chunks: ${recoveredChunks}/${totalChunks} | Speed: ${fps.toFixed(0)} fps | QR/s: ${qrRate.toFixed(0)}`);

    this.lastProgressUpdate = now;
  }
}

// Extremely optimized video processor - no batching, minimal processing
async function processVideoMaxSpeed(videoPath, fps, decoder) {
  return new Promise((resolve, reject) => {
    const qr = new QrCode();
    let frameId = 0;

    // Minimal FFmpeg args for absolute maximum speed
    const args = [
      '-i', videoPath,
      '-threads', '0',
      '-vf', `fps=${fps}`,
      '-c:v', 'mjpeg',
      '-q:v', '4', // Balance between speed and quality
      '-f', 'image2pipe',
      '-'
    ];

    const ffmpegProcess = spawn('ffmpeg', args, {
      stdio: ['ignore', 'pipe', 'ignore']
    });

    let buffer = Buffer.alloc(0);

    // Process frame immediately without any batching or queuing
    const processFrameImmediate = async (frameData, id) => {
      try {
        // Only use Jimp without any preprocessing for maximum speed
        const image = await Jimp.read(frameData);

        await new Promise((resolve) => {
          qr.callback = async (err, value) => {
            if (!err && value && value.result) {
              await decoder.processQRData(value.result, id / fps);
            }
            resolve();
          };
          qr.decode(image.bitmap);
        });
      } catch (error) {
        // Ignore decode errors silently for maximum speed
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

        // Process frame immediately - no queuing at all
        await processFrameImmediate(frame, frameId);

        decoder.frameCount = frameId++;

        // Update progress less frequently for speed
        if (frameId % 50 === 0) {
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
  const fps = options.fps || 20; // Higher default for faster scanning
  const outputDir = options.output || './decoded';

  console.log('\n🎬 QRF Fast Simple Decoder\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`⚡ Scan rate: ${fps} FPS (MAXIMUM SPEED)`);
  console.log(`📁 Output: ${outputDir}`);
  console.log(`✅ Optimized for absolute maximum speed\n`);
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
  const decoder = new FastSimpleDecoder();

  console.log('🔥 Processing video at MAXIMUM speed - no batching, no delays...\n');

  // Process video
  await processVideoMaxSpeed(videoPath, fps, decoder);

  // Final progress update
  decoder.showProgress();

  console.log('\n' + '─'.repeat(60));
  console.log('\n📊 Scan Complete!\n');

  const elapsed = ((Date.now() - decoder.startTime) / 1000).toFixed(1);
  console.log(`   Frames processed: ${decoder.frameCount}`);
  console.log(`   QR codes decoded: ${decoder.qrDecoded}`);
  console.log(`   Total packets: ${decoder.totalProcessedPackets}`);
  console.log(`   Duplicate packets: ${decoder.duplicatePackets}`);
  console.log(`   Processing time: ${elapsed}s`);
  console.log(`   Average speed: ${(decoder.frameCount / elapsed).toFixed(0)} fps\n`);

  console.log('📁 Discovered Files:\n');

  let completedCount = 0;
  for (const [fileName, fileInfo] of decoder.discoveredFiles) {
    const icon = fileInfo.completed ? '✅' : '⏳';
    const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);
    console.log(`${icon} ${fileName}`);
    console.log(`   Progress: ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount} chunks (${progress}%)`);
    if (fileInfo.completed) {
      console.log(`   Completed at: ${fileInfo.completedTime.toFixed(1)}s`);
      completedCount++;
    }
    console.log();
  }

  console.log(`📊 Summary: ${completedCount}/${decoder.discoveredFiles.size} files completed\n`);

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

  console.log(`\n⏱️  Total time: ${elapsed}s`);
  console.log(`📈 Average speed: ${(decoder.frameCount / elapsed).toFixed(0)} fps\n`);
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node decoder-fast-simple.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>     Frame extraction rate (default: 20)');
  console.log('  --output <dir>   Output directory (default: ./decoded)');
  console.log('\nOptimized for MAXIMUM SPEED - no batching, no delays');
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