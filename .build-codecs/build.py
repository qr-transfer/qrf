#!/usr/bin/env python3
"""Reproducible build: add airgap-safe zstd/gzip compression to the QR encoder
and zstd/gzip decompression to all three decoders.

Codecs are EMBEDDED inline as base64 (no network ever needed):
  - encoder: zstd-wasm v0.0.27 glue (+esm, MIT, bokuweb) + zstd.wasm
  - decoders: fzstd v0.1.1 (+esm, MIT, 101arrowz) pure-JS decompressor
gzip uses the browser-native Compression/DecompressionStream APIs.

Run from repo root:  python3 .build-codecs/build.py
Idempotent against the committed (pristine) HTML files.
"""
import base64, os, re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(ROOT)
BC = '.build-codecs'

def b64(path):
    return base64.b64encode(open(path, 'rb').read()).decode('ascii')

def chunked(s, n=120):
    return '\n'.join(s[i:i+n] for i in range(0, len(s), n))

ZSTD_GLUE = chunked(b64(f'{BC}/zstd-glue.mjs'))
ZSTD_WASM = chunked(b64(f'{BC}/zstd.wasm'))
FZSTD     = chunked(b64(f'{BC}/fzstd.mjs'))

def replace_once(src, old, new, label):
    # Whitespace-tolerant: ignore trailing whitespace at the end of each anchor
    # line (the source has trailing spaces on "blank" lines inside functions).
    lines = old.split('\n')
    pattern = r'[ \t]*\n'.join(re.escape(ln.rstrip()) for ln in lines)
    matches = list(re.finditer(pattern, src))
    if len(matches) != 1:
        raise SystemExit(f'FAIL [{label}]: anchor matched {len(matches)} times (expected 1): {old[:80]!r}')
    m = matches[0]
    return src[:m.start()] + new + src[m.end():]

# =====================================================================
# ENCODER
# =====================================================================
enc = open('vde-qr-encoder.html').read()

enc = replace_once(enc,
    '<title>QR File Transfer - Encoder v1.1.0 (v4 FileId Fix)</title>',
    '<title>QR File Transfer - Encoder v1.2.0 (embedded zstd/gzip compression)</title>',
    'enc-title')

ENC_ASSETS = f'''<body>
<!-- ===== Embedded compression codec (airgap-safe, no network needed) =====
     zstd-wasm v0.0.27 (MIT, github.com/bokuweb/zstd-wasm), decoded at runtime
     from base64 -> ES module via blob URL; wasm bytes are passed to init(). -->
<script type="application/octet-stream" id="zstd-glue-b64">
{ZSTD_GLUE}
</script>
<script type="application/octet-stream" id="zstd-wasm-b64">
{ZSTD_WASM}
</script>
'''
enc = replace_once(enc, '<body>', ENC_ASSETS, 'enc-body')

