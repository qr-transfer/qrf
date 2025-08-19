#!/usr/bin/env node

/**
 * File Splitter Utility - Node.js Version
 * Splits files into smaller chunks with configurable parameters.
 * 
 * Usage:
 *   node file_splitter.js --split <file> [options]
 *   node file_splitter.js --join <pattern> [options]
 * 
 * Examples:
 *   # Split with default 100KB chunks
 *   node file_splitter.js --split video.mp4
 *   
 *   # Split with custom chunk size
 *   node file_splitter.js --split document.pdf --chunk-size 50KB --output-dir ./chunks
 *   
 *   # Join files back
 *   node file_splitter.js --join video.mp4.part --output merged_video.mp4
 */

const fs = require('fs');
const path = require('path');
const { program } = require('commander');

/**
 * Parse size string (e.g., '100KB', '1.5MB', '2GB') to bytes
 * @param {string} sizeStr - Size string to parse
 * @returns {number} Size in bytes
 */
function parseSize(sizeStr) {
    if (!sizeStr) {
        return 100 * 1024; // Default 100KB
    }
    
    const sizeUpper = sizeStr.toUpperCase().trim();
    const units = {
        'B': 1,
        'KB': 1024,
        'MB': 1024 ** 2,
        'GB': 1024 ** 3,
        'TB': 1024 ** 4
    };
    
    for (const [unit, multiplier] of Object.entries(units)) {
        if (sizeUpper.endsWith(unit)) {
            const number = parseFloat(sizeUpper.slice(0, -unit.length));
            if (isNaN(number)) {
                throw new Error(`Invalid size format: ${sizeStr}`);
            }
            return Math.floor(number * multiplier);
        }
    }
    
    // Try to parse as plain number (assume bytes)
    const number = parseInt(sizeStr);
    if (isNaN(number)) {
        throw new Error(`Invalid size format: ${sizeStr}`);
    }
    return number;
}

/**
 * Format bytes to human readable string
 * @param {number} bytes - Number of bytes
 * @returns {string} Formatted size string
 */
function formatSize(bytes) {
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = bytes;
    let unitIndex = 0;
    
    while (size >= 1024 && unitIndex < units.length - 1) {
        size /= 1024;
        unitIndex++;
    }
    
    return `${size.toFixed(1)}${units[unitIndex]}`;
}

/**
 * Split a file into chunks
 * @param {string} inputPath - Path to the file to split
 * @param {number} chunkSize - Size of each chunk in bytes
 * @param {string} outputDir - Output directory
 * @returns {Promise<string[]>} Array of created chunk file paths
 */
async function splitFile(inputPath, chunkSize = 100 * 1024, outputDir = null) {
    return new Promise((resolve, reject) => {
        // Check if input file exists
        if (!fs.existsSync(inputPath)) {
            reject(new Error(`Input file not found: ${inputPath}`));
            return;
        }
        
        const stats = fs.statSync(inputPath);
        if (!stats.isFile()) {
            reject(new Error(`Input path is not a file: ${inputPath}`));
            return;
        }
        
        // Determine output directory
        const inputDir = path.dirname(inputPath);
        const actualOutputDir = outputDir || inputDir;
        
        // Create output directory if it doesn't exist
        if (!fs.existsSync(actualOutputDir)) {
            fs.mkdirSync(actualOutputDir, { recursive: true });
        }
        
        const fileName = path.basename(inputPath);
        const fileSize = stats.size;
        const chunkFiles = [];
        
        console.log(`Splitting '${fileName}' (${formatSize(fileSize)}) into ${formatSize(chunkSize)} chunks...`);
        
        const readStream = fs.createReadStream(inputPath, { highWaterMark: chunkSize });
        let chunkNum = 0;
        let buffer = Buffer.alloc(0);
        
        readStream.on('data', (chunk) => {
            buffer = Buffer.concat([buffer, chunk]);
            
            while (buffer.length >= chunkSize) {
                const chunkData = buffer.slice(0, chunkSize);
                buffer = buffer.slice(chunkSize);
                
                writeChunk(chunkData, chunkNum);
                chunkNum++;
            }
        });
        
        readStream.on('end', () => {
            // Write remaining data as final chunk
            if (buffer.length > 0) {
                writeChunk(buffer, chunkNum);
                chunkNum++;
            }
            
            // Create metadata file
            const metadataPath = path.join(actualOutputDir, `${fileName}.meta`);
            const metadata = [
                `original_name=${fileName}`,
                `original_size=${fileSize}`,
                `chunk_size=${chunkSize}`,
                `total_chunks=${chunkFiles.length}`,
                `chunks=${chunkFiles.map(f => path.basename(f)).join(',')}`
            ].join('\n');
            
            fs.writeFileSync(metadataPath, metadata);
            
            console.log(`\nSplit complete!`);
            console.log(`Created ${chunkFiles.length} chunks in: ${actualOutputDir}`);
            console.log(`Metadata file: ${metadataPath}`);
            
            resolve(chunkFiles);
        });
        
        readStream.on('error', reject);
        
        function writeChunk(data, num) {
            const chunkFileName = `${fileName}.part${num.toString().padStart(4, '0')}`;
            const chunkPath = path.join(actualOutputDir, chunkFileName);
            
            fs.writeFileSync(chunkPath, data);
            chunkFiles.push(chunkPath);
            console.log(`Created: ${chunkFileName} (${formatSize(data.length)})`);
        }
    });
}

/**
 * Join split files back together
 * @param {string} pattern - Pattern to match chunk files
 * @param {string} outputPath - Output file path
 * @returns {Promise<string>} Path to the joined file
 */
