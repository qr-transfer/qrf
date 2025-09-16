import Jimp from 'jimp';
import QrCode from 'qrcode-reader';

export class QRDecoder {
  constructor() {
    this.qr = new QrCode();
    this.qr.callback = (err, value) => {
      if (err) {
        this.lastError = err;
        this.lastResult = null;
      } else {
        this.lastError = null;
        this.lastResult = value;
      }
    };
  }

  async decode(frameData) {
    try {
      // Convert frame buffer to Jimp image
      const image = await Jimp.read(frameData.data);

      // Prepare image for QR detection
      const prepared = await this.prepareImage(image);

      // Decode QR code
      this.qr.decode(prepared.bitmap);

      if (this.lastResult) {
        const data = this.parseQRData(this.lastResult.result);
        return data;
      }

      return null;
    } catch (error) {
      console.error('QR decode error:', error);
      return null;
    }
  }

  async prepareImage(image) {
    // Apply image processing to improve QR detection
    return image
      .greyscale()
      .contrast(0.3)
      .brightness(0.1);
  }

  parseQRData(qrString) {
    // Parse QR data based on format
    if (qrString.startsWith('M:')) {
      // Metadata packet
      const parts = qrString.split(':');
      return {
        type: 'metadata',
        version: parts[1],
        fileName: decodeURIComponent(parts[2]),
        fileType: decodeURIComponent(parts[3]),
        fileSize: parseInt(parts[4]),
        chunksCount: parseInt(parts[5]),
        packetCount: parseInt(parts[6]),
        checksum: parts[13],
        fileChecksum: parts[14],
        encoderVersion: parts[15] || '3.0'
      };
    } else if (qrString.startsWith('D:')) {
      // Data packet
      const parts = qrString.split(':');

      // Check for new format with fileId
      if (parts[1] && parts[1].length === 8 && /^[a-fA-F0-9]{8}$/.test(parts[1])) {
        return {
          type: 'data',
          fileId: parts[1],
          packetId: parseInt(parts[2]),
          seed: parseInt(parts[3]),
          seedBase: parseInt(parts[4]),
          numChunks: parseInt(parts[5]),
          degree: parseInt(parts[6]),
          data: parts.slice(7).join(':')
        };
      } else {
        // Legacy format
        return {
          type: 'data',
          packetId: parseInt(parts[1]),
          seed: parseInt(parts[2]),
          seedBase: parseInt(parts[3]),
          numChunks: parseInt(parts[4]),
          degree: parseInt(parts[5]),
          data: parts.slice(6).join(':')
        };
      }
    }

    return null;
  }
}