ENC_MODULE = r'''        let MAX_QR_CONTENT_SIZE = DEFAULT_QR_CONTENT_SIZE;

        // ===== Compression layer (encoder protocol v5) =====
        // The file is compressed BEFORE chunking/fountain-coding, and the chosen
        // algorithm is announced to the decoder via the M: metadata packet
        // (appended fields: <compression>:<originalSize>:<originalChecksum>), so
        // the decoder always knows how to restore the original file.
        //
        // AIRGAP-SAFE: the zstd codec (wasm + glue) is EMBEDDED in this HTML as
        // base64 (the <script type="application/octet-stream"> blocks), so no
        // network access is ever required. gzip uses the browser-native
        // CompressionStream API (also zero-dependency).
        //
        // Algorithm choice per file type:
        //   already-compressed (jpg/mp4/zip/...) -> none (recompression gains ~0)
        //   web assets (html/js/css/json/svg)    -> zstd -19 (gzip fallback)
        //   text/logs/source code                -> zstd -19 (gzip fallback)
        //   other binaries                       -> zstd -12 (gzip fallback)
        // If zstd cannot initialise, the chain falls back to native gzip, then
        // to storing the file uncompressed.
        const COMPRESSION_SKIP_EXTENSIONS = new Set([
            // images / video / audio (already entropy-coded)
            'jpg','jpeg','png','gif','webp','avif','heic','heif','jxl',
            'mp4','m4v','m4a','mov','mkv','webm','avi','mp3','aac','ogg','opus','flac',
            // archives / compressed containers
            'zip','gz','tgz','bz2','xz','zst','br','7z','rar','lz4',
            'jar','war','apk','ipa','docx','xlsx','pptx','odt','ods','odp','epub',
            // misc compressed
            'woff','woff2','pdf'
        ]);
        const WEB_ASSET_EXTENSIONS = new Set([
            'html','htm','js','mjs','cjs','css','json','svg','xml','map'
        ]);
        const TEXT_EXTENSIONS = new Set([
            'txt','log','md','markdown','csv','tsv','yaml','yml','ini','cfg','conf','toml',
            'sql','sh','bash','zsh','bat','ps1','py','java','c','cpp','cc','h','hpp','rs',
            'go','ts','tsx','jsx','vue','rb','php','pl','r','swift','kt','scala','lua',
            'dart','tex','bib','properties','env','gradle','makefile','dockerfile'
        ]);

        // Decode an embedded base64 <script> payload to a Uint8Array (airgap-safe)
        function decodeEmbeddedBase64(elementId) {
            const el = document.getElementById(elementId);
            if (!el) throw new Error(`Embedded asset '${elementId}' not found`);
            const b64 = el.textContent.replace(/\s+/g, '');
            const bin = atob(b64);
            const bytes = new Uint8Array(bin.length);
            for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
            return bytes;
        }

        // Import an ES module from inlined source text via a blob URL (airgap-safe)
        function importModuleFromText(src) {
            const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
            return import(url);
        }

        let _zstdModulePromise = null;
        function loadZstdEncoder() {
            if (!_zstdModulePromise) {
                // Codec is embedded in the page: decode glue JS + wasm from the
                // base64 <script> blocks and hand the wasm BYTES to init(). The
                // emscripten build instantiates directly from bytes (no fetch),
                // so this works with zero network access.
                _zstdModulePromise = (async () => {
                    const glueSrc = new TextDecoder().decode(decodeEmbeddedBase64('zstd-glue-b64'));
                    const wasmBytes = decodeEmbeddedBase64('zstd-wasm-b64');
                    const m = await importModuleFromText(glueSrc);
                    await m.init(wasmBytes);
                    return m;
                })();
            }
            return _zstdModulePromise;
        }

        async function compressWith(algo, data, opts) {
            if (algo === 'zstd') {
                const zstd = await loadZstdEncoder();
                return new Uint8Array(zstd.compress(data, opts.level || 12));
            }
            if (algo === 'gzip') {
                if (typeof CompressionStream === 'undefined') {
                    throw new Error('CompressionStream API not supported');
                }
                const stream = new Blob([data]).stream().pipeThrough(new CompressionStream('gzip'));
                return new Uint8Array(await new Response(stream).arrayBuffer());
            }
            throw new Error(`Unknown compression algorithm: ${algo}`);
        }

        function classifyFileForCompression(fileName, fileType) {
            const ext = (fileName && fileName.includes('.') ? fileName.split('.').pop() : '').toLowerCase();
            const mime = (fileType || '').toLowerCase();
            const mimeException = /svg|xml|bmp|tiff|x-icon|x-portable/.test(mime);
            if (COMPRESSION_SKIP_EXTENSIONS.has(ext) ||
                (!mimeException && (/^(image|video|audio)\//.test(mime) || /zip|compressed|gzip|x-7z|x-rar/.test(mime)))) {
                return 'precompressed';
            }
            if (WEB_ASSET_EXTENSIONS.has(ext) || /json|javascript|html|css|xml|svg/.test(mime)) return 'web';
            if (TEXT_EXTENSIONS.has(ext) || mime.startsWith('text/')) return 'text';
            return 'binary';
        }

        async function compressForTransfer(fileName, fileType, rawData) {
            const kind = classifyFileForCompression(fileName, fileType);
            if (kind === 'precompressed') {
                console.log(`🗜️ ${fileName}: already-compressed format — storing without recompression`);
                return { algo: 'none', data: rawData };
            }
            // Cap heavy levels on big files to keep in-browser encoding responsive
            const big = rawData.length > 8 * 1024 * 1024;
            const plan = (kind === 'web' || kind === 'text')
                ? [['zstd', { level: big ? 12 : 19 }], ['gzip', {}]]
                : [['zstd', { level: 12 }], ['gzip', {}]];
            for (const [algo, opts] of plan) {
                try {
                    const t0 = performance.now();
                    const compressed = await compressWith(algo, rawData, opts);
                    const ms = Math.round(performance.now() - t0);
                    if (compressed.length < rawData.length * 0.98) {
                        const saved = (100 * (1 - compressed.length / rawData.length)).toFixed(1);
                        console.log(`🗜️ ${fileName}: ${algo} ${rawData.length} → ${compressed.length} bytes (${saved}% saved, ${ms}ms) — fewer QR frames to transfer`);
                        return { algo, data: compressed };
                    }
                    console.log(`🗜️ ${fileName}: ${algo} gave no meaningful gain — storing uncompressed`);
                    return { algo: 'none', data: rawData };
                } catch (e) {
                    console.warn(`🗜️ ${algo} unavailable (${e.message}) — trying next algorithm`);
                }
            }
            return { algo: 'none', data: rawData };
        }

        // Read a file and compress it for transfer. Returns everything the
        // encoder needs to chunk the payload and announce compression metadata.
        async function prepareFileForEncoding(file) {
            const rawData = new Uint8Array(await file.arrayBuffer());
            const originalChecksum = calculateFileChecksumFNV1a(rawData);
            const { algo, data } = await compressForTransfer(file.name, file.type, rawData);
            return {
                data,
                compression: algo,
                originalSize: rawData.length,
                originalChecksum,
                transmittedSize: data.length
            };
        }'''
