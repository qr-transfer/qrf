#!/usr/bin/env node
import { spawn } from 'child_process';
import jsQR from 'jsqr';
import sharp from 'sharp';
import fs from 'fs/promises';
import path from 'path';
import crypto from 'crypto';
import { Worker } from 'worker_threads';
import os from 'os';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

class ParallelDecoder {
  constructor() {
    this.discoveredFiles = new Map();
    this.processedChunks = new Map();
    this.totalProcessedPackets = 0;
    this.duplicatePackets = 0;
    this.startTime = Date.now();
    this.frameCount = 0;
    this.qrDecoded = 0;
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

      // Show progress every 20 chunks
      if (fileInfo.recoveredChunks % 20 === 0 || fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
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
}

// Extract frames first, then process in parallel
async function extractFrames(videoPath, fps, maxFrames = 1000) {
  const tempDir = './tmp/frames';
  await fs.mkdir(tempDir, { recursive: true });

  console.log(`🎬 Extracting frames to ${tempDir}...`);

  return new Promise((resolve, reject) => {
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

    ffmpegProcess.on('close', async (code) => {
      if (code === 0) {
        // Get list of extracted frames
        const files = await fs.readdir(tempDir);
        const frameFiles = files
          .filter(f => f.startsWith('frame_') && f.endsWith('.jpg'))
          .sort()
          .map(f => path.join(tempDir, f));

        console.log(`✅ Extracted ${frameFiles.length} frames`);
        resolve(frameFiles);
      } else {
        reject(new Error(`FFmpeg exited with code ${code}`));
      }
    });

    ffmpegProcess.on('error', reject);
  });
}

async function processFramesInParallel(frameFiles, fps) {
  const numWorkers = Math.min(os.cpus().length, 4); // Limit to 4 workers
  const framesPerWorker = Math.ceil(frameFiles.length / numWorkers);

  console.log(`🔧 Processing ${frameFiles.length} frames with ${numWorkers} workers...`);

  const workers = [];
  const promises = [];

  for (let i = 0; i < numWorkers; i++) {
    const startIndex = i * framesPerWorker;
    const endIndex = Math.min(startIndex + framesPerWorker, frameFiles.length);

    const worker = new Worker(path.join(__dirname, 'qr-worker.js'), {
      workerData: {
        frameFiles,
        startIndex,
        endIndex
      }
    });

    workers.push(worker);

    const promise = new Promise((resolve) => {
      worker.on('message', (results) => {
        resolve(results);
      });
    });

    promises.push(promise);
  }

  const allResults = await Promise.all(promises);

  // Cleanup workers
  workers.forEach(worker => worker.terminate());

  // Flatten results and sort by frame index
  const qrResults = allResults
    .flat()
    .sort((a, b) => a.frameIndex - b.frameIndex);

  console.log(`✅ Found ${qrResults.length} QR codes`);
  return qrResults;
}

// Main function
async function decodeVideo(videoPath, options) {
  const fps = options.fps || 10; // Lower FPS for initial test
  const maxFrames = options.maxFrames || 3000; // Limit frames for testing
  const outputDir = options.output || './decoded';

  console.log('\n🎬 QRF Parallel Decoder\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`⚡ Extract rate: ${fps} FPS`);
  console.log(`📁 Output: ${outputDir}`);
  console.log(`🔄 Max frames: ${maxFrames}`);
  console.log(`⚙️  Parallel processing enabled\n`);
  console.log('─'.repeat(60) + '\n');

  // Create output directory
  await fs.mkdir(outputDir, { recursive: true });

  // Step 1: Extract frames
  const frameFiles = await extractFrames(videoPath, fps, maxFrames);

  // Step 2: Process frames in parallel
  const qrResults = await processFramesInParallel(frameFiles, fps);

  // Step 3: Process QR data
  const decoder = new ParallelDecoder();

  console.log('\n🔍 Processing QR data...\n');

  for (const result of qrResults) {
    const timestamp = result.frameIndex / fps;
    await decoder.processQRData(result.qrData, timestamp);
  }

  // Step 4: Save files
  console.log('\n📊 Results:\n');
  console.log(`   Frames processed: ${frameFiles.length}`);
  console.log(`   QR codes found: ${qrResults.length}`);
  console.log(`   Files discovered: ${decoder.discoveredFiles.size}\n`);

  if (decoder.discoveredFiles.size > 0) {
    console.log('💾 Saving files...\n');
    const results = await decoder.saveCompletedFiles(outputDir);

    for (const result of results) {
      if (result.success) {
        console.log(`   ✅ ${result.fileName} - ${(result.size / 1024).toFixed(1)} KB`);
      } else {
        console.log(`   ❌ ${result.fileName} - ${result.error}`);
      }
    }
  }

  // Cleanup
  try {
    await fs.rm('./tmp/frames', { recursive: true });
  } catch (error) {
    // Ignore cleanup errors
  }

  console.log('\n✅ Parallel processing complete!');
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node decoder-parallel.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>       Frame extraction rate (default: 10)');
  console.log('  --maxFrames <n>    Maximum frames to process (default: 3000)');
  console.log('  --output <dir>     Output directory (default: ./decoded)');
  console.log('\nParallel processing with worker threads');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 10,
  maxFrames: 3000,
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