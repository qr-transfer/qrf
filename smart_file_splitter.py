#!/usr/bin/env python3
"""
Smart File Splitter with ZIP Organization
Groups files into optimal ZIP blocks of ~100KB each

Usage:
    python smart_file_splitter.py organize <directory> [options]
    python smart_file_splitter.py split <file> [options]

Examples:
    # Organize directory into ZIP blocks
    python smart_file_splitter.py organize ./files --target-size 100KB
    
    # Split single large file
    python smart_file_splitter.py split large_file.pdf --target-size 100KB
"""

import os
import sys
import argparse
import json
import shutil
from pathlib import Path
from typing import List, Dict, Tuple, Any
import math


def estimate_compressed_size(file_path: str, file_size: int) -> int:
    """
    Estimate compressed size using compression ratio heuristics.
    
    Args:
        file_path: Path to file
        file_size: File size in bytes
    
    Returns:
        Estimated compressed size in bytes
    """
    ext = Path(file_path).suffix.lower()
    
    # Compression ratios based on file types (approximate)
    compression_ratios = {
        # Already compressed formats (minimal compression)
        '.zip': 1.0, '.rar': 1.0, '.7z': 1.0, '.gz': 1.0, '.bz2': 1.0,
        '.jpg': 1.0, '.jpeg': 1.0, '.png': 1.05, '.gif': 1.0,
        '.mp4': 1.0, '.avi': 1.0, '.mkv': 1.0, '.mov': 1.0,
        '.mp3': 1.0, '.flac': 1.0, '.aac': 1.0, '.ogg': 1.0,
        '.pdf': 1.1, '.docx': 1.1, '.xlsx': 1.1, '.pptx': 1.1,
        
        # Text and data files (good compression)
        '.txt': 0.3, '.md': 0.35, '.csv': 0.4, '.json': 0.4, '.xml': 0.5,
        '.html': 0.4, '.css': 0.35, '.js': 0.45, '.ts': 0.45,
        '.py': 0.4, '.java': 0.45, '.cpp': 0.45, '.c': 0.45,
        '.log': 0.2, '.sql': 0.4, '.yaml': 0.4, '.yml': 0.4,
        
        # Binary data (moderate compression)
        '.exe': 0.7, '.dll': 0.7, '.bin': 0.8, '.dat': 0.6,
        '.db': 0.6, '.sqlite': 0.6, '.iso': 0.9,
        
        # Images (varies)
        '.bmp': 0.1, '.tiff': 0.4, '.svg': 0.3, '.ico': 0.8,
        
        # Documents
        '.doc': 0.5, '.rtf': 0.4, '.odt': 0.7, '.tex': 0.4
    }
    
    ratio = compression_ratios.get(ext, 0.6)  # Default 60% compression
    
    # For very small files, add ZIP overhead
    zip_overhead = min(100, file_size * 0.1)  # ~10% overhead, max 100 bytes
    estimated_size = max(zip_overhead, file_size * ratio + zip_overhead)
    
    return round(estimated_size)


def analyze_directory(dir_path: str) -> List[Dict[str, Any]]:
    """
    Analyze directory and get file information.
    
    Args:
        dir_path: Directory path to analyze
    
    Returns:
        List of file info dictionaries
    """
    files = []
    dir_path = Path(dir_path)
    
    if not dir_path.exists():
        raise FileNotFoundError(f"Directory not found: {dir_path}")
    
    for file_path in dir_path.rglob('*'):
        if file_path.is_file():
            try:
                stat = file_path.stat()
                estimated_compressed = estimate_compressed_size(str(file_path), stat.st_size)
                
                files.append({
                    'path': str(file_path),
                    'relative_path': str(file_path.relative_to(dir_path)),
                    'size': stat.st_size,
                    'estimated_compressed': estimated_compressed,
                    'compression_ratio': estimated_compressed / stat.st_size if stat.st_size > 0 else 1.0,
                    'extension': file_path.suffix.lower()
                })
            except Exception as e:
                print(f"Warning: Could not analyze {file_path}: {e}")
    
    return files