enc = replace_once(enc,
    '        let MAX_QR_CONTENT_SIZE = DEFAULT_QR_CONTENT_SIZE;',
    ENC_MODULE, 'enc-module')

# --- generateQRCodesForFileOnly: compress before chunking ---
enc = replace_once(enc, '''        // Generate QR codes for a specific file (returns packets array)
        async function generateQRCodesForFileOnly(file) {
            return new Promise((resolve, reject) => {
                try {
                    console.log(`🔧 Generating packets for ${file.name} (${file.size} bytes)`);

                    const fileReader = new FileReader();
                    fileReader.onload = async () => {
                        try {
                            const arrayBuffer = fileReader.result;
                            const fileData = new Uint8Array(arrayBuffer);
                            const chunkSize = parseInt(chunkSlider.value);
                            const chunks = [];

                            // Split file into chunks
                            for (let offset = 0; offset < fileData.length; offset += chunkSize) {
                                const chunk = fileData.slice(offset, offset + chunkSize);
                                chunks.push(chunk);
                            }

                            // Create LT encoder
                            const ltEncoder = new SystematicLTEncoder(chunks);
                            ltEncoder.systematicChunksPerQR = parseInt(systematicChunksSlider.value);
                            ltEncoder.fountainMaxDegree = parseInt(fountainDegreeSlider.value);
                            ltEncoder.currentFileName = file.name;
                            ltEncoder.currentFileSize = file.size;
                            ltEncoder.currentFileType = file.type;

                            // Generate packets
                            const filePackets = [ltEncoder.generateMetadataPacket()];
                            const totalPacketsToGenerate = ltEncoder.calculateTotalPackets();

                            for (let i = 1; i < totalPacketsToGenerate; i++) {
                                filePackets.push(ltEncoder.generatePacket());
                            }

                            console.log(`✅ Generated ${filePackets.length} packets for ${file.name}`);
                            resolve(filePackets);
                        } catch (error) {
                            reject(error);
                        }
                    };

                    fileReader.onerror = reject;
                    fileReader.readAsArrayBuffer(file);
                } catch (error) {
                    reject(error);
                }
            });
        }''', '''        // Generate QR codes for a specific file (returns packets array)
        async function generateQRCodesForFileOnly(file) {
            console.log(`🔧 Generating packets for ${file.name} (${file.size} bytes)`);

            // Compress before chunking (algorithm chosen per file type)
            const prep = await prepareFileForEncoding(file);
            const fileData = prep.data;
            const chunkSize = parseInt(chunkSlider.value);
            const chunks = [];

            // Split (possibly compressed) payload into chunks
            for (let offset = 0; offset < fileData.length; offset += chunkSize) {
                const chunk = fileData.slice(offset, offset + chunkSize);
                chunks.push(chunk);
            }

            // Create LT encoder
            const ltEncoder = new SystematicLTEncoder(chunks);
            ltEncoder.systematicChunksPerQR = parseInt(systematicChunksSlider.value);
            ltEncoder.fountainMaxDegree = parseInt(fountainDegreeSlider.value);
            ltEncoder.currentFileName = file.name;
            ltEncoder.currentFileSize = prep.transmittedSize; // transmitted payload size (post-compression)
            ltEncoder.currentFileType = file.type;
            ltEncoder.compressionAlgo = prep.compression;
            ltEncoder.originalFileSize = prep.originalSize;
            ltEncoder.originalFileChecksum = prep.originalChecksum;

            // Generate packets
            const filePackets = [ltEncoder.generateMetadataPacket()];
            const totalPacketsToGenerate = ltEncoder.calculateTotalPackets();

            for (let i = 1; i < totalPacketsToGenerate; i++) {
                filePackets.push(ltEncoder.generatePacket());
            }

            console.log(`✅ Generated ${filePackets.length} packets for ${file.name}`);
            return filePackets;
        }''', 'enc-fileonly')

