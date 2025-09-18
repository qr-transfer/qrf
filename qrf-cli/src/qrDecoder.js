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
        if (process.env.DEBUG_QR) {
          console.log('QR detected:', this.lastResult.result?.substring(0, 100));
        }
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
    // Debug: log raw QR data
    if (process.env.DEBUG_QR) {
      console.log('Raw QR data:', qrString.substring(0, 100));
    }

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

      // The format appears to be: D:packetId:timestamp1:timestamp2:totalChunks:degree:index:data
      // Let's handle this format
      if (parts.length >= 7) {
        return {
          type: 'data',
          packetId: parseInt(parts[1]),
          timestamp1: parts[2],
          timestamp2: parts[3],
          numChunks: parseInt(parts[4]),
          degree: parseInt(parts[5]),
          sourceIndices: [parseInt(parts[6])], // Convert single index to array
          data: parts.slice(7).join(':')
        };
      }
    }

    return null;
  }
}