#!/usr/bin/env node
import { program } from 'commander';
import { VideoProcessor, QRDecoder, FountainDecoder } from '@qrf/core';
import fs from 'fs/promises';
import path from 'path';
import process from 'process';
import chalk from 'chalk';
import ora from 'ora';

const decodeVideo = async (videoPath, options) => {
  console.log(chalk.cyan.bold('\n🎬 QRF Decoder v1.0.0\n'));
  console.log(chalk.white(`📹 Input:  ${chalk.yellow(videoPath)}`));
  console.log(chalk.white(`📁 Output: ${chalk.green(options.output)}`));
  console.log(chalk.gray(`⚙️  Settings: FPS=${options.fps}, Mode=${options.fast ? 'fast' : 'normal'}`));
  console.log(chalk.gray('\n' + '─'.repeat(50) + '\n'));

  const startTime = Date.now();
  const spinner = ora();
  const discoveredFiles = new Map();
  const decoders = new Map();

  try {
    // Ensure output directory exists
    await fs.mkdir(options.output, { recursive: true });

    spinner.start('Processing video...');

    const processor = new VideoProcessor(videoPath, {
      frameRate: parseFloat(options.fps) || 1,
      fastScan: options.fast
    });

    const qrDecoder = new QRDecoder();
    let processedFrames = 0;
    let lastUpdate = Date.now();

    processor.on('frame', async (frameData) => {
      processedFrames++;

      // Decode QR from frame
      const qrData = await qrDecoder.decode(frameData);

      if (qrData) {
        await handleQRData(qrData, discoveredFiles, decoders, options.output);
      }

      // Update spinner periodically
      if (Date.now() - lastUpdate > 100) {
        const progress = processor.processedFrames / processor.totalFrames;
        spinner.text = `Processing frames... ${Math.round(progress * 100)}% (${processedFrames} frames)`;
        lastUpdate = Date.now();
      }
    });

    processor.on('complete', () => {
      spinner.succeed('Video processing complete');
    });

    processor.on('error', (error) => {
      spinner.fail(`Error: ${error.message}`);
    });

    await processor.start();

    // Display results
    console.log(chalk.gray('\n' + '─'.repeat(50)));
    console.log(chalk.green.bold('\n✅ Decoding complete!\n'));
    console.log(chalk.white.bold('📊 Discovered Files:'));

    for (const [fileName, fileInfo] of discoveredFiles) {
      const status = fileInfo.completed ? chalk.green('✓') : chalk.yellow('○');
      const progress = fileInfo.decoder ?
        `${fileInfo.decoder.recoveredChunkCount}/${fileInfo.totalChunks}` :
        '0/0';
      console.log(`   ${status} ${fileName} - ${progress} chunks`);

      if (fileInfo.completed && fileInfo.outputPath) {
        console.log(chalk.gray(`      Saved to: ${fileInfo.outputPath}`));
      }
    }

    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);
    console.log(chalk.cyan(`\n⏱️  Total time: ${elapsed} seconds\n`));

  } catch (error) {
    spinner.fail();
    console.error(chalk.red('\n❌ Decoding error:'), error.message);
    if (options.verbose) {
      console.error(chalk.gray(error.stack));
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
        totalChunks: data.chunksCount,
        decoder: decoder,
        completed: false
      });

      decoders.set(data.fileName, decoder);

      console.log(chalk.blue(`\n📄 Discovered: ${data.fileName} (${data.chunksCount} chunks)`));
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

    if (targetDecoder) {
      const wasAdded = targetDecoder.addPacket(data);

      if (wasAdded) {
        const progress = targetDecoder.getRecoveryProgress();

        // Check if file is complete
        if (progress.recovered === progress.total && !discoveredFiles.get(targetFileName).completed) {
          const fileData = targetDecoder.finalizeFile();

          if (fileData) {
            const outputPath = path.join(outputDir, targetFileName);
            await fs.writeFile(outputPath, fileData);

            discoveredFiles.get(targetFileName).completed = true;
            discoveredFiles.get(targetFileName).outputPath = outputPath;

            console.log(chalk.green(`\n✓ Recovered: ${targetFileName}`));
          }
        }
      }
    }
  }
};

const scanVideo = async (videoPath, options) => {
  console.log(chalk.cyan.bold('\n🔍 QRF Scanner v1.0.0\n'));
  console.log(chalk.white(`📹 Scanning: ${chalk.yellow(videoPath)}`));

  const spinner = ora('Fast scanning for metadata...').start();
  const discoveredFiles = [];

  try {
    const processor = new VideoProcessor(videoPath, {
      frameRate: 0.5, // Scan at 0.5 fps for metadata
      fastScan: true
    });

    const qrDecoder = new QRDecoder();

    processor.on('frame', async (frameData) => {
      const qrData = await qrDecoder.decode(frameData);

      if (qrData && qrData.type === 'metadata') {
        const exists = discoveredFiles.find(f => f.fileName === qrData.fileName);
        if (!exists) {
          discoveredFiles.push({
            fileName: qrData.fileName,
            fileSize: qrData.fileSize,
            fileType: qrData.fileType,
            chunksCount: qrData.chunksCount,
            timestamp: frameData.timestamp
          });
          spinner.text = `Found ${discoveredFiles.length} file(s)...`;
        }
      }
    });

    await processor.start();
    spinner.succeed(`Scan complete - found ${discoveredFiles.length} file(s)`);

    // Save to JSON
    const scanData = {
      videoPath: videoPath,
      scanDate: new Date().toISOString(),
      files: discoveredFiles
    };

    await fs.writeFile(options.output, JSON.stringify(scanData, null, 2));
    console.log(chalk.green(`\n✓ Scan data saved to: ${options.output}\n`));

    // Display discovered files
    console.log(chalk.white.bold('📊 Discovered Files:'));
    for (const file of discoveredFiles) {
      console.log(`   • ${file.fileName} (${(file.fileSize / 1024).toFixed(1)} KB)`);
      console.log(chalk.gray(`     ${file.chunksCount} chunks, found at ${file.timestamp.toFixed(1)}s`));
    }

  } catch (error) {
    spinner.fail();
    console.error(chalk.red('Scan error:'), error.message);
    process.exit(1);
  }
};

// CLI setup
program
  .name('qrf-decoder')
  .description('QR Code File Decoder - Decode QR videos back to files')
  .version('1.0.0');

program
  .command('decode <video>')
  .description('Decode QR codes from video file')
  .option('-f, --fps <rate>', 'Frame processing rate', '1')
  .option('--fast', 'Fast scan mode', false)
  .option('-o, --output <dir>', 'Output directory', './decoded')
  .option('-v, --verbose', 'Verbose output', false)
  .action(decodeVideo);

program
  .command('scan <video>')
  .description('Fast scan to discover files in video')
  .option('-o, --output <file>', 'Output JSON file', 'scan.json')
  .action(scanVideo);

program
  .command('extract <video> <file>')
  .description('Extract specific file from video')
  .option('-o, --output <dir>', 'Output directory', './decoded')
  .option('-j, --json <file>', 'Use scan data from JSON')
  .action(async (video, fileName, options) => {
    console.log(chalk.cyan(`\n📤 Extracting ${fileName} from video...\n`));
    // Targeted extraction logic here
    console.log(chalk.yellow('Targeted extraction coming soon...'));
  });

program.parse();