# --- generateQRCodesForFile (legacy): compress after read ---
enc = replace_once(enc, '''                            // Use existing generateQRCodes logic - same as the main function
                            const arrayBuffer = fileReader.result;
                            const fileData = new Uint8Array(arrayBuffer);

                            console.log(`📦 File data loaded: ${fileData.length} bytes`);''',
    '''                            // Use existing generateQRCodes logic - same as the main function
                            const arrayBuffer = fileReader.result;
                            const rawData = new Uint8Array(arrayBuffer);

                            console.log(`📦 File data loaded: ${rawData.length} bytes`);

                            // Compress before chunking (algorithm chosen per file type)
                            const originalChecksum = calculateFileChecksumFNV1a(rawData);
                            const comp = await compressForTransfer(file.name, file.type, rawData);
                            const fileData = comp.data;''', 'enc-legacy-read')

enc = replace_once(enc, '''                            // Set current file info for metadata generation
                            ltEncoder.currentFileName = file.name;
                            ltEncoder.currentFileSize = file.size;
                            ltEncoder.currentFileType = file.type;''',
    '''                            // Set current file info for metadata generation
                            ltEncoder.currentFileName = file.name;
                            ltEncoder.currentFileSize = fileData.length; // transmitted payload size (post-compression)
                            ltEncoder.currentFileType = file.type;
                            ltEncoder.compressionAlgo = comp.algo;
                            ltEncoder.originalFileSize = rawData.length;
                            ltEncoder.originalFileChecksum = originalChecksum;''', 'enc-legacy-fields')

# --- generateQRCodes (main): compress before chunking ---
enc = replace_once(enc, '''            // Read the file as binary data
            const fileData = await readFileAsBinary(file);
            fileContent = fileData;

            // Split file into binary chunks
            chunks = chunkFileBinary(fileContent, chunkSize);''',
    '''            // Read the file as binary data and compress before chunking
            const prep = await prepareFileForEncoding(file);
            const fileData = prep.data;
            fileContent = fileData;

            // Split (possibly compressed) payload into binary chunks
            chunks = chunkFileBinary(fileContent, chunkSize);''', 'enc-main-read')

