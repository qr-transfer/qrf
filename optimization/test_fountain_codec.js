/**
 * Test script to verify correct fountain code implementation.
 * This simulates the encoder and decoder processing for fountain codes
 * to ensure proper binary XOR operations.
 */

// Simulate the encoder's chunk generation
function generateTestChunks(count = 5, size = 20) {
  const chunks = [];
  
  for (let i = 0; i < count; i++) {
    // Create a random chunk of data
    const chunk = new Uint8Array(size);
    for (let j = 0; j < size; j++) {
      chunk[j] = Math.floor(Math.random() * 256);
    }
    chunks.push(chunk);
  }
  
  return chunks;
}

// Convert chunks to string (simulating base64 encoding)
function chunkToString(chunk) {
  return Array.from(chunk).map(b => String.fromCharCode(b)).join('');
}

// Convert string back to Uint8Array (simulating base64 decoding)
function stringToChunk(str) {
  const chunk = new Uint8Array(str.length);
  for (let i = 0; i < str.length; i++) {
    chunk[i] = str.charCodeAt(i);
  }
  return chunk;
}

// XOR combine chunks for fountain coding (encoder side)
function xorCombineChunks(chunks, indices) {
  if (indices.length === 0) return new Uint8Array(0);
  
  // Get first chunk as starting point
  const firstChunk = chunks[indices[0]];
  
  // If only one chunk, return it directly
  if (indices.length === 1) return firstChunk;
  
  // Create result array initialized with first chunk
  const result = new Uint8Array(firstChunk.length);
  result.set(firstChunk);
  
  // XOR with remaining chunks
  for (let i = 1; i < indices.length; i++) {
    const chunkIndex = indices[i];
    const chunkData = chunks[chunkIndex];
    
    // XOR the data
    for (let j = 0; j < Math.min(result.length, chunkData.length); j++) {
      result[j] ^= chunkData[j];
    }
  }
  
  return result;
}

// Decoder side: recover missing chunk from fountain packet
function recoverMissingChunk(fountainData, recoveredChunks, indices, missingIndex) {
  // Create a copy of the fountain data
  const result = new Uint8Array(fountainData.length);
  result.set(fountainData);
  
  // XOR with all the chunks we already have
  for (const chunkIndex of indices) {
    if (chunkIndex !== missingIndex && recoveredChunks[chunkIndex]) {
      // XOR the data
      for (let i = 0; i < Math.min(result.length, recoveredChunks[chunkIndex].length); i++) {
        result[i] ^= recoveredChunks[chunkIndex][i];
      }
    }
  }
  
  return result;
}

// Function to check if two chunks are identical
function chunksEqual(chunk1, chunk2) {
  if (chunk1.length !== chunk2.length) return false;
  
  for (let i = 0; i < chunk1.length; i++) {
    if (chunk1[i] !== chunk2[i]) return false;
  }
  
  return true;
}

// Main test function
function testFountainCodec() {
  console.log("Testing Fountain Code Implementation\n");
  
  // Generate test chunks
  const chunkCount = 5;
  const chunkSize = 30;
  console.log(`Generating ${chunkCount} random chunks of size ${chunkSize} bytes...`);
  const originalChunks = generateTestChunks(chunkCount, chunkSize);
  
  // Print a sample of original chunks
  console.log("\nSample of original chunks:");
  for (let i = 0; i < Math.min(3, chunkCount); i++) {
    console.log(`Chunk ${i}: ${Array.from(originalChunks[i].slice(0, 8)).map(b => b.toString(16).padStart(2, '0')).join(' ')}...`);
  }
  
  // Test cases for different degrees
  const testCases = [
    { name: "Single chunk (degree 1)", indices: [2] },
    { name: "Two chunks (degree 2)", indices: [0, 3] },
    { name: "Three chunks (degree 3)", indices: [1, 2, 4] }
  ];
  
  let allTestsPassed = true;
  
  for (const testCase of testCases) {
    console.log(`\n\nTesting ${testCase.name}:`);
    const indices = testCase.indices;
    console.log(`Using indices: ${indices.join(', ')}`);
    
    // ENCODER
    // 1. Generate fountain packet by XOR'ing the selected chunks
    console.log("\nEncoder: Generating fountain packet...");
    const fountainData = xorCombineChunks(originalChunks, indices);
    console.log(`Fountain data (first 8 bytes): ${Array.from(fountainData.slice(0, 8)).map(b => b.toString(16).padStart(2, '0')).join(' ')}...`);
    
    // Convert to string (simulate transmission)
    const fountainDataString = chunkToString(fountainData);
    console.log(`Converted to string for transmission (length: ${fountainDataString.length})`);
    
    // DECODER
    // 1. Convert back from string
    console.log("\nDecoder: Receiving and processing fountain packet...");
    const receivedFountainData = stringToChunk(fountainDataString);
    
    // 2. Set up recovered chunks (we'll simulate having all but one)
    const recoveredChunks = {};
    const missingIndex = indices[indices.length - 1]; // Last index will be the "missing" one
    
    // Add all chunks except the missing one to the recovered chunks
    for (const idx of indices) {
      if (idx !== missingIndex) {
        recoveredChunks[idx] = originalChunks[idx];
        console.log(`Decoder already has chunk ${idx}`);
      }
    }
    
    console.log(`Decoder is missing chunk ${missingIndex}`);
    
    // 3. Recover the missing chunk
    console.log("\nDecoder: Recovering missing chunk...");
    const recoveredChunk = recoverMissingChunk(
      receivedFountainData, 
      recoveredChunks, 
      indices, 
      missingIndex
    );
    
    // 4. Verify the recovered chunk matches the original
    const isMatch = chunksEqual(recoveredChunk, originalChunks[missingIndex]);
    console.log(`Recovered chunk ${missingIndex} (first 8 bytes): ${Array.from(recoveredChunk.slice(0, 8)).map(b => b.toString(16).padStart(2, '0')).join(' ')}...`);
    console.log(`Original chunk ${missingIndex} (first 8 bytes): ${Array.from(originalChunks[missingIndex].slice(0, 8)).map(b => b.toString(16).padStart(2, '0')).join(' ')}...`);
    console.log(`\nVerification: ${isMatch ? 'MATCH ✓' : 'MISMATCH ✗'}`);
    
    if (!isMatch) {
      console.log("❌ Test failed!");
      allTestsPassed = false;
    }
  }
  
  if (allTestsPassed) {
    console.log("\n\n✅ All tests passed! The fountain code implementation is working correctly.");
  } else {
    console.log("\n\n❌ Some tests failed. The fountain code implementation needs fixing.");
  }
}

// Run the test
testFountainCodec();