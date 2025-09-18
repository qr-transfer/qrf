#!/usr/bin/env node
import { spawn } from 'child_process';
import ffmpeg from 'fluent-ffmpeg';
import Jimp from 'jimp';
import QrCode from 'qrcode-reader';
import fs from 'fs/promises';
import path from 'path';
import crypto from 'crypto';
import { performance } from 'perf_hooks';

// Exact logic from the HTML decoder with full validation
class ValidatedMultiFileDecoder {
  constructor() {
    this.discoveredFiles = new Map();
    this.processedChunks = new Map();
    this.currentVideoTime = 0;
    this.totalProcessedPackets = 0;
    this.duplicatePackets = 0;
    this.invalidPackets = 0;
    this.validPackets = 0;
    this.lastUpdateTime = Date.now();
    this.startTime = Date.now();
    this.frameCount = 0;
    this.qrDecoded = 0;
    this.qrErrors = 0;

    // Performance metrics
    this.performanceStats = {
      avgFps: 0,
      avgQrRate: 0,
      peakFps: 0,
      memoryUsage: 0
    };
  }

  // Calculate SHA-256 checksum
  calculateSHA256(data) {
    return crypto.createHash('sha256').update(data).digest('hex');
  }

  // Process QR data exactly like the original HTML decoder
  async processQRData(qrString, timestamp) {
    if (!qrString) return;

    try {
      // Parse metadata packet (M:)
      if (qrString.startsWith('M:')) {
        const parts = qrString.split(':');
        if (parts.length < 6) {
          this.invalidPackets++;
          return;
        }

        const metadata = {
          type: 'metadata',
          version: parts[1],
          fileName: decodeURIComponent(parts[2]),
          fileType: decodeURIComponent(parts[3]),
          fileSize: parseInt(parts[4]),
          chunksCount: parseInt(parts[5]),
          packetCount: parts[6] ? parseInt(parts[6]) : 0,
          degree: parts[7] ? parseInt(parts[7]) : 1,
          padding: parts[8] ? parseInt(parts[8]) : 0,
          minPaddingValue: parts[9] ? parseInt(parts[9]) : 0,
          maxPaddingValue: parts[10] ? parseInt(parts[10]) : 0,
          checksum: parts[13] || '',
          fileChecksum: parts[14] || '',
          encoderVersion: parts[15] || '3.0'
        };

        // Validate metadata
        if (!metadata.fileName || metadata.chunksCount <= 0 || metadata.fileSize < 0) {
          console.log(`⚠️  Invalid metadata packet at ${timestamp.toFixed(1)}s`);
          this.invalidPackets++;
          return;
        }

        // Calculate fileId from checksum
        const fileId = metadata.fileChecksum ?
          metadata.fileChecksum.substring(0, 8) :
          crypto.createHash('md5').update(metadata.fileName).digest('hex').substring(0, 8);

        if (!this.discoveredFiles.has(metadata.fileName)) {
          console.log(`\n✅ File discovered at ${timestamp.toFixed(1)}s: ${metadata.fileName}`);
          console.log(`   Version: ${metadata.version}`);
          console.log(`   Type: ${metadata.fileType}`);
          console.log(`   Size: ${(metadata.fileSize / 1024).toFixed(2)} KB`);
          console.log(`   Chunks: ${metadata.chunksCount}`);
          console.log(`   FileID: ${fileId}`);
          console.log(`   Checksum: ${metadata.fileChecksum ? metadata.fileChecksum.substring(0, 16) + '...' : 'none'}\n`);

          this.discoveredFiles.set(metadata.fileName, {
            metadata: metadata,
            fileId: fileId,
            chunks: new Map(),
            recoveredChunks: 0,
            validatedChunks: new Set(),
            firstSeen: timestamp,
            lastUpdate: timestamp,
            completed: false,
            completedTime: null,
            checksumValid: null,
            duplicateChunks: 0,
            invalidChunks: 0
          });
        }
        this.validPackets++;
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
        } else {
          this.invalidPackets++;
        }
      } else {
        this.invalidPackets++;
      }
    } catch (error) {
      console.error('QR parsing error:', error.message);
      this.qrErrors++;
    }

