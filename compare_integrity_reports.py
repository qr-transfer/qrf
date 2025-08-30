#!/usr/bin/env python3
"""
Compare Integrity Reports - Normalize filenames and compare checksums
Finds corrupted files between Windows and macOS versions
"""

import json
import re
from pathlib import Path


def normalize_filename(filename):
    """Normalize filename to part000 pattern for comparison"""
    # Extract part number from cc.7z.part0, cc.7z.part1, etc.
    match = re.search(r'cc\.7z\.part(\d+)', filename)
    if match:
        part_num = int(match.group(1))
        return f"cc.7z.part{part_num:03d}"  # Convert to 000 pattern
    return filename


def load_and_filter_report(report_path):
    """Load JSON report and filter only cc.7z.part files"""
    with open(report_path, 'r') as f:
        data = json.load(f)
    
    filtered_files = {}
    
    for file_path, checksums in data['files'].items():
        filename = Path(file_path).name
        
        # Only include cc.7z.part files
        if filename.startswith('cc.7z.part') and filename != 'join_parts.sh':
            normalized_name = normalize_filename(filename)
            filtered_files[normalized_name] = {
                'original_name': filename,
                'original_path': file_path,
                **checksums
            }
    
    return filtered_files, data.get('directory', 'Unknown')


def compare_reports(windows_report, macos_report):
    """Compare two integrity reports and find differences"""
    
    print("🔍 Loading integrity reports...")
    windows_files, windows_dir = load_and_filter_report(windows_report)
    macos_files, macos_dir = load_and_filter_report(macos_report)
    
    print(f"📁 Windows source: {windows_dir}")
    print(f"📁 macOS source: {macos_dir}")
    print(f"📊 Windows files: {len(windows_files)} cc.7z.part files")
    print(f"📊 macOS files: {len(macos_files)} cc.7z.part files")
    
    # Find common files
    windows_parts = set(windows_files.keys())
    macos_parts = set(macos_files.keys())
    
    common_parts = windows_parts & macos_parts
    windows_only = windows_parts - macos_parts
    macos_only = macos_parts - windows_parts
    
    print(f"\n📈 File Analysis:")
    print(f"   Common files: {len(common_parts)}")
    print(f"   Windows-only files: {len(windows_only)}")
    print(f"   macOS-only files: {len(macos_only)}")
    
    if windows_only:
        print(f"   🪟 Windows-only: {', '.join(sorted(windows_only))}")
    if macos_only:
        print(f"   🍎 macOS-only: {', '.join(sorted(macos_only))}")
    
    # Compare checksums for common files
    identical_files = []
    corrupted_files = []
    size_mismatches = []
    
    for part_name in sorted(common_parts):
        windows_file = windows_files[part_name]
        macos_file = macos_files[part_name]
        
        # Check size first
        if windows_file['size'] != macos_file['size']:
            size_mismatches.append({
                'part': part_name,
                'windows_size': windows_file['size'],
                'macos_size': macos_file['size']
            })
            continue
        
        # Compare all checksums (use old QR algorithm for Windows compatibility)
        checksum_matches = {
            'qr_checksum_old': windows_file.get('qr_checksum_old', windows_file.get('qr_checksum', '')) == macos_file.get('qr_checksum_old', ''),
            'qr_checksum_fnv1a': windows_file.get('qr_checksum_fnv1a', '') == macos_file.get('qr_checksum_fnv1a', ''),
            'md5': windows_file['md5'] == macos_file['md5'],
            'sha256': windows_file['sha256'] == macos_file['sha256'],
            'crc32': windows_file['crc32'] == macos_file['crc32']
        }
        
        if all(checksum_matches.values()):
            identical_files.append(part_name)
        else:
            corrupted_files.append({
                'part': part_name,
                'size': windows_file['size'],
                'windows_checksums': {
                    'qr': windows_file['qr_checksum'],
                    'md5': windows_file['md5'],
                    'sha256': windows_file['sha256'],
                    'crc32': windows_file['crc32']
                },
                'macos_checksums': {
                    'qr': macos_file['qr_checksum'],
                    'md5': macos_file['md5'],
                    'sha256': macos_file['sha256'],
                    'crc32': macos_file['crc32']
                },
                'failed_checks': [k for k, v in checksum_matches.items() if not v]
            })
    
    # Print results
    print(f"\n🎯 Integrity Comparison Results:")
    print(f"   ✅ Identical files: {len(identical_files)}/{len(common_parts)} ({len(identical_files)/len(common_parts)*100:.1f}%)")
    print(f"   ❌ Corrupted files: {len(corrupted_files)}")
    print(f"   📏 Size mismatches: {len(size_mismatches)}")
    
    if size_mismatches:
        print(f"\n📏 Size Mismatches:")
        for mismatch in size_mismatches:
            print(f"   {mismatch['part']}: Windows={mismatch['windows_size']}, macOS={mismatch['macos_size']}")
    
    if corrupted_files:
        print(f"\n❌ Corrupted Files (checksum mismatches):")
        for corrupted in corrupted_files:
            print(f"\n   🚫 {corrupted['part']} ({corrupted['size']} bytes)")
            print(f"      Failed checks: {', '.join(corrupted['failed_checks'])}")
            print(f"      Windows QR: {corrupted['windows_checksums']['qr']}")
            print(f"      macOS QR:   {corrupted['macos_checksums']['qr']}")
            print(f"      Windows MD5: {corrupted['windows_checksums']['md5']}")
            print(f"      macOS MD5:   {corrupted['macos_checksums']['md5']}")
    else:
        print(f"\n🎉 All common files are identical - no corruption detected!")
    
    # Generate summary report
    summary = {
        'comparison_date': '2025-08-30',
        'windows_source': windows_dir,
        'macos_source': macos_dir,
        'total_common_files': len(common_parts),
        'identical_files': len(identical_files),
        'corrupted_files': len(corrupted_files),
        'size_mismatches': len(size_mismatches),
        'corruption_rate': f"{len(corrupted_files)/len(common_parts)*100:.1f}%" if common_parts else "0%",
        'corrupted_parts': [c['part'] for c in corrupted_files],
        'size_mismatch_parts': [s['part'] for s in size_mismatches]
    }
    
    # Save detailed comparison
    with open('file_comparison_report.json', 'w') as f:
        json.dump({
            'summary': summary,
            'corrupted_files': corrupted_files,
            'size_mismatches': size_mismatches,
            'identical_files': identical_files
        }, f, indent=2)
    
    print(f"\n💾 Detailed comparison saved to: file_comparison_report.json")
    
    return summary


if __name__ == '__main__':
    import sys
    
    if len(sys.argv) != 3:
        print("Usage: python compare_integrity_reports.py <windows_report.json> <macos_report.json>")
        sys.exit(1)
    
    windows_report = sys.argv[1]
    macos_report = sys.argv[2]
    
    summary = compare_reports(windows_report, macos_report)
    
    if summary['corrupted_files'] > 0:
        print(f"\n⚠️  CORRUPTION DETECTED: {summary['corrupted_files']} files differ between platforms")
        print(f"🔍 Check file_comparison_report.json for detailed analysis")
    else:
        print(f"\n✅ ALL FILES VERIFIED: No corruption detected in transfer")