#!/usr/bin/env node
import { spawn } from 'child_process';
import { QRDecoder } from './src/qrDecoder.js';
import { FountainDecoder } from './src/fountainDecoder.js';
import fs from 'fs/promises';
import path from 'path';
import ffmpeg from 'fluent-ffmpeg';
import crypto from 'crypto';

class ValidatedFileDecoder {
  constructor() {
    this.discoveredFiles = new Map();
    this.processedPackets = new Set();
    this.metadataCache = new Map();
    this.correlationMap = new Map();
  }

  // Validate and process metadata packet
  processMetadata(data, timestamp) {
    // Validate metadata format
    if (!data.fileName || !data.chunksCount) {
      return false;
    }

    const fileId = data.fileChecksum ? data.fileChecksum.substring(0, 8) :
                   crypto.createHash('md5').update(data.fileName).digest('hex').substring(0, 8);

    if (!this.discoveredFiles.has(data.fileName)) {
      console.log(`\n📄 Metadata discovered at ${timestamp.toFixed(1)}s:`);
      console.log(`   File: ${data.fileName}`);
      console.log(`   Type: ${data.fileType || 'unknown'}`);
      console.log(`   Size: ${data.fileSize ? (data.fileSize / 1024).toFixed(1) + ' KB' : 'unknown'}`);
      console.log(`   Chunks: ${data.chunksCount}`);
      console.log(`   FileID: ${fileId}`);

      const decoder = new FountainDecoder();
      decoder.initialize(data);

      this.discoveredFiles.set(data.fileName, {
        metadata: data,
        decoder: decoder,
        fileId: fileId,
        packets: [],
        processedPacketIds: new Set(),
        firstSeen: timestamp,
        lastUpdate: timestamp,
        completed: false,
        validatedChunks: new Set(),
        duplicateCount: 0,
        errorCount: 0
      });

      // Create correlation mapping
      this.correlationMap.set(fileId, data.fileName);
      this.metadataCache.set(fileId, data);

      return true;
    }
    return false;
  }

  // Validate and process data packet
  processDataPacket(packet, timestamp) {
    // Generate packet unique ID for duplicate detection
    const packetId = `${packet.packetId}_${packet.timestamp1}_${packet.timestamp2}`;

    if (this.processedPackets.has(packetId)) {
      return { added: false, reason: 'duplicate' };
    }

    // Validate packet structure
    if (!packet.data || !packet.numChunks) {
      return { added: false, reason: 'invalid_structure' };
    }

    // Try to correlate with a file
    let targetFile = null;
    let fileInfo = null;

    // Method 1: Direct file ID correlation
    if (packet.fileId && this.correlationMap.has(packet.fileId)) {
      const fileName = this.correlationMap.get(packet.fileId);
      fileInfo = this.discoveredFiles.get(fileName);
      targetFile = fileName;
    }

    // Method 2: Match by chunk count (if only one file with that count)
    if (!targetFile) {
      const filesWithChunkCount = Array.from(this.discoveredFiles.values())
        .filter(f => f.metadata.chunksCount === packet.numChunks);

      if (filesWithChunkCount.length === 1) {
        fileInfo = filesWithChunkCount[0];
        targetFile = fileInfo.metadata.fileName;
      }
    }

    // Method 3: Try to infer from timestamp correlation
    if (!targetFile && packet.timestamp2) {
      // Look for files with similar base timestamp
      for (const [fileName, info] of this.discoveredFiles) {
        if (Math.abs(info.firstSeen - timestamp) < 10) { // Within 10 seconds
          fileInfo = info;
          targetFile = fileName;
          break;
        }
      }
    }

    // If no correlation found, store for later
    if (!targetFile) {
      // Create placeholder if this is a new chunk count
      const placeholderName = `unknown_${packet.numChunks}chunks`;
      if (!this.discoveredFiles.has(placeholderName)) {
        console.log(`\n⚠️  Data packets found for unknown file (${packet.numChunks} chunks)`);

        const decoder = new FountainDecoder();
        const pseudoMetadata = {
          fileName: placeholderName,
          fileSize: 0,
          chunksCount: packet.numChunks,
          fileType: 'application/octet-stream'
        };
        decoder.initialize(pseudoMetadata);

        this.discoveredFiles.set(placeholderName, {
          metadata: pseudoMetadata,
          decoder: decoder,
          fileId: null,
          packets: [],
          processedPacketIds: new Set(),
          firstSeen: timestamp,
          lastUpdate: timestamp,
          completed: false,
          validatedChunks: new Set(),
          duplicateCount: 0,
          errorCount: 0
        });
      }
      targetFile = placeholderName;
      fileInfo = this.discoveredFiles.get(placeholderName);
    }

    // Validate packet against file info
    if (fileInfo) {
      // Check for duplicate packet ID
      if (fileInfo.processedPacketIds.has(packetId)) {
        fileInfo.duplicateCount++;
        return { added: false, reason: 'duplicate' };
      }

      // Validate chunk indices
      if (packet.sourceIndices) {
        for (const idx of packet.sourceIndices) {
          if (idx >= fileInfo.metadata.chunksCount) {
            fileInfo.errorCount++;
            return { added: false, reason: 'invalid_index' };
          }
        }
      }

      // Try to add packet to decoder
      try {
        const added = fileInfo.decoder.addPacket({
          degree: packet.degree || 1,
          sourceIndices: packet.sourceIndices || [packet.sourceIndex || packet.packetId || 0],
          data: packet.data
        });

        if (added) {
          fileInfo.packets.push(packet);
          fileInfo.processedPacketIds.add(packetId);
          fileInfo.lastUpdate = timestamp;
          this.processedPackets.add(packetId);

          // Update validated chunks
          if (packet.sourceIndices) {
            packet.sourceIndices.forEach(idx => fileInfo.validatedChunks.add(idx));
          }

          return {
            added: true,
            fileName: targetFile,
            progress: fileInfo.decoder.getRecoveryProgress()
          };
        }
      } catch (error) {
        fileInfo.errorCount++;
        console.error(`Packet processing error: ${error.message}`);
        return { added: false, reason: 'decoder_error' };
      }
    }

    return { added: false, reason: 'no_target' };
  }

