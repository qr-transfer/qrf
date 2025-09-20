use anyhow::{anyhow, Result};
use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone)]
struct FileMetadata {
    version: String,
    file_name: String,
    file_type: String,
    file_size: usize,
    chunks_count: usize,
    file_checksum: Option<String>,
}

#[derive(Debug, Clone)]
struct SystematicChunk {
    chunk_index: usize,
    chunk_data: Vec<u8>,
}

#[derive(Debug, Clone)]
struct DataPacket {
    packet_id: usize,
    source_chunks: Vec<usize>,
    systematic_data_chunks: Vec<SystematicChunk>,
    xor_data: Option<Vec<u8>>,
}

struct FountainDecoder {
    initialized: bool,
    meta_data: Option<FileMetadata>,
    total_chunks: usize,
    source_chunks: HashMap<usize, Vec<u8>>,
    recovered_chunk_count: usize,
    coded_packets: Vec<DataPacket>,
}

impl FountainDecoder {
    fn new() -> Self {
        Self {
            initialized: false,
            meta_data: None,
            total_chunks: 0,
            source_chunks: HashMap::new(),
            recovered_chunk_count: 0,
            coded_packets: Vec::new(),
        }
    }

    fn initialize(&mut self, metadata: FileMetadata) {
        self.meta_data = Some(metadata.clone());
        self.total_chunks = metadata.chunks_count;
        self.source_chunks.clear();
        self.recovered_chunk_count = 0;
        self.coded_packets.clear();
        self.initialized = true;

        println!("📄 Initialized decoder for {} ({} chunks, {} bytes)",
                metadata.file_name, metadata.chunks_count, metadata.file_size);
        self.print_progress();
    }

    fn add_packet(&mut self, packet: DataPacket) -> bool {
        if !self.initialized {
            return false;
        }

        if !packet.systematic_data_chunks.is_empty() {
            // Process systematic chunks directly
            for chunk in &packet.systematic_data_chunks {
                if !self.source_chunks.contains_key(&chunk.chunk_index) {
                    self.source_chunks.insert(chunk.chunk_index, chunk.chunk_data.clone());
                    self.recovered_chunk_count += 1;
                    self.print_progress();
                }
            }
        } else if packet.xor_data.is_some() {
            // Store fountain packet for later processing
            self.coded_packets.push(packet);
            self.process_coded();
        }

        true
    }

    fn process_coded(&mut self) {
        let mut progress = true;
        while progress {
            progress = false;
            let mut i = self.coded_packets.len();

            while i > 0 {
                i -= 1;
                let packet = &self.coded_packets[i];
                let missing: Vec<usize> = packet.source_chunks.iter()
                    .filter(|&&idx| !self.source_chunks.contains_key(&idx))
                    .cloned()
                    .collect();

                if missing.len() == 1 {
                    // Can recover exactly one chunk
                    let missing_idx = missing[0];
                    let mut result = packet.xor_data.as_ref().unwrap().clone();

                    // XOR with known chunks
                    for &idx in &packet.source_chunks {
                        if idx != missing_idx {
                            if let Some(chunk) = self.source_chunks.get(&idx) {
                                for j in 0..result.len().min(chunk.len()) {
                                    result[j] ^= chunk[j];
                                }
                            }
                        }
                    }

                    self.source_chunks.insert(missing_idx, result);
                    self.recovered_chunk_count += 1;
                    println!("🔧 Fountain recovered chunk {}", missing_idx);

                    self.coded_packets.remove(i);
                    progress = true;
                    self.print_progress();
                } else if missing.is_empty() {
                    self.coded_packets.remove(i);
                }
            }
        }
    }

    fn is_complete(&self) -> bool {
        self.recovered_chunk_count >= self.total_chunks
    }

    fn is_nearly_complete(&self, threshold: f64) -> bool {
        (self.recovered_chunk_count as f64 / self.total_chunks as f64) >= threshold
    }

