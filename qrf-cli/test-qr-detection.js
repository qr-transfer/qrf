#!/usr/bin/env node
import { spawn } from 'child_process';
import jsQR from 'jsqr';
import sharp from 'sharp';
import Jimp from 'jimp';
import QrCode from 'qrcode-reader';

// Test script to see which QR detection method works best
async function testQRDetection(videoPath) {
  console.log('🔍 Testing QR detection methods...\n');

  const args = [
    '-i', videoPath,
    '-ss', '100', // Start at 100s where metadata should be
    '-t', '10',   // Only 10 seconds
    '-vf', 'fps=5',
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
  const qr = new QrCode();

  console.log('Testing with jsQR + Sharp...\n');

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

      console.log(`\nFrame ${frameCount}:`);

      // Test 1: jsQR + Sharp
      try {
        const { data, info } = await sharp(frame)
          .raw()
          .ensureAlpha()
          .toBuffer({ resolveWithObject: true });

        const qrResult = jsQR(data, info.width, info.height);
        console.log(`  jsQR + Sharp: ${qrResult ? 'QR FOUND: ' + qrResult.data.substring(0, 50) + '...' : 'No QR'}`);
      } catch (error) {
        console.log(`  jsQR + Sharp: Error - ${error.message}`);
      }

      // Test 2: qrcode-reader + Jimp
      try {
        const image = await Jimp.read(frame);
        await new Promise((resolve) => {
          qr.callback = (err, value) => {
            if (!err && value && value.result) {
              console.log(`  qrcode-reader + Jimp: QR FOUND: ${value.result.substring(0, 50)}...`);
            } else {
              console.log(`  qrcode-reader + Jimp: No QR`);
            }
            resolve();
          };
          qr.decode(image.bitmap);
        });
      } catch (error) {
        console.log(`  qrcode-reader + Jimp: Error - ${error.message}`);
      }

      frameStart = jpegEnd + 2;

      if (frameCount >= 5) break; // Test first 5 frames
    }

    if (frameStart > 0 && frameStart < buffer.length) {
      buffer = buffer.slice(frameStart);
    } else if (frameStart >= buffer.length) {
      buffer = Buffer.alloc(0);
    }

    if (frameCount >= 5) {
      ffmpegProcess.kill();
    }
  });

  return new Promise((resolve) => {
    ffmpegProcess.on('close', () => {
      console.log('\n✅ QR detection test complete');
      resolve();
    });
  });
}

// Run test
if (process.argv.length < 3) {
  console.log('Usage: node test-qr-detection.js <video>');
  process.exit(1);
}

testQRDetection(process.argv[2]).catch(console.error);