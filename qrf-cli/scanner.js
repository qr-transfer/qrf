#!/usr/bin/env node
import { spawn } from 'child_process';
import { QRDecoder } from './src/qrDecoder.js';
import { FountainDecoder } from './src/fountainDecoder.js';
import fs from 'fs/promises';
import path from 'path';
import ffmpeg from 'fluent-ffmpeg';

const scanVideo = async (videoPath, options = {}) => {
  const fps = options.fps || 2;
  const outputDir = options.output || './decoded';

  console.log('\n🔍 QRF Video Scanner\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`⚡ Scan rate: ${fps} FPS`);
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
  console.log(`📊 Video duration: ${duration.toFixed(1)}s\n`);

  // Create output directory
  await fs.mkdir(outputDir, { recursive: true });

  // Storage for discovered data
  const discoveredFiles = new Map();
  const dataPackets = [];
  const decoders = new Map();

  // Phase 1: Fast scan for metadata
  console.log('🔍 Phase 1: Scanning for metadata frames...\n');

  await processVideo(videoPath, fps, async (qrData, frameInfo) => {
    if (!qrData) return;

    if (qrData.type === 'metadata') {
      // Found metadata!
      const fileName = qrData.fileName;
      if (!discoveredFiles.has(fileName)) {
        console.log(`\n📄 Found metadata at ${frameInfo.timestamp.toFixed(1)}s:`);
        console.log(`   File: ${fileName}`);
        console.log(`   Size: ${(qrData.fileSize / 1024).toFixed(1)} KB`);
        console.log(`   Chunks: ${qrData.chunksCount}`);
        console.log(`   Type: ${qrData.fileType || 'unknown'}\n`);

        // Initialize decoder
        const decoder = new FountainDecoder();
        decoder.initialize(qrData);

        discoveredFiles.set(fileName, {
          metadata: qrData,
          decoder: decoder,
          packets: [],
          firstSeen: frameInfo.timestamp,
          completed: false
        });

        decoders.set(fileName, decoder);
      }
    } else if (qrData.type === 'data') {
      // Store data packet
      dataPackets.push({
        ...qrData,
        timestamp: frameInfo.timestamp
      });

      // Progress indicator
      if (dataPackets.length % 100 === 0) {
        process.stdout.write(`\r📦 Data packets found: ${dataPackets.length}`);
      }
    }
  });

  console.log(`\n\n✅ Scan complete!\n`);
  console.log(`📊 Results:`);
  console.log(`   • Metadata frames found: ${discoveredFiles.size}`);
  console.log(`   • Data packets found: ${dataPackets.length}\n`);

  // If no metadata found, try to infer from data packets
  if (discoveredFiles.size === 0 && dataPackets.length > 0) {
    console.log('⚠️  No metadata frames found, inferring from data packets...\n');

    // Group packets by total chunks to identify different files
    const chunkGroups = {};
    for (const packet of dataPackets) {
      const key = packet.numChunks;
      if (!chunkGroups[key]) {
        chunkGroups[key] = [];
      }
      chunkGroups[key].push(packet);
    }

    // Create pseudo-metadata for each group
    let fileIndex = 0;
    for (const [numChunks, packets] of Object.entries(chunkGroups)) {
      const fileName = `recovered_file_${fileIndex++}_${numChunks}chunks.bin`;
      console.log(`📄 Inferred file: ${fileName} (${numChunks} chunks, ${packets.length} packets)`);

      // Create a decoder without full metadata
      const decoder = new FountainDecoder();
      const pseudoMetadata = {
        fileName: fileName,
        fileSize: 0, // Unknown
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

  // Phase 2: Process data packets
  if (discoveredFiles.size > 0) {
    console.log('\n🔧 Phase 2: Processing data packets...\n');

    for (const [fileName, fileInfo] of discoveredFiles) {
      const decoder = fileInfo.decoder;
      const relevantPackets = fileInfo.packets.length > 0 ?
        fileInfo.packets :
        dataPackets.filter(p => p.numChunks === fileInfo.metadata.chunksCount);

      console.log(`\n📦 Processing ${fileName}:`);
      console.log(`   Packets to process: ${relevantPackets.length}`);

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

          if (addedCount % 10 === 0 || progress.recovered === progress.total) {
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
  console.log(`   Data packets processed: ${dataPackets.length}`);
};

// Process video with callback for each QR found
async function processVideo(videoPath, fps, callback) {
  return new Promise((resolve, reject) => {
    const qrDecoder = new QRDecoder();
    let frameCount = 0;
    let decodedCount = 0;

    // Use FFmpeg to extract frames
    const args = [
      '-i', videoPath,
      '-vf', `fps=${fps}`,
      '-c:v', 'mjpeg',
      '-f', 'image2pipe',
      '-'
    ];

    const ffmpegProcess = spawn('ffmpeg', args);
    let buffer = Buffer.alloc(0);

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

        // Try to decode QR
        qrDecoder.decode({ data: frame }).then(qrData => {
          if (qrData) {
            decodedCount++;
            callback(qrData, {
              index: frameCount,
              timestamp: frameCount / fps,
              decodedCount
            });
          }
        }).catch(() => {
          // Ignore decode errors
        });

        frameStart = jpegEnd + 2;
      }

      // Keep unprocessed data
      if (frameStart > 0 && frameStart < buffer.length) {
        buffer = buffer.slice(frameStart);
      } else if (frameStart >= buffer.length) {
        buffer = Buffer.alloc(0);
      }

      // Progress
      if (frameCount % 100 === 0) {
        process.stdout.write(`\r🎬 Frames processed: ${frameCount} | QRs decoded: ${decodedCount}`);
      }
    });

    ffmpegProcess.on('close', (code) => {
      process.stdout.write(`\r🎬 Frames processed: ${frameCount} | QRs decoded: ${decodedCount}\n`);
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
  console.log('Usage: node scanner.js <video> [options]');
  console.log('Options:');
  console.log('  --fps <rate>     Scan frame rate (default: 2)');
  console.log('  --output <dir>   Output directory (default: ./decoded)');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 2,
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

scanVideo(videoPath, options).catch(console.error);