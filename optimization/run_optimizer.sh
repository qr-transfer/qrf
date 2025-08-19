#!/bin/bash
# This script is used to run the optimization process for the VDF QR decoder.
# save the current directory
current_dir=$(pwd)
# change to the directory of the script
cd "$(dirname "$0")"
# check if installed
npm install
# run the optimize script
npm run optimize ../qrcoder/vdf-qr-decoder.html ./dist/vdf-qr-decoder-min.html 
npm run optimize ../qrcoder/vde-qr-encoder.html ./dist/vde-qr-encoder-min.html 
# change back to the original directory
cd "$current_dir"