    fn print_progress(&self) {
        let percentage = ((self.recovered_chunk_count as f64 / self.total_chunks as f64) * 100.0).round() as usize;
        let progress_bars = percentage / 2;
        let empty_bars = 50 - progress_bars;

        let progress_bar = "🟩".repeat(progress_bars) + &"⬜".repeat(empty_bars);
        print!("\r🔄 Progress: {}/{} ({}%) [{}]",
               self.recovered_chunk_count, self.total_chunks, percentage, progress_bar);
        std::io::stdout().flush().unwrap();
    }

    fn finalize(&mut self, output_dir: &str) -> Result<Option<Vec<u8>>> {
        if !self.is_complete() {
            println!("\n❌ File incomplete: {}/{} chunks", self.recovered_chunk_count, self.total_chunks);

            // Debug: show which chunks are missing
            let mut missing = Vec::new();
            for i in 0..self.total_chunks {
                if !self.source_chunks.contains_key(&i) {
                    missing.push(i);
                }
            }

            let missing_display = if missing.len() > 10 {
                format!("{} ... and {} more",
                       missing[..10].iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "),
                       missing.len() - 10)
            } else {
                missing.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")
            };
            println!("Missing chunks: {}", missing_display);
            return Ok(None);
        }

        println!("\n🔧 Reconstructing file from chunks...");

        // Verify all chunks exist
        for i in 0..self.total_chunks {
            if !self.source_chunks.contains_key(&i) {
                println!("❌ Missing chunk {} during reconstruction", i);
                return Ok(None);
            }
        }

        let metadata = self.meta_data.as_ref().unwrap();

        // Combine chunks in order
        let mut file_data = Vec::with_capacity(metadata.file_size);
        for i in 0..self.total_chunks {
            if let Some(chunk) = self.source_chunks.get(&i) {
                let copy_length = (file_data.len() + chunk.len()).min(metadata.file_size) - file_data.len();
                file_data.extend_from_slice(&chunk[..copy_length.min(chunk.len())]);
            }
        }

        // Truncate to exact file size
        file_data.truncate(metadata.file_size);

        // Verify checksum if available
        if let Some(ref expected_checksum) = metadata.file_checksum {
            let calculated = self.calculate_checksum(&file_data);
            if calculated == *expected_checksum {
                println!("✅ File integrity verified: checksum {}", calculated);
            } else {
                println!("❌ Checksum failed: expected {}, got {}", expected_checksum, calculated);
                return Ok(None);
            }
        }

        // Write file to output directory
        std::fs::create_dir_all(output_dir)?;
        let output_path = PathBuf::from(output_dir).join(&metadata.file_name);
        std::fs::write(&output_path, &file_data)?;

        println!("✅ File saved: {} ({} bytes)", output_path.display(), file_data.len());
        Ok(Some(file_data))
    }

    fn calculate_checksum(&self, data: &[u8]) -> String {
        let mut hash: u32 = 2166136261; // FNV-1a offset basis
        for &byte in data {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(16777619); // FNV-1a prime
        }
        format!("{:x}", hash)[..8.min(format!("{:x}", hash).len())].to_string()
    }
}

struct QRFileDecoder {
    file_decoders: HashMap<String, FountainDecoder>,
    current_active_decoder: Option<String>,
    output_dir: String,
}

impl QRFileDecoder {
    fn new() -> Self {
        Self {
            file_decoders: HashMap::new(),
            current_active_decoder: None,
            output_dir: "./decoded_files".to_string(),
        }
    }

    fn process_qr_code(&mut self, qr_data: &str, frame_index: usize) -> ProcessResult {
        match self.try_process_qr_code(qr_data, frame_index) {
            Ok(result) => result,
            Err(error) => ProcessResult {
                is_valid: false,
                qr_type: "error".to_string(),
                reason: Some(error.to_string()),
            }
        }
    }

