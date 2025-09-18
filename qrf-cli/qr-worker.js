// Worker thread for QR processing
import { parentPort, workerData } from 'worker_threads';
import jsQR from 'jsqr';
import sharp from 'sharp';

const processFrames = async () => {
  const { frameFiles, startIndex, endIndex } = workerData;
  const results = [];

  for (let i = startIndex; i < endIndex; i++) {
    if (i >= frameFiles.length) break;

    const frameFile = frameFiles[i];
    try {
      // Try multiple rotations
      for (const rotation of [0, 180, 90, 270]) {
        const { data, info } = await sharp(frameFile)
          .rotate(rotation)
          .raw()
          .ensureAlpha()
          .toBuffer({ resolveWithObject: true });

        const qrResult = jsQR(data, info.width, info.height);

        if (qrResult && qrResult.data) {
          results.push({
            frameIndex: i,
            qrData: qrResult.data,
            rotation: rotation
          });
          break; // Found QR, no need to try other rotations
        }
      }
    } catch (error) {
      // Ignore errors
    }
  }

  parentPort.postMessage(results);
};

processFrames();