#!/usr/bin/env node
import { spawn } from 'child_process';
import ffmpeg from 'fluent-ffmpeg';
import Jimp from 'jimp';
import QrCode from 'qrcode-reader';
import fs from 'fs/promises';
import path from 'path';
import crypto from 'crypto';
import readline from 'readline';
import { Worker } from 'worker_threads';
import os from 'os';

// Setup keyboard input
readline.emitKeypressEvents(process.stdin);
if (process.stdin.isTTY) {
  process.stdin.setRawMode(true);
}

// Turbo-optimized decoder with runtime controls
class TurboDecoder {
  constructor() {
    // Core data structures
    this.discoveredFiles = new Map();
    this.frameCount = 0;
    this.qrDecoded = 0;
    this.startTime = Date.now();

    // Performance settings (adjustable at runtime)
    this.settings = {
      fps: 20,
      quality: 3,  // 1-5 (1=fast/low quality, 5=slow/high quality)
      scale: 480,  // Frame scale for QR detection
      parallel: 4, // Parallel decode workers
      skipFrames: 0, // Skip N frames between processing
      turboMode: false // Ultra-fast mode
    };

    // Stats
    this.stats = {
      currentFps: 0,
      peakFps: 0,
      qrRate: 0,
      memoryMB: 0,
      totalPackets: 0,
      validPackets: 0
    };

    // Setup keyboard controls
    this.setupKeyboardControls();
  }

  setupKeyboardControls() {
    console.log('\n⌨️  KEYBOARD CONTROLS:');
    console.log('  [+/-]     Increase/decrease FPS target');
    console.log('  [Q/A]     Increase/decrease quality');
    console.log('  [T]       Toggle TURBO mode');
    console.log('  [S]       Toggle frame skipping');
    console.log('  [P]       Pause/resume');
    console.log('  [I]       Show info');
    console.log('  [ESC/Q]   Quit\n');
    console.log('─'.repeat(70) + '\n');

    process.stdin.on('keypress', (str, key) => {
      if (key.ctrl && key.name === 'c') {
        this.shutdown();
      }

      switch (key.name) {
        case 'plus':
        case '=':
          this.settings.fps = Math.min(60, this.settings.fps + 5);
          console.log(`\n⚡ FPS target increased to ${this.settings.fps}`);
          break;

        case 'minus':
        case '-':
          this.settings.fps = Math.max(5, this.settings.fps - 5);
          console.log(`\n⚡ FPS target decreased to ${this.settings.fps}`);
          break;

        case 'q':
          if (key.shift) {
            this.settings.quality = Math.min(5, this.settings.quality + 1);
            console.log(`\n🎯 Quality increased to ${this.settings.quality}/5`);
          } else {
            this.shutdown();
          }
          break;

        case 'a':
          this.settings.quality = Math.max(1, this.settings.quality - 1);
          console.log(`\n🎯 Quality decreased to ${this.settings.quality}/5`);
          break;

        case 't':
          this.settings.turboMode = !this.settings.turboMode;
          if (this.settings.turboMode) {
            this.settings.quality = 1;
            this.settings.scale = 360;
            this.settings.parallel = os.cpus().length;
            console.log(`\n🚀 TURBO MODE ACTIVATED! Quality=1, Scale=360, Workers=${this.settings.parallel}`);
          } else {
            this.settings.quality = 3;
            this.settings.scale = 480;
            this.settings.parallel = 4;
            console.log(`\n🔄 Turbo mode deactivated`);
          }
          break;

        case 's':
          this.settings.skipFrames = this.settings.skipFrames === 0 ? 2 : 0;
          console.log(`\n⏭️  Frame skipping: ${this.settings.skipFrames === 0 ? 'OFF' : `Skip ${this.settings.skipFrames} frames`}`);
          break;

        case 'i':
          this.showInfo();
          break;

        case 'escape':
          this.shutdown();
          break;
      }
    });
  }

  showInfo() {
    console.log('\n' + '═'.repeat(70));
    console.log('📊 CURRENT STATUS');
    console.log('─'.repeat(70));
    console.log(`Settings: FPS=${this.settings.fps} Quality=${this.settings.quality}/5 Scale=${this.settings.scale}px`);
    console.log(`Performance: ${this.stats.currentFps.toFixed(1)} fps (peak: ${this.stats.peakFps.toFixed(1)})`);
    console.log(`Files: ${this.discoveredFiles.size} discovered`);
    console.log(`Memory: ${this.stats.memoryMB} MB`);
    console.log('═'.repeat(70) + '\n');
  }

