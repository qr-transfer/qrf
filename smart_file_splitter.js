#!/usr/bin/env node

/**
 * Smart File Splitter with ZIP Organization
 * Groups files into optimal ZIP blocks of ~100KB each
 * 
 * Usage:
 *   node smart_file_splitter.js --organize <directory> [options]
 *   node smart_file_splitter.js --split <file> [options]
 * 
 * Examples:
 *   # Organize directory into ZIP blocks
 *   node smart_file_splitter.js --organize ./files --target-size 100KB
 *   
 *   # Split single large file
 *   node smart_file_splitter.js --split large_file.pdf --target-size 100KB
 */

const fs = require('fs').promises;
const fsSync = require('fs');
const path = require('path');
const { program } = require('commander');
const { createReadStream, createWriteStream } = require('fs');
const { pipeline } = require('stream/promises');

/**
 * Estimate compressed size using compression ratio heuristics
 * @param {string} filePath - Path to file
 * @param {number} fileSize - File size in bytes
 * @returns {Promise<number>} Estimated compressed size
 */
async function estimateCompressedSize(filePath, fileSize) {
    const ext = path.extname(filePath).toLowerCase();
    
    // Compression ratios based on file types (approximate)
    const compressionRatios = {
        // Already compressed formats (minimal compression)
        '.zip': 1.0, '.rar': 1.0, '.7z': 1.0, '.gz': 1.0, '.bz2': 1.0,
        '.jpg': 1.0, '.jpeg': 1.0, '.png': 1.05, '.gif': 1.0,
        '.mp4': 1.0, '.avi': 1.0, '.mkv': 1.0, '.mov': 1.0,
        '.mp3': 1.0, '.flac': 1.0, '.aac': 1.0, '.ogg': 1.0,
        '.pdf': 1.1, '.docx': 1.1, '.xlsx': 1.1, '.pptx': 1.1,
        
        // Text and data files (good compression)
        '.txt': 0.3, '.md': 0.35, '.csv': 0.4, '.json': 0.4, '.xml': 0.5,
        '.html': 0.4, '.css': 0.35, '.js': 0.45, '.ts': 0.45,
        '.py': 0.4, '.java': 0.45, '.cpp': 0.45, '.c': 0.45,
        '.log': 0.2, '.sql': 0.4, '.yaml': 0.4, '.yml': 0.4,
        
        // Binary data (moderate compression)
        '.exe': 0.7, '.dll': 0.7, '.bin': 0.8, '.dat': 0.6,
        '.db': 0.6, '.sqlite': 0.6, '.iso': 0.9,
        
        // Images (varies)
        '.bmp': 0.1, '.tiff': 0.4, '.svg': 0.3, '.ico': 0.8,
        
        // Documents
        '.doc': 0.5, '.rtf': 0.4, '.odt': 0.7, '.tex': 0.4
    };
    
    const ratio = compressionRatios[ext] || 0.6; // Default 60% compression
    
    // For very small files, add ZIP overhead
    const zipOverhead = Math.min(100, fileSize * 0.1); // ~10% overhead, max 100 bytes
    const estimatedSize = Math.max(zipOverhead, fileSize * ratio + zipOverhead);
    
    return Math.round(estimatedSize);
}

/**
 * Analyze directory and get file information
 * @param {string} dirPath - Directory path
 * @returns {Promise<Array>} Array of file info objects
 */
async function analyzeDirectory(dirPath) {
    const files = [];
    
    async function scanDir(currentPath, relativePath = '') {
        const entries = await fs.readdir(currentPath, { withFileTypes: true });
        
        for (const entry of entries) {
            const fullPath = path.join(currentPath, entry.name);
            const relPath = path.join(relativePath, entry.name);
            
            if (entry.isDirectory()) {
                await scanDir(fullPath, relPath);
            } else if (entry.isFile()) {
                try {
                    const stats = await fs.stat(fullPath);
                    const estimatedCompressed = await estimateCompressedSize(fullPath, stats.size);
                    
                    files.push({
                        path: fullPath,
                        relativePath: relPath,
                        size: stats.size,
                        estimatedCompressed,
                        compressionRatio: estimatedCompressed / stats.size,
                        extension: path.extname(entry.name).toLowerCase()
                    });
                } catch (error) {
                    console.warn(`Warning: Could not analyze ${fullPath}: ${error.message}`);
                }
            }
        }
    }
    
    await scanDir(dirPath);
    return files;
}

