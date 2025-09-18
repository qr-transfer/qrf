#!/usr/bin/env node
import { program } from 'commander';
import { VideoEncoder, QRGenerator, FountainEncoder, FileProcessor } from '@qrf/core';
import fs from 'fs/promises';
import path from 'path';
import process from 'process';
import chalk from 'chalk';

const encodeFile = async (inputFile, outputVideo, options) => {
  console.log(chalk.cyan.bold('\n🎬 QRF Encoder v1.0.0\n'));
  console.log(chalk.white(`📄 Input:  ${chalk.yellow(inputFile)}`));
  console.log(chalk.white(`📹 Output: ${chalk.green(outputVideo)}`));
  console.log(chalk.gray(`⚙️  Settings: FPS=${options.fps}, Density=${options.density}, Redundancy=${options.redundancy}`));
  console.log(chalk.gray('\n' + '─'.repeat(50) + '\n'));

  const startTime = Date.now();

  try {
    // Read and process input file
    console.log(chalk.blue('📖 Reading file...'));
    const fileProcessor = new FileProcessor();
    const fileData = await fileProcessor.readFile(inputFile);
    console.log(chalk.gray(`   File size: ${(fileData.size / 1024).toFixed(1)} KB`));

    // Split into chunks
    console.log(chalk.blue('🔪 Chunking file...'));
    const chunks = await fileProcessor.splitIntoChunks(fileData.buffer, {
      chunkSize: parseInt(options.chunkSize) || 1024
    });
    console.log(chalk.gray(`   Created ${chunks.length} chunks`));

    // Generate fountain packets
    console.log(chalk.blue('💧 Generating fountain packets...'));
    const fountainEncoder = new FountainEncoder();
    const packets = await fountainEncoder.encode(chunks, {
      redundancy: parseFloat(options.redundancy) || 1.5,
      systematic: true
    });
    console.log(chalk.gray(`   Generated ${packets.length} packets (${options.redundancy}× redundancy)`));

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
    console.log(chalk.blue('🔲 Generating QR codes...'));
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
    process.stdout.write(chalk.gray('   Progress: '));
    let lastPercent = 0;

    // Add data packet QR codes
    for (let i = 0; i < packets.length; i++) {
      const qrCode = await qrGenerator.generateDataPacket(packets[i], metadata);
      qrFrames.push(qrCode);

      // Update progress
      const percent = Math.floor((i + 1) / packets.length * 100);
      if (percent !== lastPercent && percent % 10 === 0) {
        process.stdout.write(chalk.cyan(`${percent}%...`));
        lastPercent = percent;
      }
    }
    process.stdout.write(chalk.green('100%\n'));
    console.log(chalk.gray(`   Total QR codes: ${qrFrames.length}`));

    // Create video from QR codes
    console.log(chalk.blue('🎥 Encoding video...'));
    const videoEncoder = new VideoEncoder({
      fps: parseInt(options.fps) || 10,
      width: parseInt(options.width) || 1080,
      height: parseInt(options.height) || 1080,
      outputPath: outputVideo,
      codec: options.codec || 'libx264'
    });

    await videoEncoder.createVideo(qrFrames, (videoProgress) => {
      const percent = Math.floor(videoProgress * 100);
      process.stdout.write(`\r   ${chalk.cyan('Video encoding:')} ${chalk.yellow(percent + '%')}`);
    });
    process.stdout.write('\n');

    // Calculate final stats
    const duration = qrFrames.length / (parseInt(options.fps) || 10);
    const outputStats = await fs.stat(outputVideo);

    console.log(chalk.gray('\n' + '─'.repeat(50)));
    console.log(chalk.green.bold('\n✅ Encoding complete!\n'));
    console.log(chalk.white.bold('📊 Final Statistics:'));
    console.log(chalk.gray(`   • Input size:  ${(fileData.size / 1024).toFixed(1)} KB`));
    console.log(chalk.gray(`   • Output size: ${(outputStats.size / (1024 * 1024)).toFixed(1)} MB`));
    console.log(chalk.gray(`   • Duration:    ${duration.toFixed(1)} seconds`));
    console.log(chalk.gray(`   • Chunks:      ${chunks.length}`));
    console.log(chalk.gray(`   • Packets:     ${packets.length}`));
    console.log(chalk.gray(`   • QR Codes:    ${qrFrames.length}`));

    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    console.log(chalk.cyan(`\n⏱️  Total time: ${elapsed} seconds\n`));

  } catch (error) {
    console.error(chalk.red('\n❌ Encoding error:'), error.message);
    if (options.verbose) {
      console.error(chalk.gray(error.stack));
    }
    process.exit(1);
  }
};

// CLI setup
program
  .name('qrf-encoder')
  .description('QR Code File Encoder - Encode files into QR video with fountain codes')
  .version('1.0.0');

program
  .command('encode <input> <output>')
  .description('Encode file into QR video')
  .option('-f, --fps <rate>', 'Video frame rate', '10')
  .option('-c, --chunk-size <size>', 'Chunk size in bytes', '1024')
  .option('-r, --redundancy <factor>', 'Redundancy factor for fountain codes', '1.5')
  .option('-d, --density <level>', 'QR code density (low/medium/high/ultra)', 'high')
  .option('-e, --error-correction <level>', 'Error correction level (L/M/Q/H)', 'L')
  .option('-w, --width <pixels>', 'Video width', '1080')
  .option('-h, --height <pixels>', 'Video height', '1080')
  .option('--codec <codec>', 'Video codec (libx264/libx265/libvpx-vp9)', 'libx264')
  .option('-v, --verbose', 'Verbose output', false)
  .action(async (input, output, options) => {
    try {
      await fs.access(input, fs.constants.R_OK);
      await encodeFile(input, output, options);
    } catch (error) {
      if (error.code === 'ENOENT') {
        console.error(chalk.red(`❌ Error: Cannot read input file: ${input}`));
      } else {
        console.error(chalk.red(`❌ Error: ${error.message}`));
      }
      process.exit(1);
    }
  });

program
  .command('batch <directory> <output-dir>')
  .description('Encode multiple files in a directory')
  .option('-p, --pattern <glob>', 'File pattern to match', '*')
  .option('-f, --fps <rate>', 'Video frame rate', '10')
  .option('-r, --redundancy <factor>', 'Redundancy factor', '1.5')
  .action(async (directory, outputDir, options) => {
    console.log(chalk.cyan(`\n📁 Batch encoding files from ${directory} to ${outputDir}\n`));
    // Batch processing would be implemented here
    console.log(chalk.yellow('Batch processing coming soon...'));
  });

program.parse();