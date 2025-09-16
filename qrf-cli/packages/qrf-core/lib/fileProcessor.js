import fs from 'fs/promises';
import crypto from 'crypto';
import path from 'path';
import mime from 'mime-types';

export class FileProcessor {
  constructor() {
    this.defaultChunkSize = 1024; // 1KB default
  }

  async readFile(filePath) {
    const buffer = await fs.readFile(filePath);
    const stats = await fs.stat(filePath);
    const mimeType = mime.lookup(filePath) || 'application/octet-stream';

    return {
      buffer,
      size: stats.size,
      mimeType,
      fileName: path.basename(filePath)
    };
  }

  async splitIntoChunks(buffer, options = {}) {
    const chunkSize = options.chunkSize || this.defaultChunkSize;
    const chunks = [];
    
    for (let i = 0; i < buffer.length; i += chunkSize) {
      const chunk = buffer.slice(i, i + chunkSize);
      chunks.push(chunk);
    }

    return chunks;
  }

  async calculateChecksum(buffer) {
    const hash = crypto.createHash('sha256');
    hash.update(buffer);
    return hash.digest('hex');
  }

  combineChunks(chunks) {
    return Buffer.concat(chunks);
  }

  async saveFile(filePath, buffer) {
    await fs.writeFile(filePath, buffer);
    return filePath;
  }
}