enc = replace_once(enc, '''            // Set current file info for metadata generation
            ltEncoder.currentFileName = fileNameText;
            ltEncoder.currentFileSize = fileSizeBytes;
            ltEncoder.currentFileType = fileInput.files[0] ? fileInput.files[0].type : '';''',
    '''            // Set current file info for metadata generation
            ltEncoder.currentFileName = fileNameText;
            ltEncoder.currentFileSize = prep.transmittedSize; // transmitted payload size (post-compression)
            ltEncoder.currentFileType = fileInput.files[0] ? fileInput.files[0].type : '';
            ltEncoder.compressionAlgo = prep.compression;
            ltEncoder.originalFileSize = prep.originalSize;
            ltEncoder.originalFileChecksum = prep.originalChecksum;''', 'enc-main-fields')

# --- metadata packet: bump version, append compression fields ---
enc = replace_once(enc, '''                // Create compact metadata string with incremental encoder version
                // Format: M:<version>:<filename>:<filetype>:<filesize>:<chunks>:<packets>:<systematicchunks>:<density>:<fps>:<chunksize>:<redund>:<ecl>:<metachecksum>:<filechecksum>:<encoderversion>:<ltparams>:<fountainmaxdegree>
                const protocolVersion = '3.0'; // Keep existing protocol version
                const encoderVersion = '4.0'; // New incremental encoder version with fileId support
                const metadataString = `M:${protocolVersion}:${encodeURIComponent(fileNameText)}:${encodeURIComponent(fileType)}:${fileSizeBytes}:${this.numChunks}:${this.calculateTotalPackets()}:${systematicChunks}:${highDensityMode}:${fps}:${chunkSize}:${redundancy}:${ecLevel}:${metaChecksum}:${fileChecksum}:${encoderVersion}:${ltParams}:${fountainMaxDegree}`;''',
    '''                // Compression negotiation: announce the algorithm used on the
                // transmitted payload plus the original (pre-compression) size
                // and checksum so the decoder can restore and verify the file.
                const compression = this.compressionAlgo || 'none';
                const originalSize = this.originalFileSize || fileSizeBytes;
                const originalChecksum = this.originalFileChecksum || fileChecksum;

                // Create compact metadata string with incremental encoder version
                // Format: M:<version>:<filename>:<filetype>:<filesize>:<chunks>:<packets>:<systematicchunks>:<density>:<fps>:<chunksize>:<redund>:<ecl>:<metachecksum>:<filechecksum>:<encoderversion>:<ltparams>:<fountainmaxdegree>:<compression>:<originalsize>:<originalchecksum>
                // NOTE: <filesize>/<filechecksum> describe the TRANSMITTED payload
                // (post-compression); <originalsize>/<originalchecksum> describe the
                // original file. Decoders < v5 ignore the appended fields.
                const protocolVersion = '3.0'; // Keep existing protocol version
                const encoderVersion = '5.0'; // v5: per-file-type compression (zstd/gzip) with negotiation
                const metadataString = `M:${protocolVersion}:${encodeURIComponent(fileNameText)}:${encodeURIComponent(fileType)}:${fileSizeBytes}:${this.numChunks}:${this.calculateTotalPackets()}:${systematicChunks}:${highDensityMode}:${fps}:${chunkSize}:${redundancy}:${ecLevel}:${metaChecksum}:${fileChecksum}:${encoderVersion}:${ltParams}:${fountainMaxDegree}:${compression}:${originalSize}:${originalChecksum}`;''',
    'enc-metadata')

open('vde-qr-encoder.html', 'w').write(enc)
print('OK encoder')

# =====================================================================
# DECODERS (x3)
# =====================================================================
DEC_ASSETS = f'''<body>
<!-- ===== Embedded zstd decompressor (airgap-safe, no network needed) =====
     fzstd v0.1.1 (MIT, github.com/101arrowz/fzstd), decoded at runtime from
     base64 -> ES module via blob URL. gzip uses native DecompressionStream. -->
<script type="application/octet-stream" id="fzstd-b64">
{FZSTD}
</script>
'''

