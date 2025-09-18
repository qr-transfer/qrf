#!/usr/bin/env node
import { program } from 'commander';
import { VideoProcessor } from './src/videoProcessorSimple.js';
import { QRDecoder } from './src/qrDecoder.js';
import { FountainDecoder } from './src/fountainDecoder.js';
import fs from 'fs/promises';
import path from 'path';

const decodeVideo = async (videoPath, options) => {
  console.log('\n🎬 QRF Decoder v1.0.0\n');
  console.log(`📹 Input:  ${videoPath}`);
  console.log(`📁 Output: ${options.output}`);
  console.log(`⚙️  Settings: FPS=${options.fps}\n`);
  console.log('─'.repeat(50) + '\n');

  const startTime = Date.now();
  const discoveredFiles = new Map();
  const decoders = new Map();

  try {
    // Ensure output directory exists
    await fs.mkdir(options.output, { recursive: true });

    console.log('📡 Processing video...\n');

    const processor = new VideoProcessor(videoPath, {
      frameRate: parseFloat(options.fps) || 10,
      fastScan: options.fast
    });

    const qrDecoder = new QRDecoder();
    let processedFrames = 0;
    let decodedQRs = 0;
    let lastProgressUpdate = Date.now();

    processor.on('frame', async (frameData) => {
      processedFrames++;

      try {
        // Decode QR from frame
        const qrData = await qrDecoder.decode(frameData);

        if (qrData) {
          decodedQRs++;
          await handleQRData(qrData, discoveredFiles, decoders, options.output);

          // Show QR decode success
          if (options.verbose) {
            console.log(`  Frame ${processedFrames}: ${qrData.type} packet decoded`);
          }
        }
      } catch (error) {
        if (options.verbose) {
          console.log(`  Frame ${processedFrames}: Decode error - ${error.message}`);
        }
      }

      // Update progress periodically
      if (Date.now() - lastProgressUpdate > 1000) {
        const progress = processor.processedFrames / processor.totalFrames;
        process.stdout.write(`\r⏳ Progress: ${Math.round(progress * 100)}% | Frames: ${processedFrames} | QRs decoded: ${decodedQRs}`);
        lastProgressUpdate = Date.now();
      }
    });

    processor.on('complete', () => {
      console.log('\n✅ Video processing complete\n');
    });

    processor.on('error', (error) => {
      console.error(`\n❌ Processing error: ${error.message}`);
    });

    await processor.start();

    // Display results
    console.log('\n' + '─'.repeat(50));
    console.log('\n📊 Results:\n');

    let recoveredCount = 0;
    for (const [fileName, fileInfo] of discoveredFiles) {
      const status = fileInfo.completed ? '✅' : '⏳';
      const progress = fileInfo.decoder ?
        `${fileInfo.decoder.recoveredChunkCount}/${fileInfo.totalChunks}` :
        '0/0';

      console.log(`${status} ${fileName}`);
      console.log(`   Chunks: ${progress} | Size: ${(fileInfo.fileSize / 1024).toFixed(1)} KB`);

      if (fileInfo.completed) {
        recoveredCount++;
        if (fileInfo.outputPath) {
          console.log(`   📁 Saved to: ${fileInfo.outputPath}`);
        }
      } else {
        console.log(`   ⚠️  Incomplete - ${Math.round((fileInfo.decoder?.recoveredChunkCount || 0) / fileInfo.totalChunks * 100)}% recovered`);
      }
      console.log();
    }

    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    console.log('─'.repeat(50));
    console.log(`\n📈 Statistics:`);
    console.log(`   • Processed frames: ${processedFrames}`);
    console.log(`   • Decoded QR codes: ${decodedQRs}`);
    console.log(`   • Discovered files: ${discoveredFiles.size}`);
    console.log(`   • Recovered files: ${recoveredCount}`);
    console.log(`   • Processing time: ${elapsed}s`);
    console.log(`   • Average FPS: ${(processedFrames / elapsed).toFixed(1)}`);

  } catch (error) {
    console.error('\n❌ Fatal error:', error.message);
    if (options.verbose) {
      console.error(error.stack);
    }
    process.exit(1);
  }
};