/**
 * Group files into optimal ZIP blocks using bin packing algorithm
 * @param {Array} files - Array of file info objects
 * @param {number} targetSize - Target ZIP size in bytes
 * @returns {Array} Array of file groups
 */
function groupFilesIntoBlocks(files, targetSize) {
    // Sort files by estimated compressed size (largest first for better packing)
    const sortedFiles = files.slice().sort((a, b) => b.estimatedCompressed - a.estimatedCompressed);
    
    const groups = [];
    const used = new Set();
    
    // First pass: handle files that are too large individually
    const largeFiles = sortedFiles.filter(f => f.estimatedCompressed > targetSize);
    for (const file of largeFiles) {
        groups.push({
            files: [file],
            totalSize: file.size,
            totalCompressed: file.estimatedCompressed,
            needsSplitting: true
        });
        used.add(file);
    }
    
    // Second pass: bin packing for remaining files
    const remainingFiles = sortedFiles.filter(f => !used.has(f));
    
    for (const file of remainingFiles) {
        if (used.has(file)) continue;
        
        // Try to find an existing group that can fit this file
        let placed = false;
        for (const group of groups) {
            if (!group.needsSplitting && 
                group.totalCompressed + file.estimatedCompressed <= targetSize) {
                group.files.push(file);
                group.totalSize += file.size;
                group.totalCompressed += file.estimatedCompressed;
                used.add(file);
                placed = true;
                break;
            }
        }
        
        // Create new group if couldn't place in existing ones
        if (!placed) {
            const newGroup = {
                files: [file],
                totalSize: file.size,
                totalCompressed: file.estimatedCompressed,
                needsSplitting: false
            };
            groups.push(newGroup);
            used.add(file);
            
            // Try to add more files to this new group
            for (const otherFile of remainingFiles) {
                if (!used.has(otherFile) && 
                    newGroup.totalCompressed + otherFile.estimatedCompressed <= targetSize) {
                    newGroup.files.push(otherFile);
                    newGroup.totalSize += otherFile.size;
                    newGroup.totalCompressed += otherFile.estimatedCompressed;
                    used.add(otherFile);
                }
            }
        }
    }
    
    return groups;
}

/**
 * Create organized directory structure
 * @param {Array} groups - File groups
 * @param {string} outputDir - Output directory
 * @returns {Promise<void>}
 */
