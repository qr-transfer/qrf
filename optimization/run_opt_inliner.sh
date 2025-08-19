#!/bin/bash
# This script is used to run the optimization process for all QR code tools
# save the current directory
current_dir=$(pwd)
# change to the directory of the script
cd "$(dirname "$0")"
# check if installed
npm install
# run the optimize script for all files
echo "Optimizing video decoder with inline option..."
npm run optimize -- --inline-external ../qrcoder/vdf-qr-decoder.html ./dist/vdf-qr-decoder-std.html 

echo "Optimizing encoder with inline option..."
npm run optimize -- --inline-external ../qrcoder/vde-qr-encoder.html ./dist/vde-qr-encoder-std.html 

echo "Optimizing camera decoder with inline option..."
npm run optimize -- --inline-external ../qrcoder/camera-qr-decoder.html ./dist/camera-qr-decoder-std.html

echo "Optimization complete!"
echo "The optimized files are available in ./dist/ directory with -std.html extension"

# Also create copies in the qrcoder directory with -min.html extension for direct testing
echo "Creating additional copies in the qrcoder directory with -min.html extension..."
cp ./dist/vdf-qr-decoder-std.html ../qrcoder/vdf-qr-decoder-min.html
cp ./dist/vde-qr-encoder-std.html ../qrcoder/vde-qr-encoder-min.html
cp ./dist/camera-qr-decoder-std.html ../qrcoder/camera-qr-decoder-min.html

# change back to the original directory
cd "$current_dir"
