#!/usr/bin/env python3
"""
File Splitter Utility
Splits files into smaller chunks with configurable parameters.

Usage:
    python file_splitter.py --split <file> [options]
    python file_splitter.py --join <pattern> [options]

Examples:
    # Split with default 100KB chunks
    python file_splitter.py --split video.mp4
    
    # Split with custom chunk size
    python file_splitter.py --split document.pdf --chunk-size 50KB --output-dir ./chunks
    
    # Join files back
    python file_splitter.py --join video.mp4.part --output merged_video.mp4
"""

import os
import sys
import argparse
import glob
from pathlib import Path

def parse_size(size_str):
    """Parse size string (e.g., '100KB', '1.5MB', '2GB') to bytes."""
    if not size_str:
        return 100 * 1024  # Default 100KB
    
    size_str = size_str.upper().strip()
    
    # Extract number and unit
    units = {'B': 1, 'KB': 1024, 'MB': 1024**2, 'GB': 1024**3, 'TB': 1024**4}
    
    for unit, multiplier in units.items():
        if size_str.endswith(unit):
            try:
                number = float(size_str[:-len(unit)])
                return int(number * multiplier)
            except ValueError:
                pass
    
    # Try to parse as plain number (assume bytes)
    try:
        return int(size_str)
    except ValueError:
        raise ValueError(f"Invalid size format: {size_str}")

def format_size(bytes_val):
    """Format bytes to human readable string."""
    for unit in ['B', 'KB', 'MB', 'GB', 'TB']:
        if bytes_val < 1024.0:
            return f"{bytes_val:.1f}{unit}"
        bytes_val /= 1024.0
    return f"{bytes_val:.1f}PB"

def split_file(input_path, chunk_size=100*1024, output_dir=None):
    """
    Split a file into chunks.
    
    Args:
        input_path (str): Path to the file to split
        chunk_size (int): Size of each chunk in bytes (default: 100KB)
        output_dir (str): Output directory (default: same as input file)
    
    Returns:
        list: List of created chunk file paths
    """
    input_path = Path(input_path)
    
    if not input_path.exists():
        raise FileNotFoundError(f"Input file not found: {input_path}")
    
    if not input_path.is_file():
        raise ValueError(f"Input path is not a file: {input_path}")
    
    # Determine output directory
    if output_dir is None:
        output_dir = input_path.parent
    else:
        output_dir = Path(output_dir)
        output_dir.mkdir(parents=True, exist_ok=True)
    
    file_size = input_path.stat().st_size
    chunk_files = []
    
    print(f"Splitting '{input_path.name}' ({format_size(file_size)}) into {format_size(chunk_size)} chunks...")
    
    with open(input_path, 'rb') as infile:
        chunk_num = 0
        
        while True:
            chunk_data = infile.read(chunk_size)
            if not chunk_data:
                break
            
            # Create chunk filename with zero-padded numbers
            chunk_filename = f"{input_path.name}.part{chunk_num:04d}"
            chunk_path = output_dir / chunk_filename
            
            with open(chunk_path, 'wb') as chunk_file:
                chunk_file.write(chunk_data)
            
            chunk_files.append(chunk_path)
            print(f"Created: {chunk_filename} ({format_size(len(chunk_data))})")
            chunk_num += 1
    
    # Create metadata file
    metadata_path = output_dir / f"{input_path.name}.meta"
    with open(metadata_path, 'w') as meta_file:
        meta_file.write(f"original_name={input_path.name}\n")
        meta_file.write(f"original_size={file_size}\n")
        meta_file.write(f"chunk_size={chunk_size}\n")
        meta_file.write(f"total_chunks={len(chunk_files)}\n")
        meta_file.write(f"chunks={','.join([f.name for f in chunk_files])}\n")
    
    print(f"\nSplit complete!")
    print(f"Created {len(chunk_files)} chunks in: {output_dir}")
    print(f"Metadata file: {metadata_path}")
    
    return chunk_files