  shutdown() {
    console.log('\n\n👋 Shutting down...');
    process.exit(0);
  }

  // Fast QR processing with quality settings
  async processQRData(qrString, timestamp) {
    if (!qrString) return;

    try {
      if (qrString.startsWith('M:')) {
        // Metadata packet
        const parts = qrString.split(':');
        if (parts.length < 6) return;

        const metadata = {
          type: 'metadata',
          fileName: decodeURIComponent(parts[2]),
          fileType: decodeURIComponent(parts[3]),
          fileSize: parseInt(parts[4]),
          chunksCount: parseInt(parts[5]),
          fileChecksum: parts[14] || ''
        };

        const fileId = metadata.fileChecksum ?
          metadata.fileChecksum.substring(0, 8) :
          crypto.createHash('md5').update(metadata.fileName).digest('hex').substring(0, 8);

        if (!this.discoveredFiles.has(metadata.fileName)) {
          console.log(`\n✅ File: ${metadata.fileName} (${(metadata.fileSize/1024).toFixed(1)}KB, ${metadata.chunksCount} chunks)`);

          this.discoveredFiles.set(metadata.fileName, {
            metadata: metadata,
            fileId: fileId,
            chunks: new Map(),
            recoveredChunks: 0,
            completed: false
          });
        }
        this.stats.validPackets++;
      }
      else if (qrString.startsWith('D:')) {
        // Data packet - simplified for speed
        const parts = qrString.split(':');
        const packet = {
          numChunks: parseInt(parts[4] || parts[5]),
          index: parseInt(parts[6] || parts[1]),
          data: parts[parts.length - 1]
        };

        if (packet.data) {
          this.processDataPacket(packet, timestamp);
        }
      }
    } catch (error) {
      // Skip errors for speed
    }

    this.qrDecoded++;
  }

  processDataPacket(packet, timestamp) {
    this.stats.totalPackets++;

    // Find target file by chunk count (simplified)
    for (const [fileName, fileInfo] of this.discoveredFiles) {
      if (fileInfo.metadata.chunksCount === packet.numChunks && !fileInfo.completed) {
        const chunkIndex = packet.index % fileInfo.metadata.chunksCount;

        if (!fileInfo.chunks.has(chunkIndex)) {
          try {
            const chunkData = Buffer.from(packet.data, 'base64');
            fileInfo.chunks.set(chunkIndex, chunkData);
            fileInfo.recoveredChunks++;
            this.stats.validPackets++;

            // Simple progress
            const progress = Math.round((fileInfo.recoveredChunks / fileInfo.metadata.chunksCount) * 100);
            if (fileInfo.recoveredChunks % 20 === 0) {
              const bar = this.createProgressBar(progress);
              console.log(`📦 ${fileName}: ${bar} ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount}`);
            }

            // Check completion
            if (fileInfo.recoveredChunks === fileInfo.metadata.chunksCount) {
              fileInfo.completed = true;
              this.saveFile(fileName, fileInfo);
            }
          } catch (e) {
            // Skip decode errors
          }
        }
        return;
      }
    }

    // Unknown file
    const unknownKey = `unknown_${packet.numChunks}`;
    if (!this.discoveredFiles.has(unknownKey)) {
      console.log(`\n⚠️  Unknown file with ${packet.numChunks} chunks`);
      this.discoveredFiles.set(unknownKey, {
        metadata: { fileName: unknownKey, chunksCount: packet.numChunks, fileSize: 0 },
        chunks: new Map(),
        recoveredChunks: 0,
        completed: false
      });
    }
  }

  createProgressBar(percentage) {
    const width = 30;
    const filled = Math.floor((percentage / 100) * width);
    return '[' + '█'.repeat(filled) + '░'.repeat(width - filled) + '] ' + percentage + '%';
  }