    this.qrDecoded++;
  }

  processDataPacket(packet, timestamp) {
    this.totalProcessedPackets++;

    // Validate packet
    if (!packet.data || packet.numChunks <= 0) {
      this.invalidPackets++;
      return;
    }

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
            fileType: 'application/octet-stream',
            fileChecksum: null
          },
          fileId: null,
          chunks: new Map(),
          recoveredChunks: 0,
          validatedChunks: new Set(),
          firstSeen: timestamp,
          lastUpdate: timestamp,
          completed: false,
          duplicateChunks: 0,
          invalidChunks: 0
        });
      }

      const fileInfo = this.discoveredFiles.get(unknownKey);
      this.addChunkToFile(unknownKey, fileInfo, packet, timestamp);
    }

    if (processed) {
      this.validPackets++;
    } else {
      this.invalidPackets++;
    }
  }

  addChunkToFile(fileName, fileInfo, packet, timestamp) {
    if (fileInfo.completed) return;

    // Determine chunk index
    const chunkIndex = packet.sourceIndex !== undefined ?
      packet.sourceIndex :
      (packet.packetId || 0) % fileInfo.metadata.chunksCount;

    // Validate chunk index
    if (chunkIndex < 0 || chunkIndex >= fileInfo.metadata.chunksCount) {
      fileInfo.invalidChunks++;
      this.invalidPackets++;
      return;
    }

    // Check if we already have this chunk
    if (fileInfo.chunks.has(chunkIndex)) {
      fileInfo.duplicateChunks++;
      this.duplicatePackets++;
      return;
    }

    // Decode base64 data
    try {
      const chunkData = Buffer.from(packet.data, 'base64');

      // Validate chunk size (basic check)
      if (chunkData.length === 0) {
        fileInfo.invalidChunks++;
        this.invalidPackets++;
        return;
      }

      fileInfo.chunks.set(chunkIndex, chunkData);
      fileInfo.recoveredChunks++;
      fileInfo.validatedChunks.add(chunkIndex);
      fileInfo.lastUpdate = timestamp;

      // Show detailed progress
      const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);
      const progressBar = this.createProgressBar(progress);

      if (fileInfo.recoveredChunks % 10 === 0 || fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
        console.log(`📦 ${fileName}: ${progressBar} ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount} (${progress}%)`);
        if (fileInfo.duplicateChunks > 0) {
          console.log(`   📊 Stats: ${fileInfo.duplicateChunks} duplicates, ${fileInfo.invalidChunks} invalid`);
        }
      }

      // Check if file is complete
      if (fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
        fileInfo.completed = true;
        fileInfo.completedTime = timestamp;
        console.log(`\n✅ File complete: ${fileName} at ${timestamp.toFixed(1)}s`);

        // Immediately verify and save
        this.verifyAndSaveFile(fileName, fileInfo);
      }
    } catch (error) {
      console.error(`Failed to decode chunk: ${error.message}`);
      fileInfo.invalidChunks++;
      this.invalidPackets++;
    }
  }

  createProgressBar(percentage) {
    const width = 20;
    const filled = Math.floor((percentage / 100) * width);
    const empty = width - filled;
    return '[' + '█'.repeat(filled) + '░'.repeat(empty) + ']';
  }

  async verifyAndSaveFile(fileName, fileInfo) {
    try {
      // Combine chunks in order
      const chunks = [];
      let missingChunks = [];

      for (let i = 0; i < fileInfo.metadata.chunksCount; i++) {
        if (fileInfo.chunks.has(i)) {
          chunks.push(fileInfo.chunks.get(i));
        } else {
          missingChunks.push(i);
        }
      }

      if (missingChunks.length > 0) {
        console.log(`⚠️  Missing chunks for ${fileName}: ${missingChunks.join(', ')}`);
        return null;
      }

      const fileData = Buffer.concat(chunks);

      // VERIFY CHECKSUM
      if (fileInfo.metadata.fileChecksum) {
        console.log(`🔍 Verifying checksum for ${fileName}...`);
        const calculatedChecksum = this.calculateSHA256(fileData);

        if (calculatedChecksum === fileInfo.metadata.fileChecksum) {
          fileInfo.checksumValid = true;
          console.log(`✅ Checksum VALID: ${calculatedChecksum.substring(0, 16)}...`);
        } else {
          fileInfo.checksumValid = false;
          console.log(`❌ Checksum MISMATCH!`);
          console.log(`   Expected: ${fileInfo.metadata.fileChecksum.substring(0, 32)}...`);
          console.log(`   Got:      ${calculatedChecksum.substring(0, 32)}...`);
        }
      } else {
        console.log(`⚠️  No checksum to verify for ${fileName}`);
      }

      return fileData;
    } catch (error) {
      console.error(`Failed to verify ${fileName}: ${error.message}`);
      return null;
    }
  }

  async saveCompletedFiles(outputDir) {
    const results = [];

    for (const [fileName, fileInfo] of this.discoveredFiles) {
      if (!fileInfo.completed || fileName.startsWith('unknown_')) continue;

      try {
        const fileData = await this.verifyAndSaveFile(fileName, fileInfo);

        if (fileData) {
          const outputPath = path.join(outputDir, fileName);
          await fs.writeFile(outputPath, fileData);

          console.log(`\n💾 SAVED: ${outputPath}`);
          console.log(`   Size: ${(fileData.length / 1024).toFixed(2)} KB`);
          console.log(`   Checksum: ${fileInfo.checksumValid === true ? '✅ VALID' : fileInfo.checksumValid === false ? '❌ INVALID' : '⚠️  NOT VERIFIED'}`);
          console.log(`   Recovery time: ${(fileInfo.completedTime - fileInfo.firstSeen).toFixed(1)}s`);

          results.push({
            fileName,
            success: true,
            size: fileData.length,
            checksumValid: fileInfo.checksumValid
          });
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
        completedTime: fileInfo.completedTime,
        checksumValid: fileInfo.checksumValid,
        duplicates: fileInfo.duplicateChunks,
        invalid: fileInfo.invalidChunks
      });
    }
    return {
      files,
      totalFiles: this.discoveredFiles.size,
      totalPackets: this.totalProcessedPackets,
      validPackets: this.validPackets,
      invalidPackets: this.invalidPackets,
      duplicates: this.duplicatePackets,
      qrDecoded: this.qrDecoded,
      qrErrors: this.qrErrors,
      frameCount: this.frameCount
    };
  }

  showProgress() {
    const elapsed = (Date.now() - this.startTime) / 1000;
    const fps = this.frameCount / elapsed;
    const qrRate = this.qrDecoded / elapsed;

    // Update performance stats
    this.performanceStats.avgFps = fps;
    this.performanceStats.avgQrRate = qrRate;
    if (fps > this.performanceStats.peakFps) {
      this.performanceStats.peakFps = fps;
    }

    let completedCount = 0;
    let totalChunks = 0;
    let recoveredChunks = 0;

    for (const fileInfo of this.discoveredFiles.values()) {
      if (fileInfo.completed) completedCount++;
      totalChunks += fileInfo.metadata.chunksCount;
      recoveredChunks += fileInfo.recoveredChunks;
    }

    const overallProgress = totalChunks > 0 ? Math.round((recoveredChunks / totalChunks) * 100) : 0;
    const memUsage = process.memoryUsage();
    const memMB = (memUsage.heapUsed / 1024 / 1024).toFixed(1);

    process.stdout.write(`\r⚡ F:${this.frameCount} | QR:${this.qrDecoded} | Files:${this.discoveredFiles.size}(${completedCount}✓) | Chunks:${recoveredChunks}/${totalChunks}(${overallProgress}%) | ${fps.toFixed(0)}fps | ${memMB}MB`);
  }

  showFinalReport() {
    console.log('\n' + '═'.repeat(70));
    console.log('📊 FINAL DECODING REPORT');
    console.log('═'.repeat(70) + '\n');

    const elapsed = (Date.now() - this.startTime) / 1000;
    const status = this.getStatus();

    console.log('📈 PERFORMANCE METRICS:');
    console.log(`   • Total time: ${elapsed.toFixed(1)}s`);
    console.log(`   • Frames processed: ${status.frameCount}`);
    console.log(`   • Average speed: ${this.performanceStats.avgFps.toFixed(1)} fps`);
    console.log(`   • Peak speed: ${this.performanceStats.peakFps.toFixed(1)} fps`);
    console.log(`   • QR decode rate: ${this.performanceStats.avgQrRate.toFixed(1)}/s\n`);

    console.log('📦 PACKET STATISTICS:');
    console.log(`   • QR codes found: ${status.qrDecoded}`);
    console.log(`   • QR decode errors: ${status.qrErrors}`);
    console.log(`   • Valid packets: ${status.validPackets}`);
    console.log(`   • Invalid packets: ${status.invalidPackets}`);
    console.log(`   • Duplicate packets: ${status.duplicates}\n`);

    console.log('📁 FILES RECOVERED:');
    let successCount = 0;
    let checksumValidCount = 0;

    for (const file of status.files) {
      const icon = file.completed ? '✅' : '⏳';
      const checksumIcon = file.checksumValid === true ? '🔒' : file.checksumValid === false ? '⚠️ ' : '❓';

      console.log(`\n${icon} ${file.fileName}`);
      console.log(`   Progress: ${file.chunks}/${file.total} chunks (${file.progress}%)`);

      if (file.completed) {
        console.log(`   Checksum: ${checksumIcon} ${file.checksumValid === true ? 'VERIFIED' : file.checksumValid === false ? 'MISMATCH' : 'NO CHECKSUM'}`);
        console.log(`   Completed at: ${file.completedTime.toFixed(1)}s`);
        successCount++;
        if (file.checksumValid === true) checksumValidCount++;
      }

      if (file.duplicates > 0 || file.invalid > 0) {
        console.log(`   Issues: ${file.duplicates} duplicates, ${file.invalid} invalid chunks`);
      }
    }

    console.log('\n' + '─'.repeat(70));
    console.log(`📊 SUMMARY: ${successCount}/${status.files.length} files recovered`);
    console.log(`🔒 CHECKSUMS: ${checksumValidCount}/${successCount} verified successfully`);
    console.log('═'.repeat(70) + '\n');
  }
}

