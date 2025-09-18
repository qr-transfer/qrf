#!/usr/bin/env node
import { spawn } from 'child_process';
import { QRDecoder } from './src/qrDecoder.js';
import { FountainDecoder } from './src/fountainDecoder.js';
import fs from 'fs/promises';
import path from 'path';
import ffmpeg from 'fluent-ffmpeg';
import { Worker } from 'worker_threads';
import os from 'os';

const scanVideoFast = async (videoPath, options = {}) => {
  const fps = options.fps || 30; // Higher FPS for faster scanning
  const outputDir = options.output || './decoded';
  const threads = options.threads || os.cpus().length;

  console.log('\n⚡ QRF Fast Video Scanner\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`🚀 Scan rate: ${fps} FPS (maximum speed)`);
  console.log(`🔧 Threads: ${threads}`);
  console.log(`📁 Output: ${outputDir}\n`);
  console.log('─'.repeat(50) + '\n');

  // Get video info
  const metadata = await new Promise((resolve, reject) => {
    ffmpeg.ffprobe(videoPath, (err, data) => {
      if (err) reject(err);
      else resolve(data);
    });
  });

  const duration = metadata.format.duration;
  const videoBitrate = metadata.format.bit_rate;
  const videoFps = metadata.streams[0].r_frame_rate.split('/')[0] / metadata.streams[0].r_frame_rate.split('/')[1];

  console.log(`📊 Video info:`);
  console.log(`   Duration: ${duration.toFixed(1)}s`);
  console.log(`   Native FPS: ${videoFps.toFixed(1)}`);
  console.log(`   Total frames: ~${Math.floor(duration * videoFps)}`);
  console.log(`   Processing at: ${fps} FPS\n`);

  // Create output directory
  await fs.mkdir(outputDir, { recursive: true });

  // Storage for discovered data
  const discoveredFiles = new Map();
  const dataPackets = [];
  const metadataFrames = [];

  // Performance tracking
  const startTime = Date.now();
  let frameCount = 0;
  let decodedCount = 0;
  let lastUpdate = Date.now();

  console.log('🔥 Processing video at maximum speed...\n');

  // Use optimized FFmpeg settings for fast extraction
  await processVideoFast(videoPath, fps, async (qrData, frameInfo) => {
    if (!qrData) return;

    if (qrData.type === 'metadata') {
      metadataFrames.push({
        ...qrData,
        timestamp: frameInfo.timestamp
      });

      const fileName = qrData.fileName;
      if (!discoveredFiles.has(fileName)) {
        console.log(`\n✅ Metadata found at ${frameInfo.timestamp.toFixed(1)}s: ${fileName}`);

        const decoder = new FountainDecoder();
        decoder.initialize(qrData);

        discoveredFiles.set(fileName, {
          metadata: qrData,
          decoder: decoder,
          packets: [],
          firstSeen: frameInfo.timestamp,
          completed: false
        });
      }
    } else if (qrData.type === 'data') {
      dataPackets.push({
        ...qrData,
        timestamp: frameInfo.timestamp
      });
    }

    frameCount = frameInfo.frameCount;
    decodedCount = frameInfo.decodedCount;

    // Update progress every 100ms
    if (Date.now() - lastUpdate > 100) {
      const elapsed = (Date.now() - startTime) / 1000;
      const fps = frameCount / elapsed;
      const qrRate = decodedCount / elapsed;

      process.stdout.write(`\r⚡ Frames: ${frameCount} | QRs: ${decodedCount} | Speed: ${fps.toFixed(0)} fps | QR rate: ${qrRate.toFixed(0)}/s`);
      lastUpdate = Date.now();
    }
  });

  const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
  console.log(`\n\n✅ Scan complete in ${elapsed}s!\n`);
  console.log(`📊 Results:`);
  console.log(`   • Frames processed: ${frameCount}`);
  console.log(`   • Processing speed: ${(frameCount / elapsed).toFixed(0)} fps`);
  console.log(`   • QR codes found: ${decodedCount}`);
  console.log(`   • Detection rate: ${(decodedCount / elapsed).toFixed(0)} QR/s`);
  console.log(`   • Metadata frames: ${metadataFrames.length}`);
  console.log(`   • Data packets: ${dataPackets.length}\n`);

  // If no metadata found, infer from data packets
  if (discoveredFiles.size === 0 && dataPackets.length > 0) {
    console.log('⚠️  No metadata frames found, inferring from data packets...\n');

    const chunkGroups = {};
    for (const packet of dataPackets) {
      const key = packet.numChunks;
      if (!chunkGroups[key]) {
        chunkGroups[key] = [];
      }
      chunkGroups[key].push(packet);
    }

    let fileIndex = 0;
    for (const [numChunks, packets] of Object.entries(chunkGroups)) {
      const fileName = `recovered_file_${fileIndex++}_${numChunks}chunks.bin`;
      console.log(`📄 Inferred file: ${fileName} (${numChunks} chunks, ${packets.length} packets)`);

      const decoder = new FountainDecoder();
      const pseudoMetadata = {
        fileName: fileName,
        fileSize: 0,
        chunksCount: parseInt(numChunks),
        fileType: 'application/octet-stream'
      };
      decoder.initialize(pseudoMetadata);

      discoveredFiles.set(fileName, {
        metadata: pseudoMetadata,
        decoder: decoder,
        packets: packets,
        firstSeen: packets[0].timestamp,
        completed: false
      });
    }
  }

  // Process data packets
  if (discoveredFiles.size > 0) {
    console.log('\n🔧 Processing data packets...\n');

    for (const [fileName, fileInfo] of discoveredFiles) {
      const decoder = fileInfo.decoder;
      const relevantPackets = fileInfo.packets.length > 0 ?
        fileInfo.packets :
        dataPackets.filter(p => p.numChunks === fileInfo.metadata.chunksCount);

      console.log(`\n📦 Processing ${fileName}:`);
      console.log(`   Packets available: ${relevantPackets.length}`);

      let addedCount = 0;
      for (const packet of relevantPackets) {
        const added = decoder.addPacket({
          degree: packet.degree || 1,
          sourceIndices: packet.sourceIndices || [packet.sourceIndex || packet.packetId || 0],
          data: packet.data
        });

        if (added) {
          addedCount++;
          const progress = decoder.getRecoveryProgress();

          if (addedCount % 50 === 0 || progress.recovered === progress.total) {
            process.stdout.write(`\r   Progress: ${progress.recovered}/${progress.total} chunks (${progress.percentage}%)`);
          }

          if (progress.recovered === progress.total) {
            console.log('\n   ✅ File complete!');

            try {
              const fileData = decoder.finalizeFile();
              if (fileData) {
                const outputPath = path.join(outputDir, fileName);
                await fs.writeFile(outputPath, fileData);
                console.log(`   📁 Saved to: ${outputPath}`);
                fileInfo.completed = true;
              } else {
                console.log('   ⚠️  File recovery failed (checksum mismatch)');
              }
            } catch (error) {
              console.log(`   ❌ Error saving file: ${error.message}`);
            }
            break;
          }
        }
      }

      if (!fileInfo.completed) {
        const progress = decoder.getRecoveryProgress();
        console.log(`\n   ⏳ Incomplete: ${progress.recovered}/${progress.total} chunks (${progress.percentage}%)`);
      }
    }
  }

  // Summary
  console.log('\n' + '─'.repeat(50));
  console.log('\n📊 Final Summary:\n');

  let completedCount = 0;
  for (const [fileName, fileInfo] of discoveredFiles) {
    const status = fileInfo.completed ? '✅' : '⏳';
    console.log(`${status} ${fileName}`);
    if (fileInfo.completed) completedCount++;
  }

  console.log(`\n   Total files: ${discoveredFiles.size}`);
  console.log(`   Completed: ${completedCount}`);
  console.log(`   Processing time: ${elapsed}s`);
  console.log(`   Average speed: ${(frameCount / elapsed).toFixed(0)} fps`);
};

// Optimized video processing with maximum speed
async function processVideoFast(videoPath, targetFps, callback) {
  return new Promise((resolve, reject) => {
    const qrDecoder = new QRDecoder();
    let frameCount = 0;
    let decodedCount = 0;
    const pendingDecodes = new Map();
    let frameId = 0;

    // Optimized FFmpeg settings for maximum speed
    const args = [
      '-i', videoPath,

      // Hardware acceleration (if available)
      // '-hwaccel', 'auto',

      // Fast seeking and decoding
      '-threads', '0',  // Use all available threads
      '-preset', 'ultrafast',

      // Frame extraction settings
      '-vf', `fps=${targetFps},scale=640:640`,  // Downscale for faster QR detection

      // Output settings
      '-c:v', 'mjpeg',
      '-q:v', '2',  // Higher quality for better QR detection
      '-f', 'image2pipe',
      '-'
    ];

    console.log('🚀 FFmpeg command optimized for speed\n');

    const ffmpegProcess = spawn('ffmpeg', args, {
      stdio: ['ignore', 'pipe', 'ignore']  // Ignore stderr for speed
    });

    let buffer = Buffer.alloc(0);
    let isProcessing = false;
    const frameQueue = [];

    // Process frames in parallel
    const processFrame = async (frame, id) => {
      try {
        const qrData = await qrDecoder.decode({ data: frame });
        if (qrData) {
          decodedCount++;
          await callback(qrData, {
            index: id,
            timestamp: id / targetFps,
            decodedCount,
            frameCount
          });
        }
      } catch (error) {
        // Ignore decode errors
      } finally {
        pendingDecodes.delete(id);
      }
    };

    // Batch process frames
    const processBatch = async () => {
      if (isProcessing || frameQueue.length === 0) return;
      isProcessing = true;

      const batch = frameQueue.splice(0, 10); // Process 10 frames at a time
      const promises = batch.map(({ frame, id }) => processFrame(frame, id));

      await Promise.all(promises);
      isProcessing = false;

      // Process next batch
      if (frameQueue.length > 0) {
        setImmediate(processBatch);
      }
    };

    ffmpegProcess.stdout.on('data', (chunk) => {
      buffer = Buffer.concat([buffer, chunk]);

      // Extract JPEG frames
      let frameStart = 0;
      while (true) {
        const jpegStart = buffer.indexOf(Buffer.from([0xFF, 0xD8]), frameStart);
        if (jpegStart === -1) break;

        const jpegEnd = buffer.indexOf(Buffer.from([0xFF, 0xD9]), jpegStart + 2);
        if (jpegEnd === -1) break;

        const frame = buffer.slice(jpegStart, jpegEnd + 2);
        frameCount++;
        const currentId = frameId++;

        // Add to queue for batch processing
        frameQueue.push({ frame, id: currentId });

        // Start processing if not already
        if (!isProcessing) {
          setImmediate(processBatch);
        }

        frameStart = jpegEnd + 2;
      }

      // Keep unprocessed data
      if (frameStart > 0 && frameStart < buffer.length) {
        buffer = buffer.slice(frameStart);
      } else if (frameStart >= buffer.length) {
        buffer = Buffer.alloc(0);
      }
    });

    ffmpegProcess.on('close', async (code) => {
      // Wait for pending decodes
      while (pendingDecodes.size > 0 || frameQueue.length > 0) {
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
  console.log('Usage: node scanner-fast.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>     Target frame rate (default: 30)');
  console.log('  --output <dir>   Output directory (default: ./decoded)');
  console.log('  --threads <n>    Number of threads (default: CPU count)');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 30,
  output: './decoded',
  threads: os.cpus().length
};

// Parse options
for (let i = 3; i < process.argv.length; i += 2) {
  if (process.argv[i] === '--fps') {
    options.fps = parseFloat(process.argv[i + 1]);
  } else if (process.argv[i] === '--output') {
    options.output = process.argv[i + 1];
  } else if (process.argv[i] === '--threads') {
    options.threads = parseInt(process.argv[i + 1]);
  }
}

scanVideoFast(videoPath, options).catch(console.error);