  async saveFile(fileName, fileInfo) {
    try {
      const chunks = [];
      for (let i = 0; i < fileInfo.metadata.chunksCount; i++) {
        if (fileInfo.chunks.has(i)) {
          chunks.push(fileInfo.chunks.get(i));
        }
      }

      const fileData = Buffer.concat(chunks);

      // Quick checksum verify
      if (fileInfo.metadata.fileChecksum) {
        const hash = crypto.createHash('sha256').update(fileData).digest('hex');
        const valid = hash === fileInfo.metadata.fileChecksum;
        console.log(`\n✅ ${fileName} complete! Checksum: ${valid ? '✓' : '✗'}`);
      }

      const outputPath = path.join(this.outputDir, fileName);
      await fs.writeFile(outputPath, fileData);
      console.log(`💾 Saved to: ${outputPath}`);

    } catch (error) {
      console.error(`Save error: ${error.message}`);
    }
  }

  updateStats() {
    const elapsed = (Date.now() - this.startTime) / 1000;
    this.stats.currentFps = this.frameCount / elapsed;
    this.stats.qrRate = this.qrDecoded / elapsed;
    this.stats.memoryMB = Math.round(process.memoryUsage().heapUsed / 1024 / 1024);

    if (this.stats.currentFps > this.stats.peakFps) {
      this.stats.peakFps = this.stats.currentFps;
    }

    // Progress display
    let totalChunks = 0;
    let recoveredChunks = 0;
    let completed = 0;

    for (const fileInfo of this.discoveredFiles.values()) {
      totalChunks += fileInfo.metadata.chunksCount;
      recoveredChunks += fileInfo.recoveredChunks;
      if (fileInfo.completed) completed++;
    }

    const progress = totalChunks > 0 ? Math.round((recoveredChunks / totalChunks) * 100) : 0;

    process.stdout.write(`\r🚀 F:${this.frameCount} | QR:${this.qrDecoded} | ${this.stats.currentFps.toFixed(1)}fps (${this.settings.turboMode ? 'TURBO' : `Q${this.settings.quality}`}) | Files:${this.discoveredFiles.size}(${completed}✓) | ${progress}% | ${this.stats.memoryMB}MB`);
  }
}

