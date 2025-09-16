#!/usr/bin/env node
import { program } from 'commander';
import { VideoEncoder } from './src/videoEncoder.js';
import { QRGenerator } from './src/qrGenerator.js';
import { FountainEncoder } from './src/fountainEncoder.js';
import { FileProcessor } from './src/fileProcessor.js';
import fs from 'fs/promises';
import path from 'path';
import process from 'process';

const encodeFile = async (inputFile, outputVideo, options) => {
  console.log('\n🎬 QRF Encoder v1.0.0\n');
  console.log(`📄 Input:  ${inputFile}`);
  console.log(`📹 Output: ${outputVideo}`);
  console.log(`⚙️  Settings: FPS=${options.fps}, Density=${options.density}, Redundancy=${options.redundancy}`);
  console.log('\n' + '─'.repeat(50) + '\n');

  const startTime = Date.now();

  try {
    // Read and process input file
    console.log('📖 Reading file...');
    const fileProcessor = new FileProcessor();
    const fileData = await fileProcessor.readFile(inputFile);
    console.log(`   File size: ${(fileData.size / 1024).toFixed(1)} KB`);

    // Split into chunks
    console.log('🔪 Chunking file...');
    const chunks = await fileProcessor.splitIntoChunks(fileData.buffer, {
      chunkSize: parseInt(options.chunkSize) || 1024
    });
    console.log(`   Created ${chunks.length} chunks`);

    // Generate fountain packets
    console.log('💧 Generating fountain packets...');
    const fountainEncoder = new FountainEncoder();
    const packets = await fountainEncoder.encode(chunks, {
      redundancy: parseFloat(options.redundancy) || 1.5,
      systematic: true
    });
    console.log(`   Generated ${packets.length} packets (${options.redundancy}× redundancy)`);

    // Calculate file checksum
    const fileChecksum = await fileProcessor.calculateChecksum(fileData.buffer);

    // Generate metadata
    const metadata = {
      fileName: path.basename(inputFile),
      fileType: fileData.mimeType,
      fileSize: fileData.size,
      chunksCount: chunks.length,
      packetCount: packets.length,
      fileChecksum: fileChecksum,
      encoderVersion: '4.0'
    };

    // Generate QR codes
    console.log('🔲 Generating QR codes...');
    const qrGenerator = new QRGenerator({
      errorCorrection: options.errorCorrection || 'L',
      density: options.density || 'high'
    });

    const qrFrames = [];

    // Add metadata QR codes (repeat for reliability)
    const metadataQR = await qrGenerator.generateMetadata(metadata);
    for (let i = 0; i < 10; i++) {
      qrFrames.push(metadataQR);
    }

    // Progress display for QR generation
    process.stdout.write('   Progress: ');
    let lastPercent = 0;

    // Add data packet QR codes
    for (let i = 0; i < packets.length; i++) {
      const qrCode = await qrGenerator.generateDataPacket(packets[i], metadata);
      qrFrames.push(qrCode);

      // Update progress
      const percent = Math.floor((i + 1) / packets.length * 100);
      if (percent !== lastPercent && percent % 10 === 0) {
        process.stdout.write(`${percent}%...`);
        lastPercent = percent;
      }
    }
    process.stdout.write('100%\n');
    console.log(`   Total QR codes: ${qrFrames.length}`);

    // Create video from QR codes
    console.log('🎥 Encoding video...');
    const videoEncoder = new VideoEncoder({
      fps: parseInt(options.fps) || 10,
      width: parseInt(options.width) || 1080,
      height: parseInt(options.height) || 1080,
      outputPath: outputVideo
    });

    await videoEncoder.createVideo(qrFrames, (videoProgress) => {
      const percent = Math.floor(videoProgress * 100);
      process.stdout.write(`\r   Video encoding: ${percent}%`);
    });
    process.stdout.write('\n');

    // Calculate final stats
    const duration = qrFrames.length / (parseInt(options.fps) || 10);
    const outputStats = await fs.stat(outputVideo);

    console.log('\n' + '─'.repeat(50));
    console.log('\n✅ Encoding complete!\n');
    console.log('📊 Final Statistics:');
    console.log(`   • Input size:  ${(fileData.size / 1024).toFixed(1)} KB`);
    console.log(`   • Output size: ${(outputStats.size / (1024 * 1024)).toFixed(1)} MB`);
    console.log(`   • Duration:    ${duration.toFixed(1)} seconds`);
    console.log(`   • Chunks:      ${chunks.length}`);
    console.log(`   • Packets:     ${packets.length}`);
    console.log(`   • QR Codes:    ${qrFrames.length}`);

    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    console.log(`\n⏱️  Total time: ${elapsed} seconds\n`);

  } catch (error) {
    console.error('\n❌ Encoding error:', error.message);
    process.exit(1);
  }
};

// CLI setup
program
  .name('qrf-encoder')
  .description('QR Code File Encoder CLI - Encode files into QR video')
  .version('1.0.0');

program
  .command('encode <input> <output>')
  .description('Encode file into QR video')
  .option('-f, --fps <rate>', 'Video frame rate', '10')
  .option('-c, --chunk-size <size>', 'Chunk size in bytes', '1024')
  .option('-r, --redundancy <factor>', 'Redundancy factor for fountain codes', '1.5')
  .option('-d, --density <level>', 'QR code density (low/medium/high)', 'high')
  .option('-e, --error-correction <level>', 'Error correction level (L/M/Q/H)', 'L')
  .option('-w, --width <pixels>', 'Video width', '1080')
  .option('-h, --height <pixels>', 'Video height', '1080')
  .option('--codec <codec>', 'Video codec (h264/h265/vp9)', 'h264')
  .action(async (input, output, options) => {
    try {
      await fs.access(input, fs.constants.R_OK);
      await encodeFile(input, output, options);
    } catch (error) {
      if (error.code === 'ENOENT') {
        console.error(`❌ Error: Cannot read input file: ${input}`);
      } else {
        console.error(`❌ Error: ${error.message}`);
      }
      process.exit(1);
    }
  });

program.parse();