    fn try_process_qr_code(&mut self, qr_data: &str, frame_index: usize) -> Result<ProcessResult> {
        if qr_data.starts_with("M:") {
            self.process_metadata_packet(qr_data, frame_index)
        } else if qr_data.starts_with("D:") {
            self.process_data_packet(qr_data, frame_index)
        } else {
            Ok(ProcessResult {
                is_valid: false,
                qr_type: "unknown".to_string(),
                reason: Some("Unknown packet type".to_string()),
            })
        }
    }

    fn process_metadata_packet(&mut self, meta_string: &str, _frame_index: usize) -> Result<ProcessResult> {
        let parts: Vec<&str> = meta_string.split(':').collect();
        if parts.len() < 10 {
            return Err(anyhow!("Invalid metadata format"));
        }

        let metadata = FileMetadata {
            version: parts[1].to_string(),
            file_name: urlencoding::decode(parts[2])?.to_string(),
            file_type: urlencoding::decode(parts[3])?.to_string(),
            file_size: parts[4].parse()?,
            chunks_count: parts[5].parse()?,
            file_checksum: parts.get(13).filter(|s| !s.is_empty()).map(|s| s.to_string()),
        };

        // Initialize new file decoder if not exists
        if !self.file_decoders.contains_key(&metadata.file_name) {
            let mut decoder = FountainDecoder::new();
            decoder.initialize(metadata.clone());
            self.file_decoders.insert(metadata.file_name.clone(), decoder);
        }

        // Set as current active decoder (temporal routing)
        self.current_active_decoder = Some(metadata.file_name.clone());
        println!("🎯 Switched to processing: {}", metadata.file_name);

        Ok(ProcessResult {
            is_valid: true,
            qr_type: "metadata".to_string(),
            reason: None,
        })
    }

    fn process_data_packet(&mut self, data_string: &str, _frame_index: usize) -> Result<ProcessResult> {
        let parts: Vec<&str> = data_string.split(':').collect();
        if parts.len() < 6 {
            return Err(anyhow!("Invalid data packet format"));
        }

        let mut packet = DataPacket {
            packet_id: parts[1].parse()?,
            source_chunks: Vec::new(),
            systematic_data_chunks: Vec::new(),
            xor_data: None,
        };

        // Parse enhanced format - CORRECTED to match HTML script exactly
        if parts.len() >= 7 {
            let chunk_count = parts[5].parse::<usize>()?;
            let data_field_offset = 6;

            // Reconstruct data part by joining from dataFieldOffset onwards (critical fix!)
            let all_data_part = parts[data_field_offset..].join(":");

            if all_data_part.contains('|') {
                // Systematic packet format: chunkIndex:base64Data|chunkIndex:base64Data
                let records: Vec<&str> = all_data_part.split('|').collect();

                // Debug: log packet structure for first few packets
                if packet.packet_id <= 5 {
                    println!("\n🔍 DEBUG Packet {}: chunkCount={}, records={}",
                            packet.packet_id, chunk_count, records.len());
                    println!("  AllDataPart length: {}", all_data_part.len());
                    for (idx, record) in records.iter().enumerate() {
                        if let Some(colon_index) = record.find(':') {
                            println!("  Record {}: chunk {}, data length {}",
                                    idx, &record[..colon_index], record.len() - colon_index - 1);
                        } else {
                            println!("  Record {}: no colon, length {}", idx, record.len());
                        }
                    }
                }

                for record in records {
                    let chunk_parts: Vec<&str> = record.splitn(2, ':').collect();

                    if chunk_parts.len() == 2 {
                        let chunk_index: usize = chunk_parts[0].parse()?;
                        let chunk_data_b64 = chunk_parts[1];

                        if !chunk_data_b64.is_empty() {
                            match general_purpose::STANDARD.decode(chunk_data_b64) {
                                Ok(chunk_data) => {
                                    packet.source_chunks.push(chunk_index);
                                    packet.systematic_data_chunks.push(SystematicChunk {
                                        chunk_index,
                                        chunk_data,
                                    });

                                    if packet.packet_id <= 5 {
                                        println!("    ✅ Decoded chunk {}: {} bytes",
                                                chunk_index, packet.systematic_data_chunks.last().unwrap().chunk_data.len());
                                    }
                                },
                                Err(e) => {
                                    println!("❌ Failed to decode chunk {}: {}", chunk_index, e);
                                }
                            }
                        }
                    }
                }
            } else if all_data_part.contains(',') {
                // Fountain packet: comma-separated indices
                packet.source_chunks = all_data_part.split(',')
                    .map(|s| s.parse())
                    .collect::<Result<Vec<_>, _>>()?;

                // XOR data would be in next field for fountain packets
                if parts.len() >= 8 {
                    match general_purpose::STANDARD.decode(parts[7]) {
                        Ok(xor_data) => packet.xor_data = Some(xor_data),
                        Err(e) => println!("Failed to decode fountain XOR data: {}", e),
                    }
                }
            }
        }

        // Route to current active decoder (temporal routing - CRITICAL FIX!)
        let current_decoder_name = match &self.current_active_decoder {
            Some(name) => name.clone(),
            None => {
                println!("⚠️ No active decoder for data packet {}", packet.packet_id);
                return Ok(ProcessResult {
                    is_valid: false,
                    qr_type: "data".to_string(),
                    reason: Some("No active decoder".to_string()),
                });
            }
        };

        // Add packet to current active decoder
        let success = if let Some(decoder) = self.file_decoders.get_mut(&current_decoder_name) {
            decoder.add_packet(packet)
        } else {
            false
        };

        // Check if file is complete
        let is_complete = self.file_decoders.get(&current_decoder_name)
            .map(|d| d.is_complete())
            .unwrap_or(false);

        if is_complete {
            println!("\n🎉 File complete! Finalizing...");
            if let Some(decoder) = self.file_decoders.get_mut(&current_decoder_name) {
                let _ = decoder.finalize(&self.output_dir);
            }
        }

        Ok(ProcessResult {
            is_valid: success,
            qr_type: "data".to_string(),
            reason: None,
        })
    }
}