// Ultra-fast frame processor
async function processTurbo(videoPath, decoder) {
  return new Promise((resolve, reject) => {
    let frameId = 0;
    let skipCounter = 0;
    const qrQueue = [];
    let processing = false;

    // Dynamic FFmpeg args based on settings
    const getFFmpegArgs = () => {
      const scale = decoder.settings.scale;
      const fps = decoder.settings.fps;
      const quality = 6 - decoder.settings.quality; // Invert for ffmpeg

      return [
        '-i', videoPath,
        '-threads', '0',
        '-vf', `fps=${fps},scale=${scale}:${scale}:flags=fast_bilinear`,
        '-c:v', 'mjpeg',
        '-q:v', quality.toString(),
        '-f', 'image2pipe',
        '-'
      ];
    };

    const ffmpegProcess = spawn('ffmpeg', getFFmpegArgs(), {
      stdio: ['ignore', 'pipe', 'ignore']
    });

    let buffer = Buffer.alloc(0);

    // Batch QR processing
    const processQRBatch = async () => {
      if (processing || qrQueue.length === 0) return;
      processing = true;

      const batch = qrQueue.splice(0, decoder.settings.parallel);
      const qr = new QrCode();

      await Promise.all(batch.map(async ({ frame, id }) => {
        try {
          const image = await Jimp.read(frame);

          // Quality-based processing
          let prepared;
          if (decoder.settings.quality === 1) {
            // Turbo mode - minimal processing
            prepared = image.greyscale();
          } else if (decoder.settings.quality <= 3) {
            // Fast mode
            prepared = image.greyscale().contrast(0.2);
          } else {
            // Quality mode
            prepared = image.greyscale().contrast(0.3).brightness(0.1);
          }

          await new Promise((resolve) => {
            qr.callback = async (err, value) => {
              if (!err && value && value.result) {
                await decoder.processQRData(value.result, id / decoder.settings.fps);
              }
              resolve();
            };
            qr.decode(prepared.bitmap);
          });
        } catch (error) {
          // Ignore errors in turbo mode
        }
      }));

      processing = false;
      if (qrQueue.length > 0) {
        setImmediate(processQRBatch);
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

        // Frame skipping
        if (decoder.settings.skipFrames > 0) {
          skipCounter++;
          if (skipCounter % (decoder.settings.skipFrames + 1) !== 0) {
            frameStart = jpegEnd + 2;
            continue;
          }
        }

        frameId++;
        decoder.frameCount = frameId;

        qrQueue.push({ frame, id: frameId });

        if (!processing) {
          setImmediate(processQRBatch);
        }

        // Update stats
        if (frameId % 10 === 0) {
          decoder.updateStats();
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
      while (qrQueue.length > 0 || processing) {
        await new Promise(resolve => setTimeout(resolve, 100));
      }

      console.log('\n');
      if (code === 0) resolve();
      else reject(new Error(`FFmpeg exited with code ${code}`));
    });

    ffmpegProcess.on('error', reject);
  });
}

// Main
async function main(videoPath, options) {
  console.log('\n🚀 QRF TURBO DECODER v2.0\n');
  console.log(`📹 Video: ${videoPath}`);
  console.log(`📁 Output: ${options.output}`);
  console.log(`⚡ Initial FPS: ${options.fps}`);
  console.log(`🎯 Quality: ${options.quality}/5\n`);

  // Get video info
  const metadata = await new Promise((resolve, reject) => {
    ffmpeg.ffprobe(videoPath, (err, data) => {
      if (err) reject(err);
      else resolve(data);
    });
  });

  const duration = metadata.format.duration;
  console.log(`📊 Duration: ${duration.toFixed(1)}s (~${Math.floor(duration * options.fps)} frames)\n`);

  // Create output dir
  await fs.mkdir(options.output, { recursive: true });

  // Initialize decoder
  const decoder = new TurboDecoder();
  decoder.outputDir = options.output;
  decoder.settings.fps = options.fps;
  decoder.settings.quality = options.quality;

  // Check for turbo flag
  if (options.turbo) {
    decoder.settings.turboMode = true;
    decoder.settings.quality = 1;
    decoder.settings.scale = 360;
    decoder.settings.parallel = os.cpus().length;
    console.log(`🚀 TURBO MODE ENABLED!\n`);
  }

  console.log('🔥 Processing... (use keyboard controls to adjust)\n');

  // Process video
  await processTurbo(videoPath, decoder);

  // Final report
  const elapsed = (Date.now() - decoder.startTime) / 1000;
  console.log('\n' + '═'.repeat(70));
  console.log('📊 FINAL REPORT');
  console.log('─'.repeat(70));
  console.log(`Time: ${elapsed.toFixed(1)}s`);
  console.log(`Frames: ${decoder.frameCount}`);
  console.log(`Average speed: ${decoder.stats.currentFps.toFixed(1)} fps`);
  console.log(`Peak speed: ${decoder.stats.peakFps.toFixed(1)} fps`);
  console.log(`QR codes: ${decoder.qrDecoded}`);
  console.log(`Files found: ${decoder.discoveredFiles.size}`);

  let completed = 0;
  for (const [fileName, fileInfo] of decoder.discoveredFiles) {
    if (fileInfo.completed) completed++;
    const status = fileInfo.completed ? '✅' : '⏳';
    console.log(`${status} ${fileName}: ${fileInfo.recoveredChunks}/${fileInfo.metadata.chunksCount}`);
  }

  console.log(`\n✅ Completed: ${completed}/${decoder.discoveredFiles.size} files`);
  console.log('═'.repeat(70) + '\n');

  process.exit(0);
}

// CLI
if (process.argv.length < 3) {
  console.log('Usage: node decoder-turbo.js <video> [options]');
  console.log('\nOptions:');
  console.log('  --fps <rate>      Initial FPS (default: 20)');
  console.log('  --quality <1-5>   Quality level (default: 3)');
  console.log('  --output <dir>    Output directory (default: ./decoded)');
  console.log('  --turbo          Start in TURBO mode\n');
  console.log('Interactive controls available during processing!');
  process.exit(1);
}

const videoPath = process.argv[2];
const options = {
  fps: 20,
  quality: 3,
  output: './decoded',
  turbo: false
};

// Parse options
for (let i = 3; i < process.argv.length; i++) {
  if (process.argv[i] === '--fps' && process.argv[i + 1]) {
    options.fps = parseInt(process.argv[i + 1]);
    i++;
  } else if (process.argv[i] === '--quality' && process.argv[i + 1]) {
    options.quality = parseInt(process.argv[i + 1]);
    i++;
  } else if (process.argv[i] === '--output' && process.argv[i + 1]) {
    options.output = process.argv[i + 1];
    i++;
  } else if (process.argv[i] === '--turbo') {
    options.turbo = true;
  }
}

main(videoPath, options).catch(console.error);