import { EventEmitter } from 'events';
import { spawn } from 'child_process';
import ffmpeg from 'fluent-ffmpeg';

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
        console.log(`📹 Video duration: ${duration.toFixed(1)}s, expecting ~${this.totalFrames} frames at ${this.frameRate} FPS`);

        // Use spawn to run ffmpeg directly
        const args = [
          '-i', this.videoPath,
          '-vf', `fps=${this.frameRate}`,
          '-c:v', 'mjpeg',
          '-f', 'image2pipe',
          '-'
        ];

        const ffmpegProcess = spawn('ffmpeg', args);
        let frameCount = 0;
        let buffer = Buffer.alloc(0);
        const startTime = Date.now();

        ffmpegProcess.stdout.on('data', (chunk) => {
          // Accumulate chunks
          buffer = Buffer.concat([buffer, chunk]);

          // Extract JPEG frames
          let frameStart = 0;
          while (true) {
            // Find JPEG start marker
            const jpegStart = buffer.indexOf(Buffer.from([0xFF, 0xD8]), frameStart);
            if (jpegStart === -1) break;

            // Find JPEG end marker
            const jpegEnd = buffer.indexOf(Buffer.from([0xFF, 0xD9]), jpegStart + 2);
            if (jpegEnd === -1) break;

            // Extract frame
            const frame = buffer.slice(jpegStart, jpegEnd + 2);
            frameCount++;
            this.processedFrames++;

            // Emit frame event
            this.emit('frame', {
              data: frame,
              index: frameCount,
              timestamp: frameCount / this.frameRate
            });

            // Update progress
            if (this.totalFrames > 0) {
              const progress = this.processedFrames / this.totalFrames;
              this.emit('progress', progress);
            }

            // Calculate FPS
            const elapsed = (Date.now() - startTime) / 1000;
            if (elapsed > 0) {
              const fps = frameCount / elapsed;
              this.emit('fps', fps);
            }

            frameStart = jpegEnd + 2;
          }

          // Keep unprocessed data
          if (frameStart > 0 && frameStart < buffer.length) {
            buffer = buffer.slice(frameStart);
          } else if (frameStart >= buffer.length) {
            buffer = Buffer.alloc(0);
          }
        });

        ffmpegProcess.stderr.on('data', (data) => {
          // FFmpeg outputs to stderr, usually just progress info
          if (process.env.DEBUG) {
            console.log(`FFmpeg: ${data}`);
          }
        });

        ffmpegProcess.on('close', (code) => {
          if (code === 0) {
            this.emit('complete');
            resolve();
          } else {
            const error = new Error(`FFmpeg exited with code ${code}`);
            this.emit('error', error);
            reject(error);
          }
        });

        ffmpegProcess.on('error', (err) => {
          this.emit('error', err);
          reject(err);
        });
      });
    });
  }

  async extractFrame(timestamp) {
    return new Promise((resolve, reject) => {
      const args = [
        '-ss', timestamp.toString(),
        '-i', this.videoPath,
        '-frames:v', '1',
        '-c:v', 'mjpeg',
        '-f', 'image2pipe',
        '-'
      ];

      const ffmpegProcess = spawn('ffmpeg', args);
      const chunks = [];

      ffmpegProcess.stdout.on('data', (chunk) => {
        chunks.push(chunk);
      });

      ffmpegProcess.on('close', (code) => {
        if (code === 0) {
          const frameBuffer = Buffer.concat(chunks);
          resolve(frameBuffer);
        } else {
          reject(new Error(`Failed to extract frame at ${timestamp}`));
        }
      });

      ffmpegProcess.on('error', reject);
    });
  }

  pause() {
    this.paused = true;
  }

  resume() {
    this.paused = false;
  }
}