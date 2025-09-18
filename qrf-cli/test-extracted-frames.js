#!/usr/bin/env node
import jsQR from 'jsqr';
import sharp from 'sharp';
import fs from 'fs/promises';
import path from 'path';

async function testFrame(framePath) {
  console.log(`\n🔍 Testing frame: ${framePath}`);

  try {
    // Check if file exists
    await fs.access(framePath);

    // Try original orientation
    const { data, info } = await sharp(framePath)
      .raw()
      .ensureAlpha()
      .toBuffer({ resolveWithObject: true });

    console.log(`   Image size: ${info.width}x${info.height}`);

    let qrResult = jsQR(data, info.width, info.height);
    if (qrResult) {
      console.log(`   ✅ QR found (original): ${qrResult.data.substring(0, 80)}...`);
      return true;
    }

    // Try different rotations
    for (const rotation of [90, 180, 270]) {
      const rotated = await sharp(framePath)
        .rotate(rotation)
        .raw()
        .ensureAlpha()
        .toBuffer({ resolveWithObject: true });

      qrResult = jsQR(rotated.data, rotated.info.width, rotated.info.height);
      if (qrResult) {
        console.log(`   ✅ QR found (${rotation}°): ${qrResult.data.substring(0, 80)}...`);
        return true;
      }
    }

    console.log(`   ❌ No QR code detected`);
    return false;
  } catch (error) {
    console.log(`   ❌ Error: ${error.message}`);
    return false;
  }
}

async function testAllFrames() {
  console.log('🔍 Testing extracted frames for QR codes...');

  const frameDir = './tmp/test_frames';

  try {
    const files = await fs.readdir(frameDir);
    const jpgFiles = files.filter(f => f.endsWith('.jpg')).sort();

    console.log(`Found ${jpgFiles.length} frame files to test`);

    let foundCount = 0;
    for (const file of jpgFiles) {
      const framePath = path.join(frameDir, file);
      const found = await testFrame(framePath);
      if (found) foundCount++;
    }

    console.log(`\n📊 Results: ${foundCount}/${jpgFiles.length} frames contained QR codes`);

    if (foundCount === 0) {
      console.log('\n❌ No QR codes found in any extracted frames');
      console.log('   This suggests:');
      console.log('   1. QR codes are at different time positions');
      console.log('   2. QR codes need different image processing');
      console.log('   3. The video format/encoding is incompatible');
    }

  } catch (error) {
    console.error('Error testing frames:', error.message);
  }
}

testAllFrames();