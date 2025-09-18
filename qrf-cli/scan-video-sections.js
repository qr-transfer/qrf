#!/usr/bin/env node
import { spawn } from 'child_process';
import jsQR from 'jsqr';
import sharp from 'sharp';

async function scanVideoSection(videoPath, startSeconds, durationSeconds) {
  console.log(`\n🔍 Scanning ${startSeconds}s to ${startSeconds + durationSeconds}s...`);

  const args = [
    '-i', videoPath,
    '-ss', startSeconds.toString(),
    '-t', durationSeconds.toString(),
    '-vf', 'fps=10,transpose=2,transpose=2', // 180 degree rotation + 10 FPS
    '-c:v', 'mjpeg',
    '-q:v', '2',
    '-f', 'image2pipe',
    '-'
  ];

  const ffmpegProcess = spawn('ffmpeg', args, {
    stdio: ['ignore', 'pipe', 'ignore']
  });

  let buffer = Buffer.alloc(0);
  let frameCount = 0;
  let qrFound = 0;

  ffmpegProcess.stdout.on('data', async (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);

    let frameStart = 0;
    while (true) {
      const jpegStart = buffer.indexOf(Buffer.from([0xFF, 0xD8]), frameStart);
      if (jpegStart === -1) break;

      const jpegEnd = buffer.indexOf(Buffer.from([0xFF, 0xD9]), jpegStart + 2);
      if (jpegEnd === -1) break;

      const frame = buffer.slice(jpegStart, jpegEnd + 2);
      frameCount++;

      try {
        const { data, info } = await sharp(frame)
          .raw()
          .ensureAlpha()
          .toBuffer({ resolveWithObject: true });

        const qrResult = jsQR(data, info.width, info.height);

        if (qrResult && qrResult.data) {
          qrFound++;
          const timestamp = startSeconds + (frameCount / 10);
          console.log(`   ✅ QR found at ${timestamp.toFixed(1)}s: ${qrResult.data.substring(0, 80)}...`);

          if (qrFound >= 3) { // Stop after finding 3 QRs
            ffmpegProcess.kill();
            break;
          }
        }
      } catch (error) {
        // Ignore
      }

      frameStart = jpegEnd + 2;
    }

    if (frameStart > 0 && frameStart < buffer.length) {
      buffer = buffer.slice(frameStart);
    } else if (frameStart >= buffer.length) {
      buffer = Buffer.alloc(0);
    }
  });

  return new Promise((resolve) => {
    ffmpegProcess.on('close', () => {
      console.log(`   Scanned ${frameCount} frames, found ${qrFound} QR codes`);
      resolve({ frameCount, qrFound });
    });
  });
}

async function quickScan(videoPath) {
  console.log('🔍 Quick QR Code Location Scan\n');

  // Test different sections of the video
  const sections = [
    { start: 0, duration: 30 },      // Start
    { start: 60, duration: 30 },     // 1 minute
    { start: 90, duration: 30 },     // Around where metadata was expected
    { start: 120, duration: 30 },    // 2 minutes
    { start: 300, duration: 30 },    // 5 minutes
    { start: 600, duration: 30 },    // 10 minutes
    { start: 1800, duration: 30 },   // 30 minutes
    { start: 3600, duration: 30 }    // 1 hour
  ];

  for (const section of sections) {
    await scanVideoSection(videoPath, section.start, section.duration);
  }

  console.log('\n✅ Quick scan complete');
}

// Run scan
if (process.argv.length < 3) {
  console.log('Usage: node scan-video-sections.js <video>');
  process.exit(1);
}

quickScan(process.argv[2]).catch(console.error);