async function createOrganizedStructure(groups, outputDir) {
    // Create output directory
    await fs.mkdir(outputDir, { recursive: true });
    
    console.log(`📁 Creating ${groups.length} ZIP blocks in ${outputDir}`);
    
    for (let i = 0; i < groups.length; i++) {
        const group = groups[i];
        const blockDir = path.join(outputDir, `block_${i + 1}`);
        
        await fs.mkdir(blockDir, { recursive: true });
        
        console.log(`\n📦 Block ${i + 1}:`);
        console.log(`   Files: ${group.files.length}`);
        console.log(`   Original size: ${formatSize(group.totalSize)}`);
        console.log(`   Estimated ZIP size: ${formatSize(group.totalCompressed)}`);
        console.log(`   Needs splitting: ${group.needsSplitting ? 'Yes' : 'No'}`);
        
        if (group.needsSplitting) {
            // Split large files
            console.log(`   ⚠️  Large file will be split:`);
            for (const file of group.files) {
                console.log(`       ${file.relativePath} (${formatSize(file.size)})`);
                await splitLargeFile(file, blockDir, 100 * 1024); // 100KB chunks
            }
        } else {
            // Copy files to block directory
            for (const file of group.files) {
                const destPath = path.join(blockDir, path.basename(file.relativePath));
                await fs.copyFile(file.path, destPath);
                console.log(`       ✓ ${file.relativePath} (${formatSize(file.estimatedCompressed)} compressed)`);
            }
            
            // Create ZIP instructions file
            const zipInfo = {
                block: i + 1,
                files: group.files.map(f => ({
                    name: path.basename(f.relativePath),
                    originalPath: f.relativePath,
                    size: f.size,
                    estimatedCompressed: f.estimatedCompressed
                })),
                totalOriginalSize: group.totalSize,
                estimatedZipSize: group.totalCompressed,
                command: `zip -r block_${i + 1}.zip *`
            };
            
            await fs.writeFile(
                path.join(blockDir, 'zip_info.json'),
                JSON.stringify(zipInfo, null, 2)
            );
        }
    }
    
    // Create summary report
    const summary = {
        totalBlocks: groups.length,
        blocksNeedingSplit: groups.filter(g => g.needsSplitting).length,
        totalFiles: groups.reduce((sum, g) => sum + g.files.length, 0),
        totalOriginalSize: groups.reduce((sum, g) => sum + g.totalSize, 0),
        totalEstimatedCompressed: groups.reduce((sum, g) => sum + g.totalCompressed, 0),
        averageBlockSize: Math.round(groups.reduce((sum, g) => sum + g.totalCompressed, 0) / groups.length),
        blocks: groups.map((g, i) => ({
            blockNumber: i + 1,
            fileCount: g.files.length,
            originalSize: g.totalSize,
            estimatedCompressed: g.totalCompressed,
            needsSplitting: g.needsSplitting,
            efficiency: ((g.totalCompressed / (100 * 1024)) * 100).toFixed(1) + '%'
        }))
    };
    
    await fs.writeFile(
        path.join(outputDir, 'organization_summary.json'),
        JSON.stringify(summary, null, 2)
    );
    
    console.log(`\n📊 Summary:`);
    console.log(`   Total blocks: ${summary.totalBlocks}`);
    console.log(`   Total files: ${summary.totalFiles}`);
    console.log(`   Original size: ${formatSize(summary.totalOriginalSize)}`);
    console.log(`   Estimated compressed: ${formatSize(summary.totalEstimatedCompressed)}`);
    console.log(`   Compression ratio: ${(summary.totalEstimatedCompressed / summary.totalOriginalSize * 100).toFixed(1)}%`);
    console.log(`   Average block size: ${formatSize(summary.averageBlockSize)}`);
}

/**
 * Split a large file into chunks
 * @param {Object} fileInfo - File information
 * @param {string} outputDir - Output directory
 * @param {number} chunkSize - Chunk size in bytes
 * @returns {Promise<void>}
 */
async function splitLargeFile(fileInfo, outputDir, chunkSize) {
    const fileName = path.basename(fileInfo.relativePath);
    const fileExtension = path.extname(fileName);
    const baseName = path.basename(fileName, fileExtension);
    
    const readStream = createReadStream(fileInfo.path);
    let chunkIndex = 0;
    let currentChunkSize = 0;
    let currentWriteStream = null;
    
    const chunks = [];
    
    return new Promise((resolve, reject) => {
        function createNewChunk() {
            if (currentWriteStream) {
                currentWriteStream.end();
            }
            
            chunkIndex++;
            const chunkFileName = `${baseName}.part${chunkIndex.toString().padStart(3, '0')}${fileExtension}`;
            const chunkPath = path.join(outputDir, chunkFileName);
            
            chunks.push({
                name: chunkFileName,
                index: chunkIndex
            });
            
            currentWriteStream = createWriteStream(chunkPath);
            currentChunkSize = 0;
        }
        
        createNewChunk();
        
        readStream.on('data', (chunk) => {
            if (currentChunkSize + chunk.length > chunkSize && currentChunkSize > 0) {
                createNewChunk();
            }
            
            currentWriteStream.write(chunk);
            currentChunkSize += chunk.length;
        });
        
        readStream.on('end', () => {
            if (currentWriteStream) {
                currentWriteStream.end();
            }
            
            // Create split info file
            const splitInfo = {
                originalFile: fileInfo.relativePath,
                originalSize: fileInfo.size,
                chunkSize: chunkSize,
                chunks: chunks,
                totalChunks: chunkIndex,
                joinCommand: `cat ${baseName}.part* > ${fileName}`
            };
            
            fs.writeFile(
                path.join(outputDir, `${baseName}_split_info.json`),
                JSON.stringify(splitInfo, null, 2)
            ).then(resolve).catch(reject);
        });
        
        readStream.on('error', reject);
    });
}