def join_files(pattern, output_path=None):
    """
    Join split files back together.
    
    Args:
        pattern (str): Pattern to match chunk files (e.g., 'file.txt.part')
        output_path (str): Output file path (optional)
    
    Returns:
        str: Path to the joined file
    """
    # Find all matching chunk files
    if pattern.endswith('.part'):
        # Pattern like 'file.txt.part' - find all numbered parts
        chunk_pattern = f"{pattern}[0-9]*"
    else:
        chunk_pattern = pattern
    
    chunk_files = sorted(glob.glob(chunk_pattern))
    
    if not chunk_files:
        raise FileNotFoundError(f"No chunk files found matching pattern: {chunk_pattern}")
    
    # Try to find metadata file
    base_name = pattern.replace('.part', '') if pattern.endswith('.part') else pattern
    meta_path = f"{base_name}.meta"
    
    original_name = None
    if os.path.exists(meta_path):
        try:
            with open(meta_path, 'r') as meta_file:
                for line in meta_file:
                    if line.startswith('original_name='):
                        original_name = line.split('=', 1)[1].strip()
                        break
        except Exception as e:
            print(f"Warning: Could not read metadata file: {e}")
    
    # Determine output path
    if output_path is None:
        if original_name:
            output_path = original_name
        else:
            # Strip .part from the first chunk file
            first_chunk = Path(chunk_files[0])
            if '.part' in first_chunk.name:
                output_path = first_chunk.name.split('.part')[0]
            else:
                output_path = f"joined_{first_chunk.name}"
    
    print(f"Joining {len(chunk_files)} chunks into '{output_path}'...")
    
    total_size = 0
    with open(output_path, 'wb') as outfile:
        for i, chunk_file in enumerate(chunk_files):
            print(f"Processing chunk {i+1}/{len(chunk_files)}: {os.path.basename(chunk_file)}")
            
            if not os.path.exists(chunk_file):
                raise FileNotFoundError(f"Chunk file not found: {chunk_file}")
            
            with open(chunk_file, 'rb') as chunk:
                chunk_data = chunk.read()
                outfile.write(chunk_data)
                total_size += len(chunk_data)
    
    print(f"\nJoin complete!")
    print(f"Output file: {output_path} ({format_size(total_size)})")
    
    return output_path

def main():
    parser = argparse.ArgumentParser(
        description="File Splitter/Joiner Utility",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Split file with default 100KB chunks
  python file_splitter.py --split video.mp4
  
  # Split with custom chunk size and output directory
  python file_splitter.py --split document.pdf --chunk-size 50KB --output-dir ./chunks
  
  # Join files back together
  python file_splitter.py --join video.mp4.part
  
  # Join with specific output name
  python file_splitter.py --join video.mp4.part --output merged_video.mp4

Supported size formats: 100B, 50KB, 1.5MB, 2GB, etc.
        """
    )
    
    # Main operation
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('--split', metavar='FILE', help='Split the specified file')
    group.add_argument('--join', metavar='PATTERN', help='Join files matching the pattern')
    
    # Options for splitting
    parser.add_argument('--chunk-size', '-s', default='100KB',
                       help='Chunk size (default: 100KB). Examples: 50KB, 1MB, 2.5GB')
    parser.add_argument('--output-dir', '-d',
                       help='Output directory (default: same as input file)')
    
    # Options for joining
    parser.add_argument('--output', '-o',
                       help='Output file name for joining (default: auto-detect from metadata)')
    
    # General options
    parser.add_argument('--verbose', '-v', action='store_true',
                       help='Enable verbose output')
    
    args = parser.parse_args()
    
    try:
        if args.split:
            # Split operation
            chunk_size = parse_size(args.chunk_size)
            chunk_files = split_file(args.split, chunk_size, args.output_dir)
            
            if args.verbose:
                print(f"\nCreated chunks:")
                for chunk in chunk_files:
                    size = os.path.getsize(chunk)
                    print(f"  {chunk} ({format_size(size)})")
        
        elif args.join:
            # Join operation
            output_file = join_files(args.join, args.output)
            
            if args.verbose:
                final_size = os.path.getsize(output_file)
                print(f"Final file size: {format_size(final_size)}")
    
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)

if __name__ == "__main__":
    main()