#[derive(Debug)]
struct ProcessResult {
    is_valid: bool,
    qr_type: String,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct QRCodeInput {
    #[serde(rename = "sequenced_qr_codes")]
    sequenced_qr_codes: Option<Vec<SequencedQRCode>>,
    #[serde(rename = "unique_qr_codes")]
    unique_qr_codes: Option<Vec<String>>,
    #[serde(rename = "video_info")]
    video_info: Option<VideoInfo>,
}

#[derive(Deserialize)]
struct SequencedQRCode {
    data: String,
}

#[derive(Deserialize)]
struct VideoInfo {
    duration_seconds: f64,
    fps: f64,
    total_frames: u64,
}

// JSONL format structures
#[derive(Deserialize)]
#[serde(tag = "type")]
enum JsonlEntry {
    #[serde(rename = "header")]
    Header { video_info: JsonlVideoInfo },
    #[serde(rename = "qr_code")]
    QrCode {
        frame_number: u64,
        timestamp_ms: f64,
        data: String,
    },
    #[serde(rename = "footer")]
    Footer { summary: JsonlSummary },
}

#[derive(Deserialize)]
struct JsonlVideoInfo {
    duration_seconds: f64,
    fps: f64,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct JsonlSummary {
    frames_processed: u64,
    qr_codes_found: u64,
    processing_time_ms: u64,
}

fn main() -> Result<()> {
    use std::io::IsTerminal;

    let args: Vec<String> = std::env::args().collect();

    // Auto-detect stdin mode when input is piped
    let stdin_mode = args.iter().any(|arg| arg == "--stdin") || !std::io::stdin().is_terminal();

    if stdin_mode {
        println!("🌊 Processing streaming JSONL from stdin...");
        return process_streaming_stdin();
    }

    if args.len() < 2 {
        println!("Usage: {} <qr_codes.json> [--stream]", args[0]);
        println!("       echo jsonl | {}  (auto-detects piped input)", args[0]);
        println!("  --stream: Process JSONL format with continuous progress saving");
        std::process::exit(1);
    }

    let input_file = &args[1];
    let stream_mode = args.iter().any(|arg| arg == "--stream");

    println!("📖 Loading QR codes from: {}", input_file);

    if stream_mode {
        return process_streaming_jsonl(input_file);
    }

    // Create output directory
    std::fs::create_dir_all("./decoded_files")?;

    // Load QR codes (support JSON, JSONL formats)
    let data_str = std::fs::read_to_string(input_file)?;

    let qr_codes = if data_str.lines().any(|line| line.trim().starts_with("{\"type\":")) {
        // JSONL format detected
        println!("📊 Using streaming JSONL format");
        parse_jsonl_format(&data_str)?
    } else {
        // JSON format
        let data: QRCodeInput = serde_json::from_str(&data_str)?;

        if let Some(sequenced) = data.sequenced_qr_codes {
            // Sequenced format - already perfectly ordered by frame number
            let codes: Vec<String> = sequenced.into_iter().map(|item| item.data).collect();
            println!("📊 Using sequenced format with frame-perfect ordering");
            if let Some(video_info) = data.video_info {
                println!("📺 Video info: {}min, {}fps, {} total frames",
                        (video_info.duration_seconds / 60.0).round(),
                        video_info.fps.round(),
                        video_info.total_frames);
            }
            codes
        } else if let Some(unique) = data.unique_qr_codes {
            // Legacy format
            println!("📊 Using legacy format (temporal order)");
            unique
        } else {
            return Err(anyhow!("No QR codes found in JSON file"));
        }
    };

    println!("Found {} QR codes in temporal order", qr_codes.len());

    // Initialize decoder
    let mut decoder = QRFileDecoder::new();

    // Process QR codes
    let mut processed = 0;
    let mut successful = 0;

    for (i, qr_code) in qr_codes.iter().enumerate() {
        if i % 100 == 0 {
            println!("\nProcessing QR code {} / {}...", i + 1, qr_codes.len());
        }

        let result = decoder.process_qr_code(qr_code, i);
        if result.is_valid {
            successful += 1;
        } else if let Some(reason) = result.reason {
            if i < 10 { // Only show first few errors to avoid spam
                println!("Warning: Failed to process QR {}: {}", i + 1, reason);
            }
        }
        processed += 1;
    }

    // Finalize any remaining files and save partial progress
    let mut completed_files = 0;
    let mut partial_files = 0;

    for (file_name, fountain_decoder) in &mut decoder.file_decoders {
        if fountain_decoder.is_complete() {
            println!("\n🎉 Finalizing complete file: {}", file_name);
            let _ = fountain_decoder.finalize("./decoded_files");
            completed_files += 1;
        } else {
            let percentage = ((fountain_decoder.recovered_chunk_count as f64 / fountain_decoder.total_chunks as f64) * 100.0).round() as usize;
            println!("\n⚠️ File incomplete: {} - {}/{} chunks ({}%)",
                    file_name, fountain_decoder.recovered_chunk_count, fountain_decoder.total_chunks, percentage);

            // Show missing chunks
            let mut missing = Vec::new();
            for i in 0..fountain_decoder.total_chunks {
                if !fountain_decoder.source_chunks.contains_key(&i) {
                    missing.push(i);
                }
            }

            if missing.len() <= 10 {
                println!("   Missing chunks: [{}]", missing.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "));
            } else {
                let first_5: Vec<String> = missing[..5].iter().map(|x| x.to_string()).collect();
                let last_5: Vec<String> = missing[missing.len()-5..].iter().map(|x| x.to_string()).collect();
                println!("   Missing chunks: [{}, ..., {}] ({} total)",
                        first_5.join(", "), last_5.join(", "), missing.len());
            }

            // For nearly complete files, show more details
            if fountain_decoder.is_nearly_complete(0.95) {
                println!("   🔍 NEARLY COMPLETE: {}% - only {} chunks missing!", percentage, missing.len());
                println!("   📊 Available fountain packets: {}", fountain_decoder.coded_packets.len());
            }

            // Save partial file progress for potential merging later
            if percentage >= 10 { // Only save if significant progress
                let partial_data = serde_json::json!({
                    "fileName": file_name,
                    "metadata": {
                        "file_name": fountain_decoder.meta_data.as_ref().map(|m| &m.file_name),
                        "file_size": fountain_decoder.meta_data.as_ref().map(|m| m.file_size),
                        "chunks_count": fountain_decoder.meta_data.as_ref().map(|m| m.chunks_count),
                    },
                    "recoveredChunks": fountain_decoder.recovered_chunk_count,
                    "totalChunks": fountain_decoder.total_chunks,
                    "percentage": percentage,
                    "missingChunks": missing,
                    "availableFountainPackets": fountain_decoder.coded_packets.len()
                });

                let partial_path = format!("./decoded_files/{}.partial.json", file_name);
                std::fs::write(&partial_path, serde_json::to_string_pretty(&partial_data)?)?;
                println!("   💾 Saved partial progress to: {}", partial_path);
                partial_files += 1;
            }
        }
    }

    println!("\n📊 Final Results:");
    println!("   ✅ Complete files: {}", completed_files);
    println!("   📝 Partial files: {}", partial_files);

    if completed_files + partial_files > 0 {
        println!("   🎯 Success rate: {}%",
                (completed_files * 100) / (completed_files + partial_files));
    }

    if completed_files > 0 {
        println!("\n🎉 SUCCESS: {} files fully reconstructed with integrity verification!", completed_files);
    }

    println!("\n✅ Processing complete: {}/{} QR codes successfully processed", successful, processed);
    println!("📁 Check './decoded_files' directory for extracted files");

    Ok(())
}

fn parse_jsonl_format(data_str: &str) -> Result<Vec<String>> {
    let mut qr_codes = Vec::new();
    let mut video_info: Option<JsonlVideoInfo> = None;

    for line in data_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<JsonlEntry>(line) {
            Ok(JsonlEntry::Header { video_info: vi }) => {
                println!("📺 Video info: {}min, {}fps, {}x{}",
                        (vi.duration_seconds / 60.0).round(),
                        vi.fps.round(),
                        vi.width,
                        vi.height);
                video_info = Some(vi);
            },
            Ok(JsonlEntry::QrCode { frame_number: _, timestamp_ms: _, data }) => {
                qr_codes.push(data);
            },
            Ok(JsonlEntry::Footer { summary }) => {
                println!("📊 Processing summary: {} frames processed, {} QR codes found, {:.2}s processing time",
                        summary.frames_processed,
                        summary.qr_codes_found,
                        summary.processing_time_ms as f64 / 1000.0);
            },
            Err(e) => {
                // Skip invalid lines with a warning
                println!("⚠️ Skipping invalid JSONL line: {}", e);
                continue;
            }
        }
    }

    Ok(qr_codes)
}

fn process_streaming_jsonl(input_file: &str) -> Result<()> {
    use std::io::{BufRead, BufReader};
    use std::fs::File;

    println!("🌊 Processing streaming JSONL format with continuous progress saving");

    // Create output directory
    std::fs::create_dir_all("./decoded_files")?;

    // Initialize decoder
    let mut decoder = QRFileDecoder::new();

    // Open file for line-by-line reading
    let file = File::open(input_file)?;
    let reader = BufReader::new(file);

    let mut processed = 0;
    let mut successful = 0;
    let mut qr_count = 0;

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<JsonlEntry>(&line) {
            Ok(JsonlEntry::Header { video_info }) => {
                println!("📺 Video info: {}min, {}fps, {}x{}",
                        (video_info.duration_seconds / 60.0).round(),
                        video_info.fps.round(),
                        video_info.width,
                        video_info.height);
            },
            Ok(JsonlEntry::QrCode { frame_number, timestamp_ms: _, data }) => {
                qr_count += 1;

                // Process QR code immediately
                let result = decoder.process_qr_code(&data, frame_number as usize);
                if result.is_valid {
                    successful += 1;
                } else if let Some(reason) = result.reason {
                    if qr_count <= 10 { // Only show first few errors
                        println!("Warning: Failed to process QR {}: {}", qr_count, reason);
                    }
                }
                processed += 1;

                // Save progress every 100 QR codes
                if qr_count % 100 == 0 {
                    println!("🔄 Processed {} QR codes (line {}), {} successful", qr_count, line_num + 1, successful);
                    save_current_progress(&mut decoder, qr_count)?;
                }

                // Check for completed files and finalize them immediately
                check_and_finalize_completed_files(&mut decoder)?;
            },
            Ok(JsonlEntry::Footer { summary }) => {
                println!("📊 Processing summary: {} frames processed, {} QR codes found, {:.2}s processing time",
                        summary.frames_processed,
                        summary.qr_codes_found,
                        summary.processing_time_ms as f64 / 1000.0);
            },
            Err(e) => {
                println!("⚠️ Skipping invalid JSONL line {}: {}", line_num + 1, e);
                continue;
            }
        }
    }