def group_files_into_blocks(files: List[Dict[str, Any]], target_size: int) -> List[Dict[str, Any]]:
    """
    Group files into optimal ZIP blocks using bin packing algorithm.
    
    Args:
        files: List of file info dictionaries
        target_size: Target ZIP size in bytes
    
    Returns:
        List of file group dictionaries
    """
    # Sort files by estimated compressed size (largest first for better packing)
    sorted_files = sorted(files, key=lambda f: f['estimated_compressed'], reverse=True)
    
    groups = []
    used = set()
    
    # First pass: handle files that are too large individually
    large_files = [f for f in sorted_files if f['estimated_compressed'] > target_size]
    for file_info in large_files:
        groups.append({
            'files': [file_info],
            'total_size': file_info['size'],
            'total_compressed': file_info['estimated_compressed'],
            'needs_splitting': True
        })
        used.add(file_info['path'])
    
    # Second pass: bin packing for remaining files
    remaining_files = [f for f in sorted_files if f['path'] not in used]
    
    for file_info in remaining_files:
        if file_info['path'] in used:
            continue
        
        # Try to find an existing group that can fit this file
        placed = False
        for group in groups:
            if (not group['needs_splitting'] and 
                group['total_compressed'] + file_info['estimated_compressed'] <= target_size):
                group['files'].append(file_info)
                group['total_size'] += file_info['size']
                group['total_compressed'] += file_info['estimated_compressed']
                used.add(file_info['path'])
                placed = True
                break
        
        # Create new group if couldn't place in existing ones
        if not placed:
            new_group = {
                'files': [file_info],
                'total_size': file_info['size'],
                'total_compressed': file_info['estimated_compressed'],
                'needs_splitting': False
            }
            groups.append(new_group)
            used.add(file_info['path'])
            
            # Try to add more files to this new group
            for other_file in remaining_files:
                if (other_file['path'] not in used and 
                    new_group['total_compressed'] + other_file['estimated_compressed'] <= target_size):
                    new_group['files'].append(other_file)
                    new_group['total_size'] += other_file['size']
                    new_group['total_compressed'] += other_file['estimated_compressed']
                    used.add(other_file['path'])
    
    return groups


def create_organized_structure(groups: List[Dict[str, Any]], output_dir: str) -> None:
    """
    Create organized directory structure.
    
    Args:
        groups: File groups
        output_dir: Output directory path
    """
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)
    
    print(f"📁 Creating {len(groups)} ZIP blocks in {output_dir}")
    
    for i, group in enumerate(groups):
        block_dir = output_path / f"block_{i + 1}"
        block_dir.mkdir(parents=True, exist_ok=True)
        
        print(f"\n📦 Block {i + 1}:")
        print(f"   Files: {len(group['files'])}")
        print(f"   Original size: {format_size(group['total_size'])}")
        print(f"   Estimated ZIP size: {format_size(group['total_compressed'])}")
        print(f"   Needs splitting: {'Yes' if group['needs_splitting'] else 'No'}")
        
        if group['needs_splitting']:
            # Split large files
            print("   ⚠️  Large file will be split:")
            for file_info in group['files']:
                print(f"       {file_info['relative_path']} ({format_size(file_info['size'])})")
                split_large_file(file_info, str(block_dir), 100 * 1024)  # 100KB chunks
        else:
            # Copy files to block directory
            for file_info in group['files']:
                src_path = Path(file_info['path'])
                dest_path = block_dir / src_path.name
                shutil.copy2(src_path, dest_path)
                print(f"       ✓ {file_info['relative_path']} ({format_size(file_info['estimated_compressed'])} compressed)")
            
            # Create ZIP instructions file
            zip_info = {
                'block': i + 1,
                'files': [
                    {
                        'name': Path(f['relative_path']).name,
                        'original_path': f['relative_path'],
                        'size': f['size'],
                        'estimated_compressed': f['estimated_compressed']
                    }
                    for f in group['files']
                ],
                'total_original_size': group['total_size'],
                'estimated_zip_size': group['total_compressed'],
                'command': f"zip -r block_{i + 1}.zip *"
            }
            
            with open(block_dir / 'zip_info.json', 'w') as f:
                json.dump(zip_info, f, indent=2)
    
    # Create summary report
    summary = {
        'total_blocks': len(groups),
        'blocks_needing_split': sum(1 for g in groups if g['needs_splitting']),
        'total_files': sum(len(g['files']) for g in groups),
        'total_original_size': sum(g['total_size'] for g in groups),
        'total_estimated_compressed': sum(g['total_compressed'] for g in groups),
        'average_block_size': round(sum(g['total_compressed'] for g in groups) / len(groups)) if groups else 0,
        'blocks': [
            {
                'block_number': i + 1,
                'file_count': len(g['files']),
                'original_size': g['total_size'],
                'estimated_compressed': g['total_compressed'],
                'needs_splitting': g['needs_splitting'],
                'efficiency': f"{(g['total_compressed'] / (100 * 1024)) * 100:.1f}%"
            }
            for i, g in enumerate(groups)
        ]
    }
    
    with open(output_path / 'organization_summary.json', 'w') as f:
        json.dump(summary, f, indent=2)
    
    print(f"\n📊 Summary:")
    print(f"   Total blocks: {summary['total_blocks']}")
    print(f"   Total files: {summary['total_files']}")
    print(f"   Original size: {format_size(summary['total_original_size'])}")
    print(f"   Estimated compressed: {format_size(summary['total_estimated_compressed'])}")
    print(f"   Compression ratio: {(summary['total_estimated_compressed'] / summary['total_original_size'] * 100):.1f}%")
    print(f"   Average block size: {format_size(summary['average_block_size'])}")


