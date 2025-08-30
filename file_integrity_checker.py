#!/usr/bin/env python3
"""
File Integrity Checker for QR Transfer Verification
Calculates multiple checksums to verify file integrity across platforms

Usage:
    python file_integrity_checker.py scan <directory> [--output report.txt]
    python file_integrity_checker.py verify <file1> <file2> [--detailed]
    python file_integrity_checker.py hash <file> [--all-algorithms]
"""

import os
import sys
import hashlib
import argparse
import json
from pathlib import Path
from datetime import datetime
import zlib


def calculate_qr_checksum(file_data):
    """Calculate the same checksum used by QR encoder/decoder"""
    hash_val = 0
    for byte in file_data:
        hash_val = ((hash_val << 5) - hash_val) + byte
        hash_val = hash_val & 0xFFFFFFFF  # Keep as 32-bit
    return format(abs(hash_val), 'x')[:8]  # 8 chars hex


def calculate_checksums(file_path):
    """Calculate multiple checksums for a file"""
    try:
        with open(file_path, 'rb') as f:
            data = f.read()
        
        checksums = {
            'qr_checksum': calculate_qr_checksum(data),
            'md5': hashlib.md5(data).hexdigest(),
            'sha1': hashlib.sha1(data).hexdigest(),
            'sha256': hashlib.sha256(data).hexdigest(),
            'crc32': format(zlib.crc32(data) & 0xffffffff, '08x'),
            'size': len(data),
            'file_path': str(file_path)
        }
        
        return checksums
    except Exception as e:
        return {'error': str(e), 'file_path': str(file_path)}


def scan_directory(directory, output_file=None):
    """Scan directory and generate integrity report"""
    directory = Path(directory)
    if not directory.exists():
        print(f"❌ Directory not found: {directory}")
        return
    
    print(f"🔍 Scanning directory: {directory}")
    
    results = {
        'scan_date': datetime.now().isoformat(),
        'directory': str(directory),
        'files': {},
        'summary': {
            'total_files': 0,
            'total_size': 0,
            'errors': 0
        }
    }
    
    # Scan all files
    for file_path in directory.rglob('*'):
        if file_path.is_file():
            print(f"📄 Processing: {file_path.name}")
            
            checksums = calculate_checksums(file_path)
            relative_path = str(file_path.relative_to(directory))
            results['files'][relative_path] = checksums
            
            if 'error' not in checksums:
                results['summary']['total_files'] += 1
                results['summary']['total_size'] += checksums['size']
            else:
                results['summary']['errors'] += 1
                print(f"❌ Error processing {file_path.name}: {checksums['error']}")
    
    # Generate report
    report_lines = [
        f"# File Integrity Report",
        f"Generated: {results['scan_date']}",
        f"Directory: {results['directory']}",
        f"Total Files: {results['summary']['total_files']}",
        f"Total Size: {format_size(results['summary']['total_size'])}",
        f"Errors: {results['summary']['errors']}",
        "",
        "# File Checksums (QR | MD5 | SHA256 | Size)",
        "# Format: filename | qr_hash | md5 | sha256 | size_bytes"
    ]
    
    for file_path, checksums in results['files'].items():
        if 'error' not in checksums:
            line = f"{file_path} | {checksums['qr_checksum']} | {checksums['md5']} | {checksums['sha256']} | {checksums['size']}"
            report_lines.append(line)
        else:
            report_lines.append(f"{file_path} | ERROR: {checksums['error']}")
    
    # Save to file
    output_path = output_file or f"integrity_report_{datetime.now().strftime('%Y%m%d_%H%M%S')}.txt"
    with open(output_path, 'w') as f:
        f.write('\n'.join(report_lines))
    
    # Save JSON version for programmatic use
    json_path = output_path.replace('.txt', '.json')
    with open(json_path, 'w') as f:
        json.dump(results, f, indent=2)
    
    print(f"\n📊 Summary:")
    print(f"   Files processed: {results['summary']['total_files']}")
    print(f"   Total size: {format_size(results['summary']['total_size'])}")
    print(f"   Errors: {results['summary']['errors']}")
    print(f"   Report saved: {output_path}")
    print(f"   JSON data: {json_path}")