async function joinFiles(pattern, outputPath = null) {
    return new Promise((resolve, reject) => {
        // Find all matching chunk files
        const glob = require('glob');
        
        let chunkPattern;
        if (pattern.endsWith('.part')) {
            chunkPattern = `${pattern}[0-9]*`;
        } else {
            chunkPattern = pattern;
        }
        
        const chunkFiles = glob.sync(chunkPattern).sort();
        
        if (chunkFiles.length === 0) {
            reject(new Error(`No chunk files found matching pattern: ${chunkPattern}`));
            return;
        }
        
        // Try to find metadata file
        const baseName = pattern.endsWith('.part') ? pattern.replace('.part', '') : pattern;
        const metaPath = `${baseName}.meta`;
        
        let originalName = null;
        if (fs.existsSync(metaPath)) {
            try {
                const metadata = fs.readFileSync(metaPath, 'utf8');
                const lines = metadata.split('\n');
                for (const line of lines) {
                    if (line.startsWith('original_name=')) {
                        originalName = line.split('=', 2)[1];
                        break;
                    }
                }
            } catch (error) {
                console.warn(`Warning: Could not read metadata file: ${error.message}`);
            }
        }
        
        // Determine output path
        let finalOutputPath = outputPath;
        if (!finalOutputPath) {
            if (originalName) {
                finalOutputPath = originalName;
            } else {
                // Strip .part from the first chunk file
                const firstChunk = path.basename(chunkFiles[0]);
                if (firstChunk.includes('.part')) {
                    finalOutputPath = firstChunk.split('.part')[0];
                } else {
                    finalOutputPath = `joined_${firstChunk}`;
                }
            }
        }
        
        console.log(`Joining ${chunkFiles.length} chunks into '${finalOutputPath}'...`);
        
        const writeStream = fs.createWriteStream(finalOutputPath);
        let totalSize = 0;
        let currentIndex = 0;
        
        function processNextChunk() {
            if (currentIndex >= chunkFiles.length) {
                writeStream.end();
                return;
            }
            
            const chunkFile = chunkFiles[currentIndex];
            console.log(`Processing chunk ${currentIndex + 1}/${chunkFiles.length}: ${path.basename(chunkFile)}`);
            
            if (!fs.existsSync(chunkFile)) {
                reject(new Error(`Chunk file not found: ${chunkFile}`));
                return;
            }
            
            const readStream = fs.createReadStream(chunkFile);
            
            readStream.on('data', (chunk) => {
                writeStream.write(chunk);
                totalSize += chunk.length;
            });
            
            readStream.on('end', () => {
                currentIndex++;
                processNextChunk();
            });
            
            readStream.on('error', reject);
        }
        
        writeStream.on('close', () => {
            console.log(`\nJoin complete!`);
            console.log(`Output file: ${finalOutputPath} (${formatSize(totalSize)})`);
            resolve(finalOutputPath);
        });
        
        writeStream.on('error', reject);
        
        processNextChunk();
    });
}

// Check if glob is installed
try {
    require('glob');
} catch (error) {
    console.error('Error: This script requires the "glob" package.');
    console.error('Install it with: npm install glob');
    console.error('Or install commander and glob: npm install commander glob');
    process.exit(1);
}

// Check if commander is installed
try {
    require('commander');
} catch (error) {
    console.error('Error: This script requires the "commander" package.');
    console.error('Install it with: npm install commander');
    console.error('Or install both packages: npm install commander glob');
    process.exit(1);
}

// CLI setup
program
    .name('file_splitter')
    .description('File Splitter/Joiner Utility')
    .version('1.0.0');

program
    .option('--split <file>', 'Split the specified file')
    .option('--join <pattern>', 'Join files matching the pattern')
    .option('--chunk-size <size>', 'Chunk size (default: 100KB). Examples: 50KB, 1MB, 2.5GB', '100KB')
    .option('--output-dir <dir>', 'Output directory (default: same as input file)')
    .option('--output <file>', 'Output file name for joining (default: auto-detect from metadata)')
    .option('--verbose', 'Enable verbose output')
    .addHelpText('after', `
Examples:
  # Split file with default 100KB chunks
  node file_splitter.js --split video.mp4
  
  # Split with custom chunk size and output directory
  node file_splitter.js --split document.pdf --chunk-size 50KB --output-dir ./chunks
  
  # Join files back together
  node file_splitter.js --join video.mp4.part
  
  # Join with specific output name
  node file_splitter.js --join video.mp4.part --output merged_video.mp4

Supported size formats: 100B, 50KB, 1.5MB, 2GB, etc.
`);

program.parse();

const options = program.opts();

async function main() {
    try {
        if (options.split && options.join) {
            throw new Error('Cannot specify both --split and --join');
        }
        
        if (!options.split && !options.join) {
            throw new Error('Must specify either --split or --join');
        }
        
        if (options.split) {
            // Split operation
            const chunkSize = parseSize(options.chunkSize);
            const chunkFiles = await splitFile(options.split, chunkSize, options.outputDir);
            
            if (options.verbose) {
                console.log('\nCreated chunks:');
                for (const chunk of chunkFiles) {
                    const size = fs.statSync(chunk).size;
                    console.log(`  ${chunk} (${formatSize(size)})`);
                }
            }
        } else if (options.join) {
            // Join operation
            const outputFile = await joinFiles(options.join, options.output);
            
            if (options.verbose) {
                const finalSize = fs.statSync(outputFile).size;
                console.log(`Final file size: ${formatSize(finalSize)}`);
            }
        }
    } catch (error) {
        console.error(`Error: ${error.message}`);
        process.exit(1);
    }
}

main();