def split_large_file(file_info: Dict[str, Any], output_dir: str, chunk_size: int) -> None:
    """
    Split a large file into chunks.
    
    Args:
        file_info: File information dictionary
        output_dir: Output directory
        chunk_size: Chunk size in bytes
    """
    file_path = Path(file_info['path'])
    file_name = file_path.name
    file_extension = file_path.suffix
    base_name = file_path.stem
    
    chunks = []
    chunk_index = 0
    
    with open(file_path, 'rb') as input_file:
        while True:
            chunk_data = input_file.read(chunk_size)
            if not chunk_data:
                break
            
            chunk_index += 1
            chunk_filename = f"{base_name}.part{chunk_index:03d}{file_extension}"
            chunk_path = Path(output_dir) / chunk_filename
            
            with open(chunk_path, 'wb') as chunk_file:
                chunk_file.write(chunk_data)
            
            chunks.append({
                'name': chunk_filename,
                'index': chunk_index
            })
    
    # Create split info file
    split_info = {
        'original_file': file_info['relative_path'],
        'original_size': file_info['size'],
        'chunk_size': chunk_size,
        'chunks': chunks,
        'total_chunks': chunk_index,
        'join_command': f"cat {base_name}.part* > {file_name}"
    }
    
    split_info_path = Path(output_dir) / f"{base_name}_split_info.json"
    with open(split_info_path, 'w') as f:
        json.dump(split_info, f, indent=2)


def format_size(bytes_size: int) -> str:
    """Format file size for display."""
    units = ['B', 'KB', 'MB', 'GB']
    size = float(bytes_size)
    unit_index = 0
    
    while size >= 1024 and unit_index < len(units) - 1:
        size /= 1024
        unit_index += 1
    
    if unit_index == 0:
        return f"{int(size)}{units[unit_index]}"
    else:
        return f"{size:.1f}{units[unit_index]}"


def parse_size(size_str: str) -> int:
    """Parse size string to bytes."""
    if not size_str:
        return 100 * 1024  # Default 100KB
    
    size_str = size_str.upper().strip()
    
    multipliers = {
        'B': 1,
        'KB': 1024,
        'MB': 1024 ** 2,
        'GB': 1024 ** 3
    }
    
    for unit, multiplier in multipliers.items():
        if size_str.endswith(unit):
            try:
                number = float(size_str[:-len(unit)])
                return round(number * multiplier)
            except ValueError:
                break
    
    raise ValueError(f"Invalid size format: {size_str}")


def organize_command(args) -> None:
    """Handle organize command."""
    try:
        target_size = parse_size(args.target_size)
        print(f"🔍 Analyzing directory: {args.directory}")
        print(f"🎯 Target ZIP size: {format_size(target_size)}")
        
        files = analyze_directory(args.directory)
        if not files:
            print("❌ No files found to organize")
            return
        
        print(f"📁 Found {len(files)} files")
        
        groups = group_files_into_blocks(files, target_size)
        create_organized_structure(groups, args.output)
        
        print(f"\n✅ Organization complete! Check {args.output} directory")
        
    except Exception as error:
        print(f"❌ Error: {error}")
        sys.exit(1)


def split_command(args) -> None:
    """Handle split command."""
    try:
        chunk_size = parse_size(args.chunk_size)
        file_path = Path(args.file)
        
        if not file_path.exists():
            raise FileNotFoundError(f"File not found: {file_path}")
        
        stats = file_path.stat()
        
        print(f"📄 Splitting: {file_path} ({format_size(stats.st_size)})")
        print(f"✂️  Chunk size: {format_size(chunk_size)}")
        
        file_info = {
            'path': str(file_path),
            'relative_path': file_path.name,
            'size': stats.st_size
        }
        
        split_large_file(file_info, args.output, chunk_size)
        print("✅ Split complete!")
        
    except Exception as error:
        print(f"❌ Error: {error}")
        sys.exit(1)


def main():
    """Main function."""
    parser = argparse.ArgumentParser(
        description='Smart File Splitter with ZIP Organization v2.0',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    
    subparsers = parser.add_subparsers(dest='command', help='Available commands')
    
    # Organize command
    organize_parser = subparsers.add_parser(
        'organize',
        help='Organize directory into optimal ZIP blocks'
    )
    organize_parser.add_argument(
        'directory',
        help='Directory to organize'
    )
    organize_parser.add_argument(
        '-s', '--target-size',
        default='100KB',
        help='Target ZIP size (default: 100KB)'
    )
    organize_parser.add_argument(
        '-o', '--output',
        default='organized_blocks',
        help='Output directory (default: organized_blocks)'
    )
    
    # Split command
    split_parser = subparsers.add_parser(
        'split',
        help='Split a single large file'
    )
    split_parser.add_argument(
        'file',
        help='File to split'
    )
    split_parser.add_argument(
        '-s', '--chunk-size',
        default='100KB',
        help='Chunk size (default: 100KB)'
    )
    split_parser.add_argument(
        '-o', '--output',
        default='.',
        help='Output directory (default: current directory)'
    )
    
    args = parser.parse_args()
    
    if args.command == 'organize':
        organize_command(args)
    elif args.command == 'split':
        split_command(args)
    else:
        parser.print_help()


if __name__ == '__main__':
    main()