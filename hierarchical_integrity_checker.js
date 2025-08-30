/**
 * Hierarchical Integrity Checker - Divide & Conquer Corruption Detection
 * Uses binary search approach to pinpoint exact corruption locations
 */

class HierarchicalIntegrityChecker {
    constructor() {
        this.corruptedBlocks = new Set();
        this.verifiedBlocks = new Set();
    }

    // Enhanced FNV-1a checksum (matches encoder/decoder)
    calculateChecksum(data) {
        let hash = 2166136261; // FNV-1a offset basis
        for (let i = 0; i < data.length; i++) {
            hash ^= data[i];
            hash = Math.imul(hash, 16777619); // FNV-1a prime
        }
        return (hash >>> 0).toString(36).substring(0, 8);
    }

    // Recursively check file blocks using divide & conquer
    async checkFileIntegrity(fileData, expectedChecksums, blockSize = 1024) {
        console.log(`🔍 Starting hierarchical integrity check...`);
        console.log(`📄 File size: ${fileData.length} bytes`);
        console.log(`📦 Block size: ${blockSize} bytes`);
        
        const totalBlocks = Math.ceil(fileData.length / blockSize);
        console.log(`🧩 Total blocks: ${totalBlocks}`);
        
        // Generate expected checksums for all blocks if not provided
        if (!expectedChecksums) {
            expectedChecksums = this.generateBlockChecksums(fileData, blockSize);
            console.log(`📋 Generated ${expectedChecksums.length} block checksums`);
        }
        
        // Start recursive checking
        await this.checkBlockRange(fileData, expectedChecksums, 0, totalBlocks - 1, blockSize, 0);
        
        return {
            totalBlocks,
            corruptedBlocks: Array.from(this.corruptedBlocks).sort((a, b) => a - b),
            verifiedBlocks: Array.from(this.verifiedBlocks).sort((a, b) => a - b),
            corruptionRate: (this.corruptedBlocks.size / totalBlocks * 100).toFixed(2)
        };
    }

    // Generate checksums for all blocks in a file
    generateBlockChecksums(fileData, blockSize) {
        const checksums = [];
        for (let i = 0; i < fileData.length; i += blockSize) {
            const block = fileData.slice(i, i + blockSize);
            checksums.push(this.calculateChecksum(block));
        }
        return checksums;
    }

    // Recursive divide & conquer block checking
    async checkBlockRange(fileData, expectedChecksums, startBlock, endBlock, blockSize, depth) {
        const indent = "  ".repeat(depth);
        const rangeSize = endBlock - startBlock + 1;
        
        console.log(`${indent}🔍 Level ${depth}: Checking blocks ${startBlock}-${endBlock} (${rangeSize} blocks)`);
        
        // Base case: single block
        if (startBlock === endBlock) {
            const blockIndex = startBlock;
            const blockStart = blockIndex * blockSize;
            const blockData = fileData.slice(blockStart, blockStart + blockSize);
            
            const actualChecksum = this.calculateChecksum(blockData);
            const expectedChecksum = expectedChecksums[blockIndex];
            
            if (actualChecksum === expectedChecksum) {
                console.log(`${indent}✅ Block ${blockIndex}: VERIFIED (${actualChecksum})`);
                this.verifiedBlocks.add(blockIndex);
            } else {
                console.log(`${indent}❌ Block ${blockIndex}: CORRUPTED`);
                console.log(`${indent}   Expected: ${expectedChecksum}`);
                console.log(`${indent}   Got: ${actualChecksum}`);
                console.log(`${indent}   Position: ${blockStart}-${blockStart + blockData.length - 1}`);
                this.corruptedBlocks.add(blockIndex);
            }
            return;
        }

        // Calculate checksum for entire range
        const rangeStart = startBlock * blockSize;
        const rangeEnd = Math.min((endBlock + 1) * blockSize, fileData.length);
        const rangeData = fileData.slice(rangeStart, rangeEnd);
        
        // Calculate expected checksum for range
        const expectedRangeChecksum = this.calculateRangeChecksum(expectedChecksums, startBlock, endBlock);
        const actualRangeChecksum = this.calculateChecksum(rangeData);
        
        if (actualRangeChecksum === expectedRangeChecksum) {
            // Entire range is clean - mark all blocks as verified
            console.log(`${indent}✅ Range ${startBlock}-${endBlock}: ALL VERIFIED`);
            for (let i = startBlock; i <= endBlock; i++) {
                this.verifiedBlocks.add(i);
            }
        } else {
            // Range has corruption - divide and recurse
            console.log(`${indent}❌ Range ${startBlock}-${endBlock}: CORRUPTION DETECTED - dividing...`);
            
            if (rangeSize === 2) {
                // Check both blocks individually
                await this.checkBlockRange(fileData, expectedChecksums, startBlock, startBlock, blockSize, depth + 1);
                await this.checkBlockRange(fileData, expectedChecksums, endBlock, endBlock, blockSize, depth + 1);
            } else {
                // Divide range in half
                const midBlock = Math.floor((startBlock + endBlock) / 2);
                
                // Check left half
                await this.checkBlockRange(fileData, expectedChecksums, startBlock, midBlock, blockSize, depth + 1);
                
                // Check right half  
                await this.checkBlockRange(fileData, expectedChecksums, midBlock + 1, endBlock, blockSize, depth + 1);
            }
        }
    }