const handleQRData = async (data, discoveredFiles, decoders, outputDir) => {
  if (data.type === 'metadata') {
    // New file discovered
    if (!discoveredFiles.has(data.fileName)) {
      const decoder = new FountainDecoder();
      decoder.initialize(data);

      discoveredFiles.set(data.fileName, {
        fileName: data.fileName,
        fileSize: data.fileSize,
        fileType: data.fileType || 'unknown',
        totalChunks: data.chunksCount,
        decoder: decoder,
        completed: false,
        startTime: Date.now()
      });

      decoders.set(data.fileName, decoder);

      console.log(`\n📄 Discovered: ${data.fileName}`);
      console.log(`   Type: ${data.fileType || 'unknown'}`);
      console.log(`   Size: ${(data.fileSize / 1024).toFixed(1)} KB`);
      console.log(`   Chunks: ${data.chunksCount}\n`);
    }
  } else if (data.type === 'data') {
    // Find the appropriate decoder
    let targetDecoder = null;
    let targetFileName = null;

    // Try to find decoder by fileId
    if (data.fileId) {
      for (const [fileName, fileInfo] of discoveredFiles) {
        if (fileInfo.decoder && fileInfo.decoder.metaData) {
          const fileChecksum = fileInfo.decoder.metaData.fileChecksum;
          if (fileChecksum && fileChecksum.substring(0, 8) === data.fileId) {
            targetDecoder = fileInfo.decoder;
            targetFileName = fileName;
            break;
          }
        }
      }
    }

    // Fallback: if only one file is being decoded, use it
    if (!targetDecoder && discoveredFiles.size === 1) {
      const entry = discoveredFiles.entries().next().value;
      targetFileName = entry[0];
      targetDecoder = entry[1].decoder;
    }

    if (targetDecoder && !discoveredFiles.get(targetFileName).completed) {
      // Convert data packet to format expected by decoder
      const packet = {
        degree: data.degree || 1,
        sourceIndices: data.sourceIndices || [data.packetId || 0],
        data: data.data
      };

      const wasAdded = targetDecoder.addPacket(packet);

      if (wasAdded) {
        const progress = targetDecoder.getRecoveryProgress();

        // Update progress periodically
        if (progress.recovered % 10 === 0 || progress.recovered === progress.total) {
          process.stdout.write(`\r📦 ${targetFileName}: ${progress.recovered}/${progress.total} chunks (${progress.percentage}%)`);
        }

        // Check if file is complete
        if (progress.recovered === progress.total && !discoveredFiles.get(targetFileName).completed) {
          const fileData = targetDecoder.finalizeFile();

          if (fileData) {
            const outputPath = path.join(outputDir, targetFileName);
            await fs.writeFile(outputPath, fileData);

            const fileInfo = discoveredFiles.get(targetFileName);
            fileInfo.completed = true;
            fileInfo.outputPath = outputPath;
            fileInfo.completionTime = Date.now();

            const elapsed = ((fileInfo.completionTime - fileInfo.startTime) / 1000).toFixed(1);
            console.log(`\n\n✅ Recovered: ${targetFileName} in ${elapsed}s`);
            console.log(`   📁 Saved to: ${outputPath}\n`);
          } else {
            console.log(`\n⚠️  Warning: ${targetFileName} recovery failed - checksum mismatch\n`);
          }
        }
      }
    }
  }
};

// CLI setup
program
  .name('qrf-decoder-simple')
  .description('QR Code File Decoder - Simple standalone version')
  .version('1.0.0');

program
  .argument('<video>', 'Video file to decode')
  .option('-f, --fps <rate>', 'Frame processing rate', '10')
  .option('--fast', 'Fast scan mode', false)
  .option('-o, --output <dir>', 'Output directory', './decoded')
  .option('-v, --verbose', 'Verbose output', false)
  .action(decodeVideo);

program.parse();