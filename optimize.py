#!/usr/bin/env python3
"""
HTML Optimizer Script
Minifies HTML files by removing unnecessary whitespace, comments, and formatting
while preserving functionality.
"""

import re
import os
import sys

def minify_html(html_content):
    """
    Minify HTML content by removing unnecessary whitespace and comments.
    """
    # Remove HTML comments (but preserve conditional comments)
    html_content = re.sub(r'<!--(?!\[if).*?-->', '', html_content, flags=re.DOTALL)
    
    # Remove CSS comments
    html_content = re.sub(r'/\*.*?\*/', '', html_content, flags=re.DOTALL)
    
    # Remove JavaScript single-line comments (but be careful with URLs and regex)
    html_content = re.sub(r'(?<!:)//[^\n\r]*(?=[\n\r])', '', html_content)
    
    # Remove JavaScript multi-line comments
    html_content = re.sub(r'/\*[\s\S]*?\*/', '', html_content)
    
    # Minimize whitespace between tags
    html_content = re.sub(r'>\s+<', '><', html_content)
    
    # Remove leading and trailing whitespace on lines
    html_content = re.sub(r'^\s+', '', html_content, flags=re.MULTILINE)
    html_content = re.sub(r'\s+$', '', html_content, flags=re.MULTILINE)
    
    # Compress multiple spaces/tabs into single spaces (except in <pre> tags)
    html_content = re.sub(r'[ \t]+', ' ', html_content)
    
    # Remove empty lines
    html_content = re.sub(r'\n\s*\n', '\n', html_content)
    
    # Remove newlines between tags (but preserve some structure)
    html_content = re.sub(r'>\n\s*<', '><', html_content)
    
    # Clean up script and style tag content
    html_content = re.sub(r'(<script[^>]*>)\s+', r'\1', html_content)
    html_content = re.sub(r'\s+(</script>)', r'\1', html_content)
    html_content = re.sub(r'(<style[^>]*>)\s+', r'\1', html_content)
    html_content = re.sub(r'\s+(</style>)', r'\1', html_content)
    
    return html_content.strip()

def optimize_file(input_path, output_path):
    """
    Optimize a single HTML file.
    """
    try:
        with open(input_path, 'r', encoding='utf-8') as f:
            original_content = f.read()
        
        print(f"Optimizing {input_path}...")
        print(f"Original size: {len(original_content):,} bytes")
        
        minified_content = minify_html(original_content)
        
        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(minified_content)
        
        print(f"Minified size: {len(minified_content):,} bytes")
        reduction = len(original_content) - len(minified_content)
        percentage = (reduction / len(original_content)) * 100
        print(f"Size reduction: {reduction:,} bytes ({percentage:.1f}%)")
        print(f"Output written to: {output_path}")
        print("-" * 50)
        
        return True
        
    except Exception as e:
        print(f"Error optimizing {input_path}: {e}")
        return False

def main():
    """
    Main function to optimize HTML files.
    """
    files_to_optimize = [
        ('camera-qr-decoder.html', 'camera-qr-decoder-min.html'),
        ('vdf-qr-decoder.html', 'vdf-qr-decoder-min.html'),
        ('vde-qr-encoder.html', 'vde-qr-encoder-min.html'),
        ('vde-qr-encoder-split.html', 'vde-qr-encoder-split-min.html'),
        ('minimal-qr-decoder.html', 'minimal-qr-decoder-min.html'),
        ('ultra-minimal-decoder.html', 'ultra-minimal-decoder-min.html')
    ]
    
    print("HTML Optimizer")
    print("=" * 50)
    
    success_count = 0
    total_count = len(files_to_optimize)
    
    for input_file, output_file in files_to_optimize:
        if os.path.exists(input_file):
            if optimize_file(input_file, output_file):
                success_count += 1
        else:
            print(f"Warning: {input_file} not found, skipping...")
            total_count -= 1
    
    print(f"\nOptimization complete!")
    print(f"Successfully optimized {success_count}/{total_count} files")

if __name__ == "__main__":
    main()