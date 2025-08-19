// Script to optimize QR encoder/decoder HTML files
const fs = require('fs');
const path = require('path');
const { minify } = require('terser');
const CleanCSS = require('clean-css');
const { minify: minifyHtml } = require('html-minifier-terser');
const axios = require('axios');

// Get command line arguments
const args = process.argv.slice(2);
let inputFile, outputFile, inlineExternal = false;

// Parse command line arguments
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--inline-external' || args[i] === '-i') {
    inlineExternal = true;
  } else if (!inputFile) {
    inputFile = args[i];
  } else if (!outputFile) {
    outputFile = args[i];
  }
}

if (!inputFile || !outputFile) {
  console.error('Usage: node optimize.js [options] <input-file.html> <output-file.html>');
  console.error('Options:');
  console.error('  --inline-external, -i  Incorporate external JavaScript into the final file');
  process.exit(1);
}

// Validate file exists
if (!fs.existsSync(inputFile)) {
  console.error(`Error: Input file "${inputFile}" does not exist.`);
  process.exit(1);
}

// Ensure output directory exists
const outputDir = path.dirname(outputFile);
if (!fs.existsSync(outputDir)) {
  fs.mkdirSync(outputDir, { recursive: true });
}

// Main optimization function
async function optimizeFile() {
  try {
    // Read the original file
    let content = fs.readFileSync(inputFile, 'utf8');
    
    // Remove any [object Promise] text that might be leftover from previous runs
    content = content.replace(/\[object Promise\]/g, '');
    
    // Trim large template literals that cause HTML minification issues
    content = content.replace(/const mobileStyles = `[\s\S]*?`;/g, 'const mobileStyles = "";');
    
    console.log(`Optimizing ${inputFile}...`);
    
    // 1. Extract and optimize CSS
    const cleanCss = new CleanCSS({ 
      level: {
        1: { all: true },
        2: { all: true }
      }
    });
    
    let styleRegex = /(<style[^>]*>)([\s\S]*?)(<\/style>)/g;
    content = content.replace(styleRegex, (match, openTag, cssContent, closeTag) => {
      // Optimize CSS with CleanCSS
      try {
        const minifiedCss = cleanCss.minify(cssContent).styles;
        return `${openTag}${minifiedCss}${closeTag}`;
      } catch (err) {
        console.warn('CSS minification error:', err.message);
        return match; // Return original on error
      }
    });

    // 2. Handle script tags
    const scriptTags = [];
    let scriptRegex = /<script([^>]*)>([\s\S]*?)<\/script>/g;
    let scriptMatch;
    let lastIndex = 0;
    const contentParts = [];
    
    while ((scriptMatch = scriptRegex.exec(content)) !== null) {
      // Add text before the script tag
      contentParts.push(content.substring(lastIndex, scriptMatch.index));
      
      // Extract script attributes and content
      const fullMatch = scriptMatch[0];
      const scriptAttrs = scriptMatch[1];
      const scriptContent = scriptMatch[2];
      
      // Check if it's an external script
      const srcMatch = scriptAttrs.match(/src=["']([^"']+)["']/);
      
      if (srcMatch && inlineExternal) {
        // This is an external script and we want to inline it
        const scriptUrl = srcMatch[1];
        scriptTags.push({ 
          isExternal: true,
          url: scriptUrl,
          attrs: scriptAttrs.replace(/src=["'][^"']+["']/, ''),
          content: ''  // Will be fetched later
        });
      } else if (srcMatch && !inlineExternal) {
        // External script but not inlining - keep as is
        contentParts.push(fullMatch);
      } else {
        // Regular inline script
        scriptTags.push({
          isExternal: false,
          attrs: scriptAttrs,
          content: scriptContent
        });
      }
      
      // If we need to process this script, add a placeholder
      if (srcMatch && inlineExternal || !srcMatch) {
        contentParts.push(`##SCRIPT_PLACEHOLDER_${scriptTags.length - 1}##`);
      }
      
      lastIndex = scriptMatch.index + fullMatch.length;
    }
    
    // Add the remaining content
    contentParts.push(content.substring(lastIndex));
    
    // Join everything without scripts
    let contentWithoutScripts = contentParts.join('');
    
    // 3. Process external scripts if needed
    if (inlineExternal) {
      await Promise.all(scriptTags.map(async (scriptTag, index) => {
        if (scriptTag.isExternal) {
          try {
            let scriptContent;
            if (scriptTag.url.startsWith('http://') || scriptTag.url.startsWith('https://')) {
              // Fetch remote script
              const response = await axios.get(scriptTag.url);
              scriptContent = response.data;
              console.log(`Fetched external script: ${scriptTag.url}`);
            } else {
              // Load local script
              const scriptPath = path.resolve(path.dirname(inputFile), scriptTag.url);
              if (fs.existsSync(scriptPath)) {
                scriptContent = fs.readFileSync(scriptPath, 'utf8');
                console.log(`Loaded local script: ${scriptTag.url}`);
              } else {
                console.warn(`Local script not found: ${scriptTag.url}`);
                scriptContent = `/* Error loading script from ${scriptTag.url} */`;
              }
            }
            scriptTag.content = scriptContent;
          } catch (error) {
            console.warn(`Failed to fetch script from ${scriptTag.url}: ${error.message}`);
            scriptTag.content = `/* Error loading script from ${scriptTag.url} */`;
          }
        }
      }));
    }
    
    // 4. Process each script tag
    for (let i = 0; i < scriptTags.length; i++) {
      const { attrs, content, isExternal } = scriptTags[i];
      
      try {
        // Only minify non-empty scripts
        if (content.trim()) {
          const result = await minify(content, {
            compress: {
              drop_console: false,
              drop_debugger: true,
              ecma: 2020
            },
            mangle: true,
            format: {
              comments: false
            }
          });
          
          const minifiedJs = result.code;
          // Replace placeholder with minified script
          contentWithoutScripts = contentWithoutScripts.replace(
            `##SCRIPT_PLACEHOLDER_${i}##`, 
            `<script${attrs}>${minifiedJs}</script>`
          );
        } else {
          // Replace placeholder with original empty script
          contentWithoutScripts = contentWithoutScripts.replace(
            `##SCRIPT_PLACEHOLDER_${i}##`, 
            `<script${attrs}>${content}</script>`
          );
        }
      } catch (err) {
        console.warn(`JS minification error for script #${i+1}:`, err.message);
        // Replace placeholder with original script on error
        contentWithoutScripts = contentWithoutScripts.replace(
          `##SCRIPT_PLACEHOLDER_${i}##`, 
          `<script${attrs}>${content}</script>`
        );
      }
    }
    
    // 5. Optimize the entire HTML
    try {
      contentWithoutScripts = await minifyHtml(contentWithoutScripts, {
        collapseWhitespace: true,
        removeComments: true,
        removeRedundantAttributes: true,
        removeEmptyAttributes: true,
        removeScriptTypeAttributes: true,
        removeStyleLinkTypeAttributes: true,
        minifyCSS: false, // Already handled by CleanCSS
        minifyJS: false   // Already handled by Terser
      });
    } catch (err) {
      console.warn('HTML minification error:', err.message);
      // Continue with partially optimized content
    }
    
    // Write the optimized file
    fs.writeFileSync(outputFile, contentWithoutScripts, 'utf8');
    
    // Get file sizes
    const originalSize = fs.statSync(inputFile).size;
    const optimizedSize = fs.statSync(outputFile).size;
    const savedBytes = originalSize - optimizedSize;
    const savedPercentage = ((savedBytes / originalSize) * 100).toFixed(2);
    
    console.log('Optimization complete.');
    console.log(`Original size: ${originalSize} bytes (${(originalSize / 1024).toFixed(2)} KB)`);
    console.log(`Optimized size: ${optimizedSize} bytes (${(optimizedSize / 1024).toFixed(2)} KB)`);
    console.log(`Saved: ${savedBytes} bytes (${savedPercentage}%)`);
    
    // Create a summary report
    const reportFile = './optimization-report.md';
    const summary = `# Optimization Summary for ${path.basename(inputFile)}

## Size Reduction
- Original Size: ${(originalSize / 1024).toFixed(2)} KB
- Optimized Size: ${(optimizedSize / 1024).toFixed(2)} KB
- Bytes Saved: ${savedBytes} bytes (${savedPercentage}%)

## Optimizations Applied
1. **CSS Minification**
   - Removed whitespace, comments, and formatting
   - Optimized selectors and rules

2. **JavaScript Minification**
   - Shortened variable names
   - Eliminated unused code
   - Removed comments and whitespace
   ${inlineExternal ? '   - Incorporated external scripts into the final file' : ''}

3. **HTML Minification**
   - Compressed whitespace
   - Removed redundant attributes
   - Streamlined document structure

## Note
This file was optimized for production distribution using the optimize.js script.
`;
    
    fs.writeFileSync(reportFile, summary, 'utf8');
    console.log(`Detailed report created: ${reportFile}`);
    
  } catch (error) {
    console.error('Optimization failed:', error);
    process.exit(1);
  }
}

// Run the optimization
optimizeFile();