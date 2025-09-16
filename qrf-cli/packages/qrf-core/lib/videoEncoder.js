import ffmpeg from 'fluent-ffmpeg';
import fs from 'fs/promises';
import path from 'path';
import { createWriteStream } from 'fs';
import { pipeline } from 'stream/promises';
import { Readable } from 'stream';

export class VideoEncoder {
  constructor(options = {}) {
    this.fps = options.fps || 10;
    this.width = options.width || 1080;
    this.height = options.height || 1080;
    this.outputPath = options.outputPath;
    this.codec = options.codec || 'libx264';
    this.tempDir = null;
  }

  async createVideo(qrFrames, progressCallback) {
    // Create temp directory for frames
    this.tempDir = path.join(path.dirname(this.outputPath), `.temp_${Date.now()}`);
    await fs.mkdir(this.tempDir, { recursive: true });

    try {
      // Save QR frames as images
      await this.saveFrames(qrFrames, progressCallback);

      // Create video from frames
      await this.encodeVideo(progressCallback);

      // Cleanup temp directory
      await this.cleanup();

      return this.outputPath;
    } catch (error) {
      await this.cleanup();
      throw error;
    }
  }

  async saveFrames(qrFrames, progressCallback) {
    const totalFrames = qrFrames.length;
    
    for (let i = 0; i < qrFrames.length; i++) {
      const framePath = path.join(this.tempDir, `frame_${String(i).padStart(6, '0')}.png`);
      await fs.writeFile(framePath, qrFrames[i]);
      
      if (progressCallback) {
        progressCallback((i + 1) / totalFrames * 0.5); // First 50% for saving frames
      }
    }
  }

  async encodeVideo(progressCallback) {
    return new Promise((resolve, reject) => {
      const inputPattern = path.join(this.tempDir, 'frame_%06d.png');
      
      const command = ffmpeg()
        .input(inputPattern)
        .inputOptions([
          '-framerate', this.fps.toString(),
          '-pattern_type', 'sequence'
        ])
        .outputOptions([
          '-c:v', this.codec,
          '-pix_fmt', 'yuv420p',
          '-preset', 'fast',
          '-crf', '23',
          '-movflags', '+faststart'
        ])
        .output(this.outputPath)
        .on('start', (cmd) => {
          console.log('FFmpeg command:', cmd);
        })
        .on('progress', (progress) => {
          if (progressCallback && progress.percent) {
            progressCallback(0.5 + (progress.percent / 100) * 0.5); // Last 50% for encoding
          }
        })
        .on('end', () => {
          console.log('Video encoding complete');
          resolve();
        })
        .on('error', (err) => {
          console.error('FFmpeg error:', err);
          reject(err);
        });

      command.run();
    });
  }

  async cleanup() {
    if (this.tempDir) {
      try {
        // Remove all temp files
        const files = await fs.readdir(this.tempDir);
        for (const file of files) {
          await fs.unlink(path.join(this.tempDir, file));
        }
        await fs.rmdir(this.tempDir);
      } catch (error) {
        console.error('Cleanup error:', error);
      }
    }
  }

  // Alternative method using pipe for streaming (more memory efficient)
  async createVideoStream(qrFrames, progressCallback) {
    return new Promise((resolve, reject) => {
      let frameIndex = 0;
      const totalFrames = qrFrames.length;

      // Create a readable stream that emits frames
      const frameStream = new Readable({
        read() {
          if (frameIndex < qrFrames.length) {
            this.push(qrFrames[frameIndex]);
            frameIndex++;
            if (progressCallback) {
              progressCallback(frameIndex / totalFrames);
            }
          } else {
            this.push(null); // End stream
          }
        }
      });

      const command = ffmpeg()
        .input(frameStream)
        .inputFormat('image2pipe')
        .inputOptions([
          '-framerate', this.fps.toString()
        ])
        .outputOptions([
          '-c:v', this.codec,
          '-pix_fmt', 'yuv420p',
          '-preset', 'fast',
          '-crf', '23',
          '-movflags', '+faststart'
        ])
        .output(this.outputPath)
        .on('end', resolve)
        .on('error', reject);

      command.run();
    });
  }
}