DEC_BLOCK = r'''    // ===== Decompression layer (decoder protocol v5) =====
    // The encoder announces the compression algorithm used on the transmitted
    // payload via the M: metadata packet (compression/originalSize/originalChecksum
    // fields). After fountain reconstruction + payload checksum verification we
    // decompress here and verify the ORIGINAL file checksum before delivery.
    //
    // AIRGAP-SAFE: the zstd decompressor (fzstd, pure JS) is EMBEDDED in this HTML
    // as base64 (the <script type="application/octet-stream"> block), so no
    // network access is ever required. gzip uses the browser-native
    // DecompressionStream API.
    //   zstd                          -> fzstd (pure-JS decompressor, embedded)
    //   gzip / deflate / deflate-raw  -> native DecompressionStream API
    //   none                          -> passthrough (legacy v3/v4 encoders)
    function decodeEmbeddedBase64(elementId) {
      const el = document.getElementById(elementId);
      if (!el) throw new Error(`Embedded asset '${elementId}' not found`);
      const b64 = el.textContent.replace(/\s+/g, '');
      const bin = atob(b64);
      const bytes = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
      return bytes;
    }

    let _fzstdModulePromise = null;
    function loadFzstd() {
      if (!_fzstdModulePromise) {
        // fzstd is embedded as base64 — decode the glue and import it via a
        // blob URL (no network access required).
        _fzstdModulePromise = (async () => {
          const src = new TextDecoder().decode(decodeEmbeddedBase64('fzstd-b64'));
          const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
          return import(url);
        })();
      }
      return _fzstdModulePromise;
    }

    async function decompressTransfer(algo, data) {
      switch (algo) {
        case undefined:
        case '':
        case 'none':
          return data;
        case 'zstd': {
          const fzstd = await loadFzstd();
          return fzstd.decompress(data);
        }
        case 'gzip':
        case 'deflate':
        case 'deflate-raw': {
          if (typeof DecompressionStream === 'undefined') {
            throw new Error('DecompressionStream API not supported');
          }
          const stream = new Blob([data]).stream().pipeThrough(new DecompressionStream(algo));
          return new Uint8Array(await new Response(stream).arrayBuffer());
        }
        default:
          throw new Error(`Unknown compression algorithm announced by encoder: ${algo}`);
      }
    }

    class EnhancedFountainDecoder {'''

DELIVER = r'''      // Decompress the verified transmitted payload according to the algorithm
      // negotiated via metadata (v5 encoders), verify the ORIGINAL file
      // checksum/size, then hand the restored file to the completion callback.
      async deliverFile(fileData) {
        const algo = (this.metaData && this.metaData.compression) || 'none';
        let finalData = fileData;

        if (algo !== 'none') {
          try {
            console.log(`🗜️ Decompressing payload with ${algo} (${fileData.length} bytes)...`);
            const t0 = performance.now();
            finalData = await decompressTransfer(algo, fileData);
            console.log(`🗜️ Decompressed: ${fileData.length} → ${finalData.length} bytes (${Math.round(performance.now() - t0)}ms)`);

            if (this.metaData.originalSize > 0 && finalData.length !== this.metaData.originalSize) {
              console.warn(`⚠️ Decompressed size mismatch: expected ${this.metaData.originalSize}, got ${finalData.length}`);
            }
            if (this.metaData.originalChecksum) {
              const checksum = this.calculateFileChecksum(finalData);
              if (checksum === this.metaData.originalChecksum) {
                console.log(`✅ Original file integrity verified after decompression: ${checksum}`);
              } else {
                console.error(`❌ Original checksum mismatch after decompression: expected ${this.metaData.originalChecksum}, got ${checksum}`);
              }
            }
          } catch (e) {
            // The transmitted payload itself was checksum-valid, so don't lose
            // it: deliver the still-compressed payload with the algorithm's
            // native extension so it can be decompressed externally.
            console.error(`❌ Decompression failed (${e.message}) — delivering raw ${algo} payload`);
            const ext = algo === 'gzip' ? 'gz' : (algo === 'brotli' ? 'br' : 'zst');
            const rawMetadata = {...this.metaData};
            rawMetadata.fileName = `${this.metaData.fileName}.${ext}`;
            if (this.completeCallback) {
              this.completeCallback(fileData, rawMetadata);
            }
            return;
          }
        }

        if (this.completeCallback) {
          this.completeCallback(finalData);
        }
      }

      arrayBufferToString(buffer) {'''