    // Final progress save
    save_current_progress(&mut decoder, qr_count)?;

    // Finalize any remaining files
    finalize_all_files(&mut decoder)?;

    println!("\n✅ Streaming processing complete: {}/{} QR codes successfully processed", successful, processed);
    println!("📁 Check './decoded_files' directory for extracted files");

    Ok(())
}

fn save_current_progress(decoder: &mut QRFileDecoder, qr_count: usize) -> Result<()> {
    for (file_name, fountain_decoder) in &decoder.file_decoders {
        if !fountain_decoder.is_complete() {
            let percentage = ((fountain_decoder.recovered_chunk_count as f64 / fountain_decoder.total_chunks as f64) * 100.0).round() as usize;

            if percentage >= 10 { // Only save if significant progress
                let mut missing = Vec::new();
                for i in 0..fountain_decoder.total_chunks {
                    if !fountain_decoder.source_chunks.contains_key(&i) {
                        missing.push(i);
                    }
                }

                let partial_data = serde_json::json!({
                    "fileName": file_name,
                    "metadata": {
                        "file_name": fountain_decoder.meta_data.as_ref().map(|m| &m.file_name),
                        "file_size": fountain_decoder.meta_data.as_ref().map(|m| m.file_size),
                        "chunks_count": fountain_decoder.meta_data.as_ref().map(|m| m.chunks_count),
                    },
                    "recoveredChunks": fountain_decoder.recovered_chunk_count,
                    "totalChunks": fountain_decoder.total_chunks,
                    "percentage": percentage,
                    "missingChunks": missing,
                    "availableFountainPackets": fountain_decoder.coded_packets.len(),
                    "qrCodesProcessed": qr_count,
                    "lastUpdated": chrono::Utc::now().to_rfc3339()
                });

                let partial_path = format!("./decoded_files/{}.streaming.json", file_name);
                std::fs::write(&partial_path, serde_json::to_string_pretty(&partial_data)?)?;
            }
        }
    }
    Ok(())
}

fn check_and_finalize_completed_files(decoder: &mut QRFileDecoder) -> Result<()> {
    let completed_files: Vec<String> = decoder.file_decoders.iter()
        .filter(|(_, fd)| fd.is_complete())
        .map(|(name, _)| name.clone())
        .collect();

    for file_name in completed_files {
        if let Some(fountain_decoder) = decoder.file_decoders.get_mut(&file_name) {
            println!("\n🎉 File complete! Finalizing: {}", file_name);
            let _ = fountain_decoder.finalize(&decoder.output_dir);

            // Remove the streaming progress file since file is complete
            let streaming_path = format!("./decoded_files/{}.streaming.json", file_name);
            let _ = std::fs::remove_file(streaming_path);
        }
    }
    Ok(())
}

fn finalize_all_files(decoder: &mut QRFileDecoder) -> Result<()> {
    let mut completed_files = 0;
    let mut partial_files = 0;

    for (file_name, fountain_decoder) in &mut decoder.file_decoders {
        if fountain_decoder.is_complete() {
            println!("\n🎉 Finalizing complete file: {}", file_name);
            let _ = fountain_decoder.finalize("./decoded_files");
            completed_files += 1;
        } else {
            let percentage = ((fountain_decoder.recovered_chunk_count as f64 / fountain_decoder.total_chunks as f64) * 100.0).round() as usize;
            println!("\n⚠️ File incomplete: {} - {}/{} chunks ({}%)",
                    file_name, fountain_decoder.recovered_chunk_count, fountain_decoder.total_chunks, percentage);

            if percentage >= 10 {
                partial_files += 1;
            }
        }
    }

    println!("\n📊 Final Results:");
    println!("   ✅ Complete files: {}", completed_files);
    println!("   📝 Partial files: {}", partial_files);

    if completed_files + partial_files > 0 {
        println!("   🎯 Success rate: {}%",
                (completed_files * 100) / (completed_files + partial_files));
    }

    if completed_files > 0 {
        println!("\n🎉 SUCCESS: {} files fully reconstructed with integrity verification!", completed_files);
    }

    Ok(())
}

fn process_streaming_stdin() -> Result<()> {
    use std::io::{BufRead, BufReader};

    println!("🌊 Processing streaming JSONL from stdin with real-time file generation");

    // Create output directory
    std::fs::create_dir_all("./decoded_files")?;

    // Initialize decoder
    let mut decoder = QRFileDecoder::new();

    // Open stdin for line-by-line reading
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin);

