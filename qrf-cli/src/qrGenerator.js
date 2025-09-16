import QRCode from 'qrcode';
import { createCanvas, loadImage } from 'canvas';
import crypto from 'crypto';

export class QRGenerator {
  constructor(options = {}) {
    this.errorCorrection = options.errorCorrection || 'L';
    this.density = options.density || 'high';
    this.size = this.getSizeForDensity(this.density);
  }

  getSizeForDensity(density) {
    switch(density) {
      case 'low': return 21;
      case 'medium': return 25;
      case 'high': return 29;
      case 'ultra': return 33;
      default: return 29;
    }
  }

  async generateMetadata(metadata) {
    // Format metadata as string
    const metaString = [
      'M',
      '4.0',
      encodeURIComponent(metadata.fileName),
      encodeURIComponent(metadata.fileType),
      metadata.fileSize,
      metadata.chunksCount,
      metadata.packetCount,
      '0', // placeholder fields for compatibility
      '0',
      '0',
      '0',
      '0',
      '0',
      metadata.fileChecksum.substring(0, 8),
      metadata.fileChecksum,
      metadata.encoderVersion
    ].join(':');

    return this.createQRImage(metaString);
  }

  async generateDataPacket(packet, metadata) {
    // Calculate fileId from checksum
    const fileId = metadata.fileChecksum.substring(0, 8);
    
    // Format data packet
    const packetString = [
      'D',
      fileId,
      packet.id,
      packet.seed,
      packet.seedBase,
      metadata.chunksCount,
      packet.degree,
      packet.data
    ].join(':');

    return this.createQRImage(packetString);
  }

  async createQRImage(data) {
    const canvas = createCanvas(1080, 1080);
    const ctx = canvas.getContext('2d');

    // White background
    ctx.fillStyle = 'white';
    ctx.fillRect(0, 0, 1080, 1080);

    // Generate QR code
    const qrOptions = {
      errorCorrectionLevel: this.errorCorrection,
      type: 'png',
      quality: 1,
      margin: 2,
      color: {
        dark: '#000000',
        light: '#FFFFFF'
      },
      width: 900
    };

    try {
      const qrBuffer = await QRCode.toBuffer(data, qrOptions);
      const qrImage = await loadImage(qrBuffer);
      
      // Center QR code on canvas
      const x = (1080 - 900) / 2;
      const y = (1080 - 900) / 2;
      ctx.drawImage(qrImage, x, y, 900, 900);

      return canvas.toBuffer('image/png');
    } catch (error) {
      console.error('QR generation error:', error);
      throw error;
    }
  }
}