    // Calculate combined checksum for a range of blocks
    calculateRangeChecksum(blockChecksums, startBlock, endBlock) {
        let combinedData = '';
        for (let i = startBlock; i <= endBlock; i++) {
            combinedData += blockChecksums[i];
        }
        
        let hash = 2166136261; // FNV-1a offset basis
        for (let i = 0; i < combinedData.length; i++) {
            hash ^= combinedData.charCodeAt(i);
            hash = Math.imul(hash, 16777619); // FNV-1a prime
        }
        return (hash >>> 0).toString(36).substring(0, 8);
    }

    // Generate integrity manifest for a file
    generateIntegrityManifest(fileData, blockSize = 1024) {
        const blocks = [];
        const totalBlocks = Math.ceil(fileData.length / blockSize);
        
        for (let i = 0; i < totalBlocks; i++) {
            const start = i * blockSize;
            const end = Math.min(start + blockSize, fileData.length);
            const blockData = fileData.slice(start, end);
            
            blocks.push({
                index: i,
                start: start,
                end: end - 1,
                size: blockData.length,
                checksum: this.calculateChecksum(blockData)
            });
        }
        
        return {
            fileSize: fileData.length,
            blockSize,
            totalBlocks,
            fileChecksum: this.calculateChecksum(fileData),
            blocks
        };
    }

    // Compare two files with detailed block analysis
    async compareFiles(fileData1, fileData2, blockSize = 1024) {
        console.log(`🔍 Comparing files with ${blockSize}-byte blocks...`);
        
        if (fileData1.length !== fileData2.length) {
            console.log(`❌ File size mismatch: ${fileData1.length} vs ${fileData2.length} bytes`);
            return {
                identical: false,
                sizeMismatch: true,
                differingBlocks: []
            };
        }

        const manifest1 = this.generateIntegrityManifest(fileData1, blockSize);
        const manifest2 = this.generateIntegrityManifest(fileData2, blockSize);
        
        const differingBlocks = [];
        
        for (let i = 0; i < manifest1.blocks.length; i++) {
            if (manifest1.blocks[i].checksum !== manifest2.blocks[i].checksum) {
                differingBlocks.push({
                    blockIndex: i,
                    start: manifest1.blocks[i].start,
                    end: manifest1.blocks[i].end,
                    checksum1: manifest1.blocks[i].checksum,
                    checksum2: manifest2.blocks[i].checksum
                });
            }
        }
        
        const identical = differingBlocks.length === 0;
        
        console.log(`📊 Comparison result:`);
        console.log(`   Identical: ${identical ? '✅ YES' : '❌ NO'}`);
        console.log(`   Total blocks: ${manifest1.totalBlocks}`);
        console.log(`   Differing blocks: ${differingBlocks.length}`);
        
        if (!identical) {
            console.log(`🚫 Corrupted blocks:`);
            differingBlocks.forEach(block => {
                console.log(`   Block ${block.blockIndex}: bytes ${block.start}-${block.end}`);
                console.log(`     File1: ${block.checksum1}`);
                console.log(`     File2: ${block.checksum2}`);
            });
        }
        
        return {
            identical,
            sizeMismatch: false,
            totalBlocks: manifest1.totalBlocks,
            differingBlocks,
            corruptionRate: (differingBlocks.length / manifest1.totalBlocks * 100).toFixed(2)
        };
    }
}

// Example usage functions
async function checkFileCorruption(fileData, expectedBlockChecksums = null, blockSize = 1024) {
    const checker = new HierarchicalIntegrityChecker();
    const result = await checker.checkFileIntegrity(fileData, expectedBlockChecksums, blockSize);
    
    console.log(`\n📋 Integrity Check Results:`);
    console.log(`   Total blocks: ${result.totalBlocks}`);
    console.log(`   Verified blocks: ${result.verifiedBlocks.length}`);
    console.log(`   Corrupted blocks: ${result.corruptedBlocks.length}`);
    console.log(`   Corruption rate: ${result.corruptionRate}%`);
    
    if (result.corruptedBlocks.length > 0) {
        console.log(`\n🚫 Corrupted block indices: ${result.corruptedBlocks.join(', ')}`);
    }
    
    return result;
}

async function compareFileBlocks(file1Data, file2Data, blockSize = 1024) {
    const checker = new HierarchicalIntegrityChecker();
    const result = await checker.compareFiles(file1Data, file2Data, blockSize);
    
    if (result.differingBlocks.length > 0) {
        console.log(`\n🎯 Corruption analysis:`);
        console.log(`   ${result.differingBlocks.length} blocks corrupted out of ${result.totalBlocks}`);
        console.log(`   Corruption rate: ${result.corruptionRate}%`);
        console.log(`   Corrupted byte ranges:`);
        
        result.differingBlocks.forEach(block => {
            console.log(`     Bytes ${block.start}-${block.end} (block ${block.blockIndex})`);
        });
    }
    
    return result;
}

// Export for use in browser or Node.js
if (typeof module !== 'undefined' && module.exports) {
    module.exports = { 
        HierarchicalIntegrityChecker, 
        checkFileCorruption, 
        compareFileBlocks 
    };
} else {
    // Browser global
    window.HierarchicalIntegrityChecker = HierarchicalIntegrityChecker;
    window.checkFileCorruption = checkFileCorruption;
    window.compareFileBlocks = compareFileBlocks;
}