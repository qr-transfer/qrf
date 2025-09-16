import crypto from 'crypto';

export class FountainEncoder {
  constructor() {
    this.rng = null;
  }

  async encode(chunks, options = {}) {
    const redundancy = options.redundancy || 1.5;
    const systematic = options.systematic !== false;
    const packets = [];
    const numChunks = chunks.length;
    const targetPackets = Math.ceil(numChunks * redundancy);

    // Add systematic packets (original chunks)
    if (systematic) {
      for (let i = 0; i < chunks.length; i++) {
        packets.push({
          id: i,
          degree: 1,
          seed: i,
          seedBase: 0,
          sourceIndices: [i],
          data: chunks[i].toString('base64')
        });
      }
    }

    // Generate fountain-coded packets
    const additionalPackets = targetPackets - packets.length;
    for (let i = 0; i < additionalPackets; i++) {
      const packet = this.generatePacket(chunks, packets.length + i);
      packets.push(packet);
    }

    return packets;
  }

  generatePacket(chunks, packetId) {
    // Initialize RNG with packet ID as seed
    const seed = packetId + chunks.length;
    this.rng = this.createRNG(seed);

    // Choose degree (number of chunks to XOR)
    const degree = this.chooseDegree(chunks.length);
    
    // Select random chunks
    const sourceIndices = this.selectRandomChunks(chunks.length, degree);
    
    // XOR the selected chunks
    const xorData = this.xorChunks(chunks, sourceIndices);

    return {
      id: packetId,
      degree,
      seed,
      seedBase: 0,
      sourceIndices,
      data: xorData.toString('base64')
    };
  }

  chooseDegree(numChunks) {
    // Soliton distribution for fountain codes
    const rand = this.rng();
    
    if (rand < 0.5) return 1;
    if (rand < 0.75) return 2;
    if (rand < 0.875) return 3;
    
    // Higher degrees with decreasing probability
    const maxDegree = Math.min(numChunks, 10);
    return Math.floor(this.rng() * (maxDegree - 3)) + 4;
  }

  selectRandomChunks(numChunks, degree) {
    const indices = [];
    const used = new Set();
    
    while (indices.length < degree) {
      const index = Math.floor(this.rng() * numChunks);
      if (!used.has(index)) {
        used.add(index);
        indices.push(index);
      }
    }
    
    return indices.sort((a, b) => a - b);
  }

  xorChunks(chunks, indices) {
    if (indices.length === 0) return Buffer.alloc(0);
    if (indices.length === 1) return chunks[indices[0]];
    
    // Start with first chunk
    let result = Buffer.from(chunks[indices[0]]);
    
    // XOR with remaining chunks
    for (let i = 1; i < indices.length; i++) {
      const chunk = chunks[indices[i]];
      const maxLen = Math.max(result.length, chunk.length);
      const xored = Buffer.alloc(maxLen);
      
      for (let j = 0; j < maxLen; j++) {
        const byte1 = j < result.length ? result[j] : 0;
        const byte2 = j < chunk.length ? chunk[j] : 0;
        xored[j] = byte1 ^ byte2;
      }
      
      result = xored;
    }
    
    return result;
  }

  createRNG(seed) {
    // Simple linear congruential generator
    let state = seed;
    return () => {
      state = (state * 1664525 + 1013904223) >>> 0;
      return state / 0x100000000;
    };
  }
}