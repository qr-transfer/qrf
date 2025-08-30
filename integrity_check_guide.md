# File Integrity Verification Guide

## 🛡️ Problem Solved
The decoder now **rejects corrupted files** and saves them as `.failed` for analysis while continuing to process for clean copies.

## 🔧 Cross-Platform Verification

### **Windows Command Line:**
```bash
# Scan entire directory (generates report.txt)
python file_integrity_checker.py scan C:\transferred_files

# Compare specific files
python file_integrity_checker.py verify file1.zip file2.zip --detailed

# Get hash for single file
python file_integrity_checker.py hash document.pdf --all-algorithms
```

### **macOS/Linux Command Line:**
```bash
# Scan directory with custom output
python file_integrity_checker.py scan ~/Downloads/transferred_files -o integrity_report.txt

# Verify two files are identical
python file_integrity_checker.py verify original.7z transferred.7z

# Check single file hash
python file_integrity_checker.py hash video.mp4 -a
```

## 📊 Report Output Example
```
# File Integrity Report
Generated: 2025-08-30T15:30:45
Directory: /Users/files
Total Files: 15
Total Size: 25.3MB
Errors: 0

# File Checksums (QR | MD5 | SHA256 | Size)
document.pdf | a1b2c3d4 | 5f4e3d2c1b0a9876 | sha256_hash_here | 1024567
video.mp4 | e5f6g7h8 | 9e8d7c6b5a4f3210 | sha256_hash_here | 15728640
```

## 🔐 Better Checksum Alternatives

### **Current QR Checksum**: Simple 32-bit hash (fast but basic)
```javascript
// Current algorithm (8 chars)
let hash = 0;
for (byte of data) hash = ((hash << 5) - hash) + byte;
```

### **Better Alternatives:**

1. **CRC32** (Fast, good error detection):
   ```python
   import zlib
   crc = zlib.crc32(data) & 0xffffffff
   ```

2. **MD5** (Fast, widely supported):
   ```python
   import hashlib
   md5 = hashlib.md5(data).hexdigest()
   ```

3. **SHA256** (Cryptographically secure):
   ```python
   import hashlib
   sha256 = hashlib.sha256(data).hexdigest()
   ```

4. **xxHash** (Fastest, best performance):
   ```python
   import xxhash
   xx = xxhash.xxh64(data).hexdigest()
   ```

## 📋 Integrity Verification Workflow

### **1. Before Transfer (Sender):**
```bash
# Generate integrity report for original files
python file_integrity_checker.py scan ./original_files -o original_hashes.txt
```

### **2. After Transfer (Receiver):**
```bash
# Generate report for received files
python file_integrity_checker.py scan ./received_files -o received_hashes.txt

# Compare reports manually or with diff tool
diff original_hashes.txt received_hashes.txt
```

### **3. File-by-File Verification:**
```bash
# Compare specific files
python file_integrity_checker.py verify original.zip received.zip

# Check individual file
python file_integrity_checker.py hash suspicious_file.pdf --all-algorithms
```

## 🚫 Corrupted File Handling

### **New Decoder Behavior:**
- ✅ **Checksum passes** → File downloaded normally
- ❌ **Checksum fails** → File saved as `filename.ext.failed` 
- 🔄 **Processing continues** → Looks for clean copy in video
- 📊 **Shows corruption details** → Expected vs actual checksums

### **Analysis Files:**
- `document.pdf` ← Clean verified file
- `document.pdf.failed` ← Corrupted version for debugging
- `integrity_report.json` ← Detailed analysis data

This system ensures data integrity while providing tools for cross-platform verification and corruption analysis!