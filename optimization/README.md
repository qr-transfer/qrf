# QR Encoder/Decoder Optimizer

This tool optimizes QR code encoder/decoder HTML files for production distribution by:
- Minifying CSS using CSSO
- Optimizing JavaScript with Terser
- Minifying HTML with html-minifier
- Generating optimization reports

## Setup

Install dependencies:

```bash
npm install
```

## Usage

Optimize a file:

```bash
node optimize.js <input-file.html> <output-file.html>
```

Example:

```bash
node optimize.js unified-qr-decoder-v9.html unified-qr-decoder.html
```

Or use the npm script:

```bash
npm run optimize unified-qr-decoder-v9.html unified-qr-decoder.html
```

## Optimization Process

The script performs:
1. CSS minification with CSSO
2. JavaScript optimization with Terser
   - Removes console.log statements
   - Mangles variable names
   - Removes comments and whitespace
3. HTML minification
   - Collapses whitespace
   - Removes redundant attributes
   - Streamlines document structure

After optimization, a report is generated showing size reduction and applied optimizations.