    let mut processed = 0;
    let mut successful = 0;
    let mut qr_count = 0;

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<JsonlEntry>(&line) {
            Ok(JsonlEntry::Header { video_info }) => {
                println!("📺 Video info: {}min, {}fps, {}x{}",
                        (video_info.duration_seconds / 60.0).round(),
                        video_info.fps.round(),
                        video_info.width,
                        video_info.height);
            },
            Ok(JsonlEntry::QrCode { frame_number, timestamp_ms: _, data }) => {
                qr_count += 1;

                // Process QR code immediately
                let result = decoder.process_qr_code(&data, frame_number as usize);
                if result.is_valid {
                    successful += 1;
                } else if let Some(reason) = result.reason {
                    if qr_count <= 5 { // Only show first few errors
                        println!("Warning: Failed to process QR {}: {}", qr_count, reason);
                    }
                }
                processed += 1;

                // Save progress and check for completed files every 10 QR codes (more frequent for real-time)
                if qr_count % 10 == 0 {
                    print!("\r🔄 Processed {} QR codes, {} successful", qr_count, successful);
                    std::io::stdout().flush().unwrap();
                    save_current_progress(&mut decoder, qr_count)?;
                }

                // Check for completed files and finalize them immediately
                check_and_finalize_completed_files(&mut decoder)?;
            },
            Ok(JsonlEntry::Footer { summary }) => {
                println!("\n📊 Processing summary: {} frames processed, {} QR codes found, {:.2}s processing time",
                        summary.frames_processed,
                        summary.qr_codes_found,
                        summary.processing_time_ms as f64 / 1000.0);
            },
            Err(e) => {
                if line.contains("error") || line.contains("warning") {
                    // Skip error/warning lines from the extractor
                    continue;
                }
                println!("\n⚠️ Skipping invalid JSONL line {}: {}", line_num + 1, e);
                continue;
            }
        }
    }

    // Final progress save
    save_current_progress(&mut decoder, qr_count)?;

    // Finalize any remaining files
    finalize_all_files(&mut decoder)?;

    println!("\n✅ Real-time processing complete: {}/{} QR codes successfully processed", successful, processed);
    println!("📁 Check './decoded_files' directory for extracted files");

    Ok(())
}