def verify_files(file1, file2, detailed=False):
    """Compare two files for integrity"""
    try:
        checksums1 = calculate_checksums(file1)
        checksums2 = calculate_checksums(file2)
        
        if 'error' in checksums1 or 'error' in checksums2:
            print(f"❌ Error reading files")
            if 'error' in checksums1:
                print(f"   File 1: {checksums1['error']}")
            if 'error' in checksums2:
                print(f"   File 2: {checksums2['error']}")
            return
        
        print(f"📋 Comparing files:")
        print(f"   File 1: {file1} ({format_size(checksums1['size'])})")
        print(f"   File 2: {file2} ({format_size(checksums2['size'])})")
        
        # Compare all checksums
        matches = {
            'qr_checksum': checksums1['qr_checksum'] == checksums2['qr_checksum'],
            'md5': checksums1['md5'] == checksums2['md5'],
            'sha256': checksums1['sha256'] == checksums2['sha256'],
            'size': checksums1['size'] == checksums2['size']
        }
        
        all_match = all(matches.values())
        
        if all_match:
            print("✅ Files are identical - no corruption detected")
        else:
            print("❌ Files differ - corruption detected!")
            
        print("\n📊 Verification Results:")
        for check, passed in matches.items():
            status = "✅ PASS" if passed else "❌ FAIL"
            print(f"   {check.upper()}: {status}")
            
        if detailed:
            print(f"\n🔍 Detailed Checksums:")
            print(f"File 1:")
            for key, value in checksums1.items():
                if key != 'file_path':
                    print(f"   {key}: {value}")
            print(f"File 2:")
            for key, value in checksums2.items():
                if key != 'file_path':
                    print(f"   {key}: {value}")
                    
    except Exception as e:
        print(f"❌ Error: {e}")


def hash_file(file_path, all_algorithms=False):
    """Calculate hash for a single file"""
    try:
        checksums = calculate_checksums(file_path)
        
        if 'error' in checksums:
            print(f"❌ Error: {checksums['error']}")
            return
            
        print(f"📄 File: {file_path}")
        print(f"💾 Size: {format_size(checksums['size'])}")
        print(f"🔢 QR Checksum: {checksums['qr_checksum']}")
        
        if all_algorithms:
            print(f"🔐 MD5: {checksums['md5']}")
            print(f"🔐 SHA1: {checksums['sha1']}")
            print(f"🔐 SHA256: {checksums['sha256']}")
            print(f"🔐 CRC32: {checksums['crc32']}")
            
    except Exception as e:
        print(f"❌ Error: {e}")


def format_size(bytes_size):
    """Format file size for display"""
    for unit in ['B', 'KB', 'MB', 'GB']:
        if bytes_size < 1024:
            return f"{bytes_size:.1f}{unit}" if unit != 'B' else f"{int(bytes_size)}B"
        bytes_size /= 1024
    return f"{bytes_size:.1f}TB"


def main():
    parser = argparse.ArgumentParser(
        description='File Integrity Checker for QR Transfer Verification',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    
    subparsers = parser.add_subparsers(dest='command', help='Available commands')
    
    # Scan command
    scan_parser = subparsers.add_parser('scan', help='Scan directory and generate integrity report')
    scan_parser.add_argument('directory', help='Directory to scan')
    scan_parser.add_argument('-o', '--output', help='Output report file (default: auto-generated)')
    
    # Verify command
    verify_parser = subparsers.add_parser('verify', help='Compare two files for integrity')
    verify_parser.add_argument('file1', help='First file to compare')
    verify_parser.add_argument('file2', help='Second file to compare')
    verify_parser.add_argument('-d', '--detailed', action='store_true', help='Show detailed checksums')
    
    # Hash command
    hash_parser = subparsers.add_parser('hash', help='Calculate hash for a single file')
    hash_parser.add_argument('file', help='File to hash')
    hash_parser.add_argument('-a', '--all-algorithms', action='store_true', help='Show all hash algorithms')
    
    args = parser.parse_args()
    
    if args.command == 'scan':
        scan_directory(args.directory, args.output)
    elif args.command == 'verify':
        verify_files(args.file1, args.file2, args.detailed)
    elif args.command == 'hash':
        hash_file(args.file, args.all_algorithms)
    else:
        parser.print_help()


if __name__ == '__main__':
    main()