/**
 * Format file size for display
 * @param {number} bytes - Size in bytes
 * @returns {string} Formatted size
 */
function formatSize(bytes) {
    const units = ['B', 'KB', 'MB', 'GB'];
    let size = bytes;
    let unitIndex = 0;
    
    while (size >= 1024 && unitIndex < units.length - 1) {
        size /= 1024;
        unitIndex++;
    }
    
    return `${size.toFixed(unitIndex === 0 ? 0 : 1)}${units[unitIndex]}`;
}

/**
 * Parse size string to bytes
 * @param {string} sizeStr - Size string (e.g., '100KB')
 * @returns {number} Size in bytes
 */
function parseSize(sizeStr) {
    if (!sizeStr) return 100 * 1024; // Default 100KB
    
    const match = sizeStr.match(/^([\d.]+)\s*(B|KB|MB|GB)$/i);
    if (!match) throw new Error(`Invalid size format: ${sizeStr}`);
    
    const [, number, unit] = match;
    const multipliers = { B: 1, KB: 1024, MB: 1024**2, GB: 1024**3 };
    
    return Math.round(parseFloat(number) * multipliers[unit.toUpperCase()]);
}

// CLI Setup
program
    .version('2.0.0')
    .description('Smart File Splitter with ZIP Organization');

program
    .command('organize')
    .description('Organize directory into optimal ZIP blocks')
    .argument('<directory>', 'Directory to organize')
    .option('-s, --target-size <size>', 'Target ZIP size (default: 100KB)', '100KB')
    .option('-o, --output <dir>', 'Output directory', 'organized_blocks')
    .action(async (directory, options) => {
        try {
            const targetSize = parseSize(options.targetSize);
            console.log(`🔍 Analyzing directory: ${directory}`);
            console.log(`🎯 Target ZIP size: ${formatSize(targetSize)}`);
            
            const files = await analyzeDirectory(directory);
            if (files.length === 0) {
                console.log('❌ No files found to organize');
                return;
            }
            
            console.log(`📁 Found ${files.length} files`);
            
            const groups = groupFilesIntoBlocks(files, targetSize);
            await createOrganizedStructure(groups, options.output);
            
            console.log(`\n✅ Organization complete! Check ${options.output} directory`);
            
        } catch (error) {
            console.error('❌ Error:', error.message);
            process.exit(1);
        }
    });

program
    .command('split')
    .description('Split a single large file')
    .argument('<file>', 'File to split')
    .option('-s, --chunk-size <size>', 'Chunk size (default: 100KB)', '100KB')
    .option('-o, --output <dir>', 'Output directory', '.')
    .action(async (file, options) => {
        try {
            const chunkSize = parseSize(options.chunkSize);
            const stats = await fs.stat(file);
            
            console.log(`📄 Splitting: ${file} (${formatSize(stats.size)})`);
            console.log(`✂️  Chunk size: ${formatSize(chunkSize)}`);
            
            const fileInfo = {
                path: file,
                relativePath: path.basename(file),
                size: stats.size
            };
            
            await splitLargeFile(fileInfo, options.output, chunkSize);
            console.log(`✅ Split complete!`);
            
        } catch (error) {
            console.error('❌ Error:', error.message);
            process.exit(1);
        }
    });

if (require.main === module) {
    program.parse();
}

module.exports = {
    analyzeDirectory,
    groupFilesIntoBlocks,
    estimateCompressedSize,
    formatSize,
    parseSize
};