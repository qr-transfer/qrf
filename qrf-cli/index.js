#!/usr/bin/env node
import React, { useState, useEffect } from 'react';
import { render, Box, Text, useApp, useInput } from 'ink';
import { Badge, ProgressBar, StatusMessage, Spinner } from '@inkjs/ui';
import { program } from 'commander';
import { VideoProcessor } from './src/videoProcessor.js';
import { QRDecoder } from './src/qrDecoder.js';
import { FountainDecoder } from './src/fountainDecoder.js';

// Main CLI Component
const QRFDecoder = ({ videoPath, options }) => {
  const { exit } = useApp();
  const [status, setStatus] = useState('initializing');
  const [progress, setProgress] = useState(0);
  const [discoveredFiles, setDiscoveredFiles] = useState([]);
  const [currentFile, setCurrentFile] = useState(null);
  const [chunks, setChunks] = useState({ recovered: 0, total: 0 });
  const [fps, setFps] = useState(0);
  const [errors, setErrors] = useState([]);

  // Handle keyboard input
  useInput((input, key) => {
    if (input === 'q' || key.escape) {
      exit();
    }
    if (input === 'p') {
      // Pause/resume processing
    }
  });

  useEffect(() => {
    // Start processing
    processVideo();
  }, []);

  const processVideo = async () => {
    try {
      setStatus('processing');

      const processor = new VideoProcessor(videoPath, {
        frameRate: options.fps || 1,
        fastScan: options.fast
      });

      const decoder = new QRDecoder();
      const fountain = new FountainDecoder();

      processor.on('frame', async (frameData) => {
        // Process frame for QR codes
        const qrData = await decoder.decode(frameData);
        if (qrData) {
          handleQRData(qrData);
        }
      });

      processor.on('progress', (percent) => {
        setProgress(percent);
      });

      processor.on('fps', (rate) => {
        setFps(rate);
      });

      await processor.start();
      setStatus('completed');
    } catch (error) {
      setStatus('error');
      setErrors(prev => [...prev, error.message]);
    }
  };

  const handleQRData = (data) => {
    if (data.type === 'metadata') {
      // New file discovered
      setDiscoveredFiles(prev => {
        const exists = prev.find(f => f.name === data.fileName);
        if (!exists) {
          return [...prev, {
            name: data.fileName,
            size: data.fileSize,
            chunks: data.chunksCount,
            discovered: new Date()
          }];
        }
        return prev;
      });
      setCurrentFile(data.fileName);
    } else if (data.type === 'data') {
      // Chunk received
      setChunks(prev => ({
        ...prev,
        recovered: prev.recovered + 1
      }));
    }
  };

  return (
    <Box flexDirection="column">
      <Box marginBottom={1}>
        <Text bold color="cyan">QRF Decoder CLI</Text>
        <Text> - </Text>
        <Text dimColor>{videoPath}</Text>
      </Box>

      <Box marginBottom={1}>
        <Badge color={status === 'error' ? 'red' : status === 'completed' ? 'green' : 'yellow'}>
          {status.toUpperCase()}
        </Badge>
        <Text> </Text>
        {status === 'processing' && <Spinner />}
        <Text> FPS: {fps.toFixed(1)}</Text>
      </Box>

      {progress > 0 && (
        <Box marginBottom={1}>
          <Text>Progress: </Text>
          <ProgressBar value={progress} />
          <Text> {Math.round(progress * 100)}%</Text>
        </Box>
      )}

      {currentFile && (
        <Box marginBottom={1} flexDirection="column">
          <Text bold>Current File: {currentFile}</Text>
          <Text>Chunks: {chunks.recovered}/{chunks.total}</Text>
        </Box>
      )}

      {discoveredFiles.length > 0 && (
        <Box flexDirection="column" marginTop={1}>
          <Text bold underline>Discovered Files ({discoveredFiles.length}):</Text>
          {discoveredFiles.map((file, i) => (
            <Box key={i} marginLeft={2}>
              <Badge color={file.completed ? 'green' : 'yellow'}>
                {file.completed ? '✓' : '⧗'}
              </Badge>
              <Text> {file.name} ({Math.round(file.size / 1024)}KB)</Text>
            </Box>
          ))}
        </Box>
      )}

      {errors.length > 0 && (
        <Box flexDirection="column" marginTop={1}>
          <Text bold color="red">Errors:</Text>
          {errors.map((error, i) => (
            <Text key={i} color="red" marginLeft={2}>• {error}</Text>
          ))}
        </Box>
      )}

      <Box marginTop={1} borderStyle="single" paddingX={1}>
        <Text dimColor>Press 'q' to quit, 'p' to pause/resume</Text>
      </Box>
    </Box>
  );
};

// CLI setup
program
  .name('qrf')
  .description('QR Code File Decoder CLI')
  .version('1.0.0');

program
  .command('decode <video>')
  .description('Decode QR codes from video file')
  .option('-f, --fps <rate>', 'Frame processing rate (fps)', '1')
  .option('--fast', 'Fast scan mode (metadata only)', false)
  .option('-o, --output <dir>', 'Output directory for decoded files', './decoded')
  .option('--json <file>', 'Import scan data from JSON')
  .action((video, options) => {
    const app = render(<QRFDecoder videoPath={video} options={options} />);
    app.waitUntilExit().then(() => {
      console.log('Decoding complete!');
    });
  });

program
  .command('scan <video>')
  .description('Fast scan to discover files in video')
  .option('-o, --output <file>', 'Output JSON file', 'scan.json')
  .action((video, options) => {
    const app = render(<QRFDecoder videoPath={video} options={{ ...options, fast: true }} />);
    app.waitUntilExit().then(() => {
      console.log(`Scan saved to ${options.output}`);
    });
  });

program.parse();