  // Get current status
  getStatus() {
    const files = [];
    for (const [fileName, info] of this.discoveredFiles) {
      const progress = info.decoder.getRecoveryProgress();
      files.push({
        fileName,
        progress,
        completed: info.completed,
        packets: info.packets.length,
        duplicates: info.duplicateCount,
        errors: info.errorCount,
        validatedChunks: info.validatedChunks.size
      });
    }
    return files;
  }

  // Try to finalize completed files
  async finalizeFiles(outputDir) {
    const results = [];

    for (const [fileName, fileInfo] of this.discoveredFiles) {
      if (fileInfo.completed) continue;

      const progress = fileInfo.decoder.getRecoveryProgress();

      if (progress.recovered === progress.total) {
        console.log(`\n✅ File ready for recovery: ${fileName}`);

        try {
          const fileData = fileInfo.decoder.finalizeFile();

          if (fileData) {
            // Validate checksum if available
            if (fileInfo.metadata.fileChecksum) {
              const actualChecksum = crypto.createHash('sha256')
                .update(fileData)
                .digest('hex');

              if (actualChecksum !== fileInfo.metadata.fileChecksum) {
                console.log(`   ⚠️  Checksum mismatch!`);
                console.log(`   Expected: ${fileInfo.metadata.fileChecksum}`);
                console.log(`   Got: ${actualChecksum}`);
              } else {
                console.log(`   ✓ Checksum verified`);
              }
            }

            const outputPath = path.join(outputDir, fileName);
            await fs.writeFile(outputPath, fileData);
            console.log(`   📁 Saved to: ${outputPath}`);
            fileInfo.completed = true;

            results.push({
              fileName,
              success: true,
              outputPath,
              size: fileData.length
            });
          } else {
            console.log(`   ❌ File recovery failed`);
            results.push({
              fileName,
              success: false,
              reason: 'finalization_failed'
            });
          }
        } catch (error) {
          console.log(`   ❌ Error: ${error.message}`);
          results.push({
            fileName,
            success: false,
            reason: error.message
          });
        }
      }
    }

    return results;
  }
}

