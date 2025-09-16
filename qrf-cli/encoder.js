#!/usr/bin/env node
import React, { useState, useEffect, useRef } from 'react';
import { render, Box, Text, useApp, useInput } from 'ink';
import { Badge, ProgressBar, StatusMessage, Spinner } from '@inkjs/ui';
import { program } from 'commander';
import { VideoEncoder } from './src/videoEncoder.js';
import { QRGenerator } from './src/qrGenerator.js';
import { FountainEncoder } from './src/fountainEncoder.js';
import { FileProcessor } from './src/fileProcessor.js';
import fs from 'fs/promises';
import path from 'path';

// Main Encoder CLI Component
const QRFEncoder = ({ inputFile, outputVideo, options }) => {
  const { exit } = useApp();
  const [status, setStatus] = useState('initializing');
  const [progress, setProgress] = useState(0);
  const [currentPhase, setCurrentPhase] = useState('');
  const [stats, setStats] = useState({
    fileSize: 0,
    chunks: 0,
    packets: 0,
    qrCodes: 0,
    fps: options.fps || 10,
    density: options.density || 'high',
    redundancy: options.redundancy || 1.5
  });
  const [errors, setErrors] = useState([]);
  const [elapsedTime, setElapsedTime] = useState(0);
  const startTimeRef = useRef(Date.now());

  // Handle keyboard input
  useInput((input, key) => {
    if (input === 'q' || key.escape) {
      setStatus('cancelled');
      exit();
    }
  });

  useEffect(() => {
    const timer = setInterval(() => {
      setElapsedTime(Math.floor((Date.now() - startTimeRef.current) / 1000));
    }, 1000);

    // Start encoding process
    encodeFile();

    return () => clearInterval(timer);
  }, []);

  const encodeFile = async () => {
    try {
      setStatus('processing');
      setCurrentPhase('Reading file');

      // Read and process input file
      const fileProcessor = new FileProcessor();
      const fileData = await fileProcessor.readFile(inputFile);

      setStats(prev => ({
        ...prev,
        fileSize: fileData.size,
        fileName: path.basename(inputFile)
      }));

      // Split into chunks
      setCurrentPhase('Chunking file');
      const chunks = await fileProcessor.splitIntoChunks(fileData.buffer, {
        chunkSize: options.chunkSize || 1024
      });

      setStats(prev => ({
        ...prev,
        chunks: chunks.length
      }));

      // Generate fountain packets
      setCurrentPhase('Generating fountain packets');
      const fountainEncoder = new FountainEncoder();
      const packets = await fountainEncoder.encode(chunks, {
        redundancy: options.redundancy || 1.5,
        systematic: true
      });

      setStats(prev => ({
        ...prev,
        packets: packets.length
      }));

      // Generate metadata
      const metadata = {
        fileName: path.basename(inputFile),
        fileType: fileData.mimeType,
        fileSize: fileData.size,
        chunksCount: chunks.length,
        packetCount: packets.length,
        fileChecksum: await fileProcessor.calculateChecksum(fileData.buffer),
        encoderVersion: '4.0'
      };

      // Generate QR codes
      setCurrentPhase('Generating QR codes');
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

      // Add data packet QR codes
      let packetIndex = 0;
      for (const packet of packets) {
        const qrCode = await qrGenerator.generateDataPacket(packet, metadata);
        qrFrames.push(qrCode);
        packetIndex++;

        // Update progress
        setProgress(packetIndex / packets.length * 0.7); // 70% for QR generation

        setStats(prev => ({
          ...prev,
          qrCodes: packetIndex + 10 // Include metadata frames
        }));
      }

      // Create video from QR codes
      setCurrentPhase('Encoding video');
      const videoEncoder = new VideoEncoder({
        fps: options.fps || 10,
        width: options.width || 1080,
        height: options.height || 1080,
        outputPath: outputVideo
      });

      await videoEncoder.createVideo(qrFrames, (videoProgress) => {
        setProgress(0.7 + videoProgress * 0.3); // Last 30% for video encoding
      });

      setStatus('completed');
      setCurrentPhase('Done!');
      setProgress(1);

      // Calculate final stats
      const duration = qrFrames.length / (options.fps || 10);
      const bitrate = (fileData.size * 8) / duration;
      const outputSize = await getFileSize(outputVideo);

      setStats(prev => ({
        ...prev,
        duration: duration,
        bitrate: bitrate,
        outputSize: outputSize
      }));

    } catch (error) {
      setStatus('error');
      setErrors(prev => [...prev, error.message]);
      console.error('Encoding error:', error);
    }
  };

  const getFileSize = async (filePath) => {
    try {
      const stats = await fs.stat(filePath);
      return stats.size;
    } catch {
      return 0;
    }
  };

  const formatTime = (seconds) => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  const formatSize = (bytes) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  return (
    <Box flexDirection="column">
      <Box marginBottom={1}>
        <Text bold color="cyan">QRF Encoder CLI</Text>
        <Text> - </Text>
        <Text dimColor>v1.0.0</Text>
      </Box>

      <Box marginBottom={1} flexDirection="column">
        <Box>
          <Text bold>Input: </Text>
          <Text color="yellow">{stats.fileName || inputFile}</Text>
          <Text> ({formatSize(stats.fileSize)})</Text>
        </Box>
        <Box>
          <Text bold>Output: </Text>
          <Text color="green">{outputVideo}</Text>
        </Box>
      </Box>

      <Box marginBottom={1}>
        <Badge color={
          status === 'error' ? 'red' :
          status === 'completed' ? 'green' :
          status === 'cancelled' ? 'yellow' :
          'cyan'
        }>
          {status.toUpperCase()}
        </Badge>
        <Text> </Text>
        {status === 'processing' && <Spinner />}
        <Text> </Text>
        <Text dimColor>{currentPhase}</Text>
      </Box>

      {status === 'processing' && (
        <Box marginBottom={1}>
          <Text>Progress: </Text>
          <Box width={40}>
            <ProgressBar value={progress} />
          </Box>
          <Text> {Math.round(progress * 100)}%</Text>
        </Box>
      )}

      <Box flexDirection="column" marginBottom={1}>
        <Text bold underline>Encoding Stats:</Text>
        <Box marginLeft={2} flexDirection="column">
          <Text>• Chunks: {stats.chunks}</Text>
          <Text>• Packets: {stats.packets} (×{stats.redundancy} redundancy)</Text>
          <Text>• QR Codes: {stats.qrCodes}</Text>
          <Text>• FPS: {stats.fps}</Text>
          <Text>• Density: {stats.density}</Text>
          {stats.duration && <Text>• Duration: {stats.duration.toFixed(1)}s</Text>}
          {stats.outputSize && <Text>• Output Size: {formatSize(stats.outputSize)}</Text>}
        </Box>
      </Box>

      <Box marginBottom={1}>
        <Text>Elapsed: {formatTime(elapsedTime)}</Text>
      </Box>

      {errors.length > 0 && (
        <Box flexDirection="column" marginTop={1}>
          <Text bold color="red">Errors:</Text>
          {errors.map((error, i) => (
            <Text key={i} color="red" marginLeft={2}>• {error}</Text>
          ))}
        </Box>
      )}

      {status === 'completed' && (
        <Box marginTop={1} borderStyle="single" paddingX={1}>
          <Text color="green">✓ Video created successfully!</Text>
        </Box>
      )}

      <Box marginTop={1} borderStyle="single" paddingX={1}>
        <Text dimColor>Press 'q' to {status === 'processing' ? 'cancel' : 'quit'}</Text>
      </Box>
    </Box>
  );
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
  .option('--preview', 'Show preview window', false)
  .action((input, output, options) => {
    // Validate input file exists
    fs.access(input, fs.constants.R_OK)
      .then(() => {
        const app = render(<QRFEncoder
          inputFile={input}
          outputVideo={output}
          options={options}
        />);

        app.waitUntilExit().then(() => {
          console.log('\nEncoding complete!');
          process.exit(0);
        }).catch(error => {
          console.error('Error:', error);
          process.exit(1);
        });
      })
      .catch(() => {
        console.error(`Error: Cannot read input file: ${input}`);
        process.exit(1);
      });
  });

program
  .command('batch <directory> <output-dir>')
  .description('Encode multiple files in a directory')
  .option('-f, --fps <rate>', 'Video frame rate', '10')
  .option('-p, --pattern <glob>', 'File pattern to match', '*')
  .action(async (directory, outputDir, options) => {
    console.log(`Batch encoding files from ${directory} to ${outputDir}`);
    // Batch processing logic here
  });

program.parse();