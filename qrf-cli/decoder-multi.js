#!/usr/bin/env node
import { spawn } from 'child_process';
import ffmpeg from 'fluent-ffmpeg';
import Jimp from 'jimp';
import QrCode from 'qrcode-reader';
import fs from 'fs/promises';
import path from 'path';
import crypto from 'crypto';

// Exact logic from the HTML decoder
class MultiFileDecoder {
  constructor() {
    this.discoveredFiles = new Map();
    this.processedChunks = new Map();
    this.currentVideoTime = 0;
    this.totalProcessedPackets = 0;
    this.duplicatePackets = 0;
    this.lastUpdateTime = Date.now();
    this.startTime = Date.now();
    this.frameCount = 0;
    this.qrDecoded = 0;
  }

  // Process QR data exactly like the original HTML decoder
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
          console.log(`\n✅ File discovered at ${timestamp.toFixed(1)}s: ${metadata.fileName}`);
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

        // Handle different formats
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

    // Method 2: Match by chunk count - can match multiple files!
    if (!processed && packet.numChunks) {
      for (const [fileName, fileInfo] of this.discoveredFiles) {
        if (fileInfo.metadata.chunksCount === packet.numChunks) {
          this.addChunkToFile(fileName, fileInfo, packet, timestamp);
          processed = true;
          // Don't break - packet might belong to multiple files with same chunk count
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

      // Show progress
      const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);

      if (fileInfo.recoveredChunks % 10 === 0 || fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
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

  getStatus() {
    const files = [];
    for (const [fileName, fileInfo] of this.discoveredFiles) {
      files.push({
        fileName,
        chunks: fileInfo.recoveredChunks,
        total: fileInfo.metadata.chunksCount,
        progress: Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100),
        completed: fileInfo.completed,
        completedTime: fileInfo.completedTime
      });
    }
    return {
      files,
      totalFiles: this.discoveredFiles.size,
      totalPackets: this.totalProcessedPackets,
      duplicates: this.duplicatePackets,
      qrDecoded: this.qrDecoded,
      frameCount: this.frameCount
    };
  }

  showProgress() {
    const elapsed = (Date.now() - this.startTime) / 1000;
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

    process.stdout.write(`\r⚡ Frames: ${this.frameCount} | QR: ${this.qrDecoded} | Files: ${this.discoveredFiles.size} (${completedCount} done) | Chunks: ${recoveredChunks}/${totalChunks} | Speed: ${fps.toFixed(0)} fps`);
  }
}

// Fast video processor
async function processVideo(videoPath, fps, decoder) {
  return new Promise((resolve, reject) => {
    const qr = new QrCode();
    let frameId = 0;

    // FFmpeg args for fast processing
    const args = [
      '-i', videoPath,
      '-threads', '0',
      '-vf', `fps=${fps}`,
      '-c:v', 'mjpeg',
      '-q:v', '2',
      '-f', 'image2pipe',
      '-'
    ];

    const ffmpegProcess = spawn('ffmpeg', args, {
      stdio: ['ignore', 'pipe', 'ignore']
    });

    let buffer = Buffer.alloc(0);
    const frameQueue = [];
    let isProcessing = false;

    const processFrame = async (frameData, id) => {
      try {
        const image = await Jimp.read(frameData);
        const prepared = image.greyscale().contrast(0.3).brightness(0.1);

        await new Promise((resolve) => {
          qr.callback = async (err, value) => {
            if (!err && value && value.result) {
              await decoder.processQRData(value.result, id / fps);
            }
            resolve();
          };
          qr.decode(prepared.bitmap);
        });
      } catch (error) {
        // Ignore decode errors
      }
    };

    const processBatch = async () => {
      if (isProcessing || frameQueue.length === 0) return;
      isProcessing = true;

      const batch = frameQueue.splice(0, 5);
      await Promise.all(batch.map(({ frame, id }) => processFrame(frame, id)));

      decoder.frameCount = frameId;
      if (frameId % 10 === 0) {
        decoder.showProgress();
      }

      isProcessing = false;
      if (frameQueue.length > 0) {
        setImmediate(processBatch);
      }
    };

    ffmpegProcess.stdout.on('data', (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);

      let frameStart = 0;
      while (true) {
        const jpegStart = buffer.indexOf(Buffer.from([0xFF, 0xD8]), frameStart);
        if (jpegStart === -1) break;

        const jpegEnd = buffer.indexOf(Buffer.from([0xFF, 0xD9]), jpegStart + 2);
        if (jpegEnd === -1) break;

        const frame = buffer.slice(jpegStart, jpegEnd + 2);
        frameQueue.push({ frame, id: frameId++ });

        if (!isProcessing) {
          setImmediate(processBatch);
        }

        frameStart = jpegEnd + 2;
      }

      if (frameStart > 0 && frameStart < buffer.length) {
        buffer = buffer.slice(frameStart);
      } else if (frameStart >= buffer.length) {
        buffer = Buffer.alloc(0);
      }
    });

    ffmpegProcess.on('close', async (code) => {
      while (frameQueue.length > 0 || isProcessing) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }

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
  const fps = options.fps || 10;
  const outputDir = options.output || './decoded';

  console.log('\n🎬 QRF Multi-File Decoder (Original Logic)\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`⚡ Scan rate: ${fps} FPS`);
  console.log(`📁 Output: ${outputDir}`);
  console.log(`✅ Capturing ALL files in video\n`);
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
  const decoder = new MultiFileDecoder();

  console.log('🔥 Processing video - capturing ALL files...\n');

  // Process video
  await processVideo(videoPath, fps, decoder);

  // Show final status
  const status = decoder.getStatus();

  console.log('\n' + '─'.repeat(60));
  console.log('\n📊 Scan Complete!\n');
  console.log(`   Frames processed: ${status.frameCount}`);
  console.log(`   QR codes decoded: ${status.qrDecoded}`);
  console.log(`   Total packets: ${status.totalPackets}`);
  console.log(`   Duplicate packets: ${status.duplicates}\n`);

  console.log('📁 Discovered Files:\n');

  let completedCount = 0;
  for (const file of status.files) {
    const icon = file.completed ? '✅' : '⏳';
    console.log(`${icon} ${file.fileName}`);
    console.log(`   Progress: ${file.chunks}/${file.total} chunks (${file.progress}%)`);
    if (file.completed) {
      console.log(`   Completed at: ${file.completedTime.toFixed(1)}s`);
      completedCount++;
    }
    console.log();
  }

  console.log(`📊 Summary: ${completedCount}/${status.files.length} files completed\n`);

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

  const elapsed = ((Date.now() - decoder.startTime) / 1000).toFixed(1);
  console.log(`\n⏱️  Total time: ${elapsed}s`);
  console.log(`📈 Average speed: ${(status.frameCount / elapsed).toFixed(0)} fps\n`);
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node decoder-multi.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>     Frame extraction rate (default: 10)');
  console.log('  --output <dir>   Output directory (default: ./decoded)');
  console.log('\nThis decoder uses the EXACT logic from the original HTML decoder');
  console.log('and will capture ALL files in the video, handling multiple files');
  console.log('with the same chunk count correctly.');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 10,
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