const scanVideoValidated = async (videoPath, options = {}) => {
  const fps = options.fps || 30;
  const outputDir = options.output || './decoded';

  console.log('\n⚡ QRF Validated Fast Scanner\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`🚀 Scan rate: ${fps} FPS`);
  console.log(`📁 Output: ${outputDir}`);
  console.log(`✅ Full validation enabled\n`);
  console.log('─'.repeat(50) + '\n');

  // Get video metadata
  const metadata = await new Promise((resolve, reject) => {
    ffmpeg.ffprobe(videoPath, (err, data) => {
      if (err) reject(err);
      else resolve(data);
    });
  });

  const duration = metadata.format.duration;
  console.log(`📊 Video duration: ${duration.toFixed(1)}s\n`);

  // Create output directory
  await fs.mkdir(outputDir, { recursive: true });

  // Initialize validated decoder
  const decoder = new ValidatedFileDecoder();

  // Statistics
  const stats = {
    startTime: Date.now(),
    frameCount: 0,
    qrDecoded: 0,
    metadataFound: 0,
    dataPackets: 0,
    lastUpdate: Date.now()
  };

  console.log('🔥 Processing video with full validation...\n');

  // Process video
  await processVideoValidated(videoPath, fps, (qrData, frameInfo) => {
    if (!qrData) return;

    stats.frameCount = frameInfo.frameCount;
    stats.qrDecoded++;

    if (qrData.type === 'metadata') {
      if (decoder.processMetadata(qrData, frameInfo.timestamp)) {
        stats.metadataFound++;
      }
    } else if (qrData.type === 'data') {
      const result = decoder.processDataPacket(qrData, frameInfo.timestamp);
      if (result.added) {
        stats.dataPackets++;

        // Show progress for active files
        if (result.progress && stats.dataPackets % 10 === 0) {
          process.stdout.write(`\r📦 ${result.fileName}: ${result.progress.recovered}/${result.progress.total} chunks (${result.progress.percentage}%)`);
        }
      }
    }

    // Update stats display
    if (Date.now() - stats.lastUpdate > 100) {
      const elapsed = (Date.now() - stats.startTime) / 1000;
      const fps = stats.frameCount / elapsed;
      const qrRate = stats.qrDecoded / elapsed;

      process.stdout.write(`\r⚡ Frames: ${stats.frameCount} | QRs: ${stats.qrDecoded} | Meta: ${stats.metadataFound} | Data: ${stats.dataPackets} | Speed: ${fps.toFixed(0)} fps`);
      stats.lastUpdate = Date.now();
    }
  });

  const elapsed = ((Date.now() - stats.startTime) / 1000).toFixed(1);
  console.log(`\n\n✅ Scan complete in ${elapsed}s!\n`);

  // Show statistics
  console.log('📊 Scan Statistics:');
  console.log(`   • Frames processed: ${stats.frameCount}`);
  console.log(`   • Processing speed: ${(stats.frameCount / elapsed).toFixed(0)} fps`);
  console.log(`   • QR codes decoded: ${stats.qrDecoded}`);
  console.log(`   • Metadata frames: ${stats.metadataFound}`);
  console.log(`   • Valid data packets: ${stats.dataPackets}\n`);

  // Show file status
  const fileStatus = decoder.getStatus();
  if (fileStatus.length > 0) {
    console.log('📁 Discovered Files:\n');

    for (const file of fileStatus) {
      const icon = file.completed ? '✅' : file.progress.percentage === 100 ? '🔄' : '⏳';
      console.log(`${icon} ${file.fileName}`);
      console.log(`   Progress: ${file.progress.recovered}/${file.progress.total} chunks (${file.progress.percentage}%)`);
      console.log(`   Packets: ${file.packets} valid, ${file.duplicates} duplicates, ${file.errors} errors`);
      console.log(`   Validated chunks: ${file.validatedChunks}\n`);
    }
  }

  // Try to recover files
  console.log('🔧 Attempting file recovery...\n');
  const results = await decoder.finalizeFiles(outputDir);

  // Final summary
  console.log('\n' + '─'.repeat(50));
  console.log('\n📊 Final Summary:\n');

  let successCount = 0;
  for (const result of results) {
    if (result.success) {
      console.log(`✅ ${result.fileName} - ${(result.size / 1024).toFixed(1)} KB`);
      successCount++;
    } else {
      console.log(`❌ ${result.fileName} - ${result.reason}`);
    }
  }

  console.log(`\n   Files recovered: ${successCount}/${fileStatus.length}`);
  console.log(`   Total time: ${elapsed}s`);
  console.log(`   Average speed: ${(stats.frameCount / elapsed).toFixed(0)} fps`);
};

// Fast video processing with validation
async function processVideoValidated(videoPath, targetFps, callback) {
  return new Promise((resolve, reject) => {
    const qrDecoder = new QRDecoder();
    let frameCount = 0;
    let frameId = 0;

    // Optimized FFmpeg settings
    const args = [
      '-i', videoPath,
      '-threads', '0',
      '-vf', `fps=${targetFps},scale=720:720`, // Slightly larger for better QR detection
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

    const processFrame = async (frame, id) => {
      try {
        const qrData = await qrDecoder.decode({ data: frame });
        if (qrData) {
          await callback(qrData, {
            frameCount,
            timestamp: id / targetFps
          });
        }
      } catch (error) {
        // Ignore decode errors
      }
    };

    const processBatch = async () => {
      if (isProcessing || frameQueue.length === 0) return;
      isProcessing = true;

      const batch = frameQueue.splice(0, 5);
      await Promise.all(batch.map(({ frame, id }) => processFrame(frame, id)));

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
        frameCount++;

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
      // Wait for queue to empty
      while (frameQueue.length > 0 || isProcessing) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }

      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`FFmpeg exited with code ${code}`));
      }
    });

    ffmpegProcess.on('error', reject);
  });
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node scanner-validated.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>     Target frame rate (default: 30)');
  console.log('  --output <dir>   Output directory (default: ./decoded)');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 30,
  output: './decoded'
};

for (let i = 3; i < process.argv.length; i += 2) {
  if (process.argv[i] === '--fps') {
    options.fps = parseFloat(process.argv[i + 1]);
  } else if (process.argv[i] === '--output') {
    options.output = process.argv[i + 1];
  }
}

scanVideoValidated(videoPath, options).catch(console.error);