TITLE_EDITS = {
    'vdf-qr-decoder.html': [
        ('<title>QR Code File Decoder v2.1.2 (Performance + v4 FileId Fix)</title>',
         '<title>QR Code File Decoder v2.2.0 (embedded zstd/gzip decompression)</title>'),
        ('QR File Decoder <span style="font-size: 0.6em; color: #666;">v2.1.2</span>',
         'QR File Decoder <span style="font-size: 0.6em; color: #666;">v2.2.0</span>'),
    ],
    'vdf-qr-decoder-firefox.html': [
        ('<title>QR Code File Decoder v2.1.2-Firefox (Browser Optimized)</title>',
         '<title>QR Code File Decoder v2.2.0-Firefox (embedded zstd/gzip decompression)</title>'),
        ('v2.1.2-Firefox</span>', 'v2.2.0-Firefox</span>'),
    ],
    'vdf-qr-decoder-safari.html': [
        ('<title>QR Code File Decoder v2.1.2-Safari (Browser Optimized)</title>',
         '<title>QR Code File Decoder v2.2.0-Safari (embedded zstd/gzip decompression)</title>'),
        ('v2.1.2-Safari</span>', 'v2.2.0-Safari</span>'),
    ],
}

for path, titles in TITLE_EDITS.items():
    src = open(path).read()
    for old, new in titles:
        src = replace_once(src, old, new, f'{path}-title')

    # inject fzstd asset after <body>
    src = replace_once(src, '<body>', DEC_ASSETS, f'{path}-body')

    # insert decompression block before the class
    src = replace_once(src, '    class EnhancedFountainDecoder {', DEC_BLOCK, f'{path}-block')

    # metadata parse: append v5 compression fields
    src = replace_once(src,
        "          encoderVersion: parts[15] || '3.0', // Default to 3.0 for legacy files\n"
        "          ltParams: parts.slice(16).join(':')\n"
        "        };",
        "          encoderVersion: parts[15] || '3.0', // Default to 3.0 for legacy files\n"
        "          ltParams: parts.slice(16, 19).join(':'),\n"
        "          // v5 compression negotiation fields (encoder announces the algorithm\n"
        "          // applied to the transmitted payload; fileSize/fileChecksum above\n"
        "          // describe the transmitted payload, original* the original file)\n"
        "          compression: parts[19] || 'none',\n"
        "          originalSize: parseInt(parts[20] || '0', 10) || 0,\n"
        "          originalChecksum: parts[21] || ''\n"
        "        };",
        f'{path}-meta')

    # finalizeFile hook: route through deliverFile
    src = replace_once(src,
        "          if (hasEnoughChunks && checksumPassed) {\n"
        "            console.log(`✅ ENTERPRISE: File truly complete and verified - all ${this.totalChunks} chunks recovered with valid checksum`);\n"
        "            if (this.completeCallback) {\n"
        "              this.completeCallback(fileData);\n"
        "            }\n"
        "          } else if (hasEnoughChunks && !checksumPassed) {",
        "          if (hasEnoughChunks && checksumPassed) {\n"
        "            console.log(`✅ ENTERPRISE: File truly complete and verified - all ${this.totalChunks} chunks recovered with valid checksum`);\n"
        "            // Decompress (if the encoder announced an algorithm) and deliver\n"
        "            this.deliverFile(fileData);\n"
        "          } else if (hasEnoughChunks && !checksumPassed) {",
        f'{path}-hook')

    # add deliverFile method before arrayBufferToString
    src = replace_once(src, '      arrayBufferToString(buffer) {', DELIVER, f'{path}-deliver')

    open(path, 'w').write(src)
    print(f'OK {path}')

print('BUILD COMPLETE')