// Fast video processor with progress
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
        decoder.qrErrors++;
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

  console.log('\n🎬 QRF Multi-File Decoder with Checksum Validation\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`⚡ Scan rate: ${fps} FPS`);
  console.log(`📁 Output: ${outputDir}`);
  console.log(`✅ Full validation & checksum verification enabled\n`);
  console.log('═'.repeat(70) + '\n');

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
  const decoder = new ValidatedMultiFileDecoder();

  console.log('🔥 Processing video - capturing ALL files with validation...\n');

  // Process video
  await processVideo(videoPath, fps, decoder);

  // Save completed files
  const status = decoder.getStatus();
  const completedFiles = status.files.filter(f => f.completed);

  if (completedFiles.length > 0) {
    console.log('\n💾 Saving completed files...\n');
    await decoder.saveCompletedFiles(outputDir);
  }

  // Show final report
  decoder.showFinalReport();
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node decoder-multi-validated.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>     Frame extraction rate (default: 10)');
  console.log('  --output <dir>   Output directory (default: ./decoded)');
  console.log('\nFeatures:');
  console.log('  • Captures ALL files in the video');
  console.log('  • Shows detailed progress bars');
  console.log('  • Verifies SHA-256 checksums');
  console.log('  • Validates all packets');
  console.log('  • Handles multiple files with same chunk count');
  console.log('  • Shows comprehensive statistics');
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