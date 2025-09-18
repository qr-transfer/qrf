import { EventEmitter } from 'events';
import ffmpeg from 'fluent-ffmpeg';
import { createReadStream } from 'fs';
import { pipeline } from 'stream/promises';

export class VideoProcessor extends EventEmitter {
  constructor(videoPath, options = {}) {
    super();
    this.videoPath = videoPath;
    this.frameRate = options.frameRate || 1;
    this.fastScan = options.fastScan || false;
    this.totalFrames = 0;
    this.processedFrames = 0;
    this.startTime = Date.now();
  }

  async start() {
    return new Promise((resolve, reject) => {
      // Get video metadata
      ffmpeg.ffprobe(this.videoPath, (err, metadata) => {
        if (err) return reject(err);

        const duration = metadata.format.duration;
        this.totalFrames = Math.floor(duration * this.frameRate);

        // Extract frames at specified rate
        const command = ffmpeg(this.videoPath)
          .fps(this.frameRate)
          .format('image2pipe')
          .videoCodec('mjpeg')
          .pipe();  // Add pipe to output to stdout

        let frameCount = 0;
        const startTime = Date.now();
        let buffer = Buffer.alloc(0);

        command.on('data', (chunk) => {
          // Accumulate chunks until we have a complete frame
          buffer = Buffer.concat([buffer, chunk]);

          // Simple JPEG detection - look for JPEG markers
          let frameStart = 0;
          while (frameStart < buffer.length - 1) {
            // Find JPEG start marker (FFD8)
            const jpegStart = buffer.indexOf(Buffer.from([0xFF, 0xD8]), frameStart);
            if (jpegStart === -1) break;

            // Find JPEG end marker (FFD9) after the start
            const jpegEnd = buffer.indexOf(Buffer.from([0xFF, 0xD9]), jpegStart + 2);
            if (jpegEnd === -1) break;

            // Extract complete JPEG frame
            const frameBuffer = buffer.slice(jpegStart, jpegEnd + 2);
            frameStart = jpegEnd + 2;

            frameCount++;
            this.processedFrames++;

            // Emit frame for processing
            this.emit('frame', {
              data: frameBuffer,
              index: frameCount,
              timestamp: frameCount / this.frameRate
            });

            // Update progress
            const progress = this.processedFrames / this.totalFrames;
            this.emit('progress', progress);

            // Calculate FPS
            const elapsed = (Date.now() - startTime) / 1000;
            const fps = frameCount / elapsed;
            this.emit('fps', fps);
          }

          // Keep remaining data in buffer
          if (frameStart < buffer.length) {
            buffer = buffer.slice(frameStart);
          } else {
            buffer = Buffer.alloc(0);
          }
        });

        command.on('end', () => {
          this.emit('complete');
          resolve();
        });

        command.on('error', (err) => {
          this.emit('error', err);
          reject(err);
        });

        // Start processing
        command.run();
      });
    });
  }

  shouldSkip(frameIndex) {
    // Logic to determine if we should skip ahead
    // Based on metadata discovery
    return false;
  }

  getSkipTime(frameIndex) {
    // Calculate skip time based on chunks discovered
    return frameIndex / this.frameRate + 10; // Skip 10 seconds ahead
  }

  async extractFrame(timestamp) {
    return new Promise((resolve, reject) => {
      const command = ffmpeg(this.videoPath)
        .seekInput(timestamp)
        .frames(1)
        .format('image2pipe')
        .videoCodec('mjpeg');

      const chunks = [];

      command.on('data', (chunk) => {
        chunks.push(chunk);
      });

      command.on('end', () => {
        const frameBuffer = Buffer.concat(chunks);
        resolve(frameBuffer);
      });

      command.on('error', reject);

      command.run();
    });
  }

  pause() {
    // Pause processing
    this.paused = true;
  }

  resume() {
    // Resume processing
    this.paused = false;
  }
}