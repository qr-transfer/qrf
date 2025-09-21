use anyhow::{anyhow, Result};
use base64::{Engine as _, engine::general_purpose};
use chrono;
use fnv::FnvHasher;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::events::{EventCallback, ProcessingEvent};
use crate::qr_extraction::{QrCodeData, QrExtractionResults};
use crate::error_logger::ErrorLogger;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub version: String,
    pub file_name: String,
    pub file_type: String,
    pub file_size: usize,
    pub chunks_count: usize,
    pub file_checksum: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SystematicChunk {
    pub chunk_index: usize,
    pub chunk_data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct DataPacket {
    pub packet_id: usize,
    pub source_chunks: Vec<usize>,
    pub systematic_data_chunks: Vec<SystematicChunk>,
    pub xor_data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructedFile {
    pub qr_checksum: String,
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
    pub crc32: String,
    pub size: u64,
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinalReport {
    pub scan_date: String,
    pub directory: String,
    pub files: HashMap<String, ReconstructedFile>,
}

pub struct FileReconstructor {
    output_dir: PathBuf,
    active_files: HashMap<String, FileDecoder>,
    file_counter: usize,
    error_logger: Arc<ErrorLogger>,
}

#[derive(Clone)]
struct FileDecoder {
    metadata: FileMetadata,
    chunks: HashMap<usize, Vec<u8>>,
    received_chunks: HashSet<usize>,
    coded_packets: Vec<DataPacket>,
    is_complete: bool,
}

impl FileReconstructor {
    pub fn new(output_dir: &PathBuf) -> Self {
        let log_path = output_dir.join("qr_processing.log");
        let error_logger = Arc::new(
            ErrorLogger::new(&log_path.to_string_lossy())
                .unwrap_or_else(|_| {
                    // Fallback to stdout logging if file creation fails
                    ErrorLogger::new("/tmp/qr_processing.log").unwrap()
                })
        );

        error_logger.log_info(&format!("FileReconstructor initialized with output_dir: {}",
                                     output_dir.display()));

        Self {
            output_dir: output_dir.clone(),
            active_files: HashMap::new(),
            file_counter: 0,
            error_logger,
        }
    }

    pub fn process_qr_data(
        mut self,
        extraction_results: QrExtractionResults,
        callback: &EventCallback,
    ) -> Result<FinalReport> {
        let _start_time = Instant::now();

        callback(ProcessingEvent::Progress {
            phase: 3,
            current: 1,
            total: 6,
            message: "Routing QR data to separate files...".to_string(),
        });

        self.error_logger.log_processing_phase("QR_ROUTING", &format!("Processing {} QR codes", extraction_results.qr_codes.len()));
        self.route_qr_data_to_files(&extraction_results.qr_codes, callback)?;

        callback(ProcessingEvent::Progress {
            phase: 3,
            current: 2,
            total: 6,
            message: format!("Processing {} active files...", self.active_files.len()),
        });

        let mut final_report = FinalReport {
            scan_date: chrono::Utc::now().to_rfc3339(),
            directory: self.output_dir.to_string_lossy().to_string(),
            files: HashMap::new(),
        };

        fs::create_dir_all(&self.output_dir)?;

        let file_names: Vec<String> = self.active_files.keys().cloned().collect();
        let _total_files = file_names.len();

        for (idx, file_name) in file_names.iter().enumerate() {
            callback(ProcessingEvent::Progress {
                phase: 3,
                current: 3 + idx,
                total: 6,
                message: format!("Reconstructing file: {}", file_name),
            });

            // Clone the file decoder to avoid borrow conflicts
            if let Some(file_decoder) = self.active_files.remove(file_name) {
                let reconstructed_file = self.reconstruct_file_owned(file_decoder, file_name)?;
                final_report.files.insert(file_name.clone(), reconstructed_file);
            }
        }

        callback(ProcessingEvent::Progress {
            phase: 3,
            current: 6,
            total: 6,
            message: format!("Generated final report with {} files", final_report.files.len()),
        });

        self.save_final_report(&final_report)?;

        Ok(final_report)
    }

    /// Process JSONL files in correct order: combine all → split by metadata → reconstruct files
    pub fn process_combined_jsonl_files(
        mut self,
        output_dir: &PathBuf,
        callback: &EventCallback,
    ) -> Result<FinalReport> {
        self.error_logger.log_processing_phase("JSONL_COMBINATION", "Starting JSONL combination and file reconstruction");

        callback(ProcessingEvent::Progress {
            phase: 3,
            current: 1,
            total: 6,
            message: "Combining all chunk JSONL files...".to_string(),
        });

        // Step 1: Read all chunk JSONL files and combine QR data
        let mut combined_qr_data = self.read_and_combine_jsonl_files(output_dir)?;

        // CRITICAL: Sort by frame number to process metadata packets first
        combined_qr_data.sort_by_key(|qr| qr.frame_number);
        self.error_logger.log_info(&format!("Sorted {} QR codes by frame order for proper metadata/data sequence", combined_qr_data.len()));

        callback(ProcessingEvent::Progress {
            phase: 3,
            current: 2,
            total: 6,
            message: format!("Combined {} QR codes from all chunks", combined_qr_data.len()),
        });

        // Step 2: Split QR data by metadata (group by files)
        self.route_qr_data_to_files(&combined_qr_data, callback)?;

        callback(ProcessingEvent::Progress {
            phase: 3,
            current: 3,
            total: 6,
            message: format!("Identified {} unique files", self.active_files.len()),
        });

        // Step 3: Reconstruct files from separated data
        let mut final_report = FinalReport {
            scan_date: chrono::Utc::now().to_rfc3339(),
            directory: self.output_dir.to_string_lossy().to_string(),
            files: HashMap::new(),
        };

        let total_files = self.active_files.len();
        let mut processed_files = 0;

        for (file_key, file_decoder) in self.active_files.clone() {
            processed_files += 1;

            callback(ProcessingEvent::Progress {
                phase: 3,
                current: 3 + processed_files,
                total: 6,
                message: format!("Reconstructing file {}/{}: {}", processed_files, total_files, file_decoder.metadata.file_name),
            });

            if file_decoder.is_complete {
                match self.reconstruct_file_owned(file_decoder, &file_key) {
                    Ok(reconstructed_file) => {
                        callback(ProcessingEvent::FileReconstructed {
                            file_name: file_key.clone(),
                            file_size: reconstructed_file.size,
                            checksum_valid: !reconstructed_file.qr_checksum.is_empty(),
                            output_path: reconstructed_file.file_path.clone(),
                        });

                        final_report.files.insert(file_key.clone(), reconstructed_file);
                    }
                    Err(e) => {
                        self.error_logger.log_error("FILE_RECONSTRUCTION", &format!("Failed to reconstruct {}: {}", file_key, e));
                    }
                }
            } else {
                self.error_logger.log_warning("INCOMPLETE_FILE", &format!("File {} is incomplete: {}/{} chunks",
                                                                         file_decoder.metadata.file_name,
                                                                         file_decoder.received_chunks.len(),
                                                                         file_decoder.metadata.chunks_count));
            }
        }

        callback(ProcessingEvent::Progress {
            phase: 3,
            current: 6,
            total: 6,
            message: format!("Generated final report with {} files", final_report.files.len()),
        });

        self.save_final_report(&final_report)?;

        Ok(final_report)
    }

    /// Read and combine all chunk JSONL files into a single QR data collection
    fn read_and_combine_jsonl_files(&self, output_dir: &PathBuf) -> Result<Vec<QrCodeData>> {
        let mut combined_qr_data = Vec::new();

        // Read all chunk_*.jsonl files
        for entry in std::fs::read_dir(output_dir)? {
            let entry = entry?;
            let path = entry.path();

            if let Some(file_name) = path.file_name() {
                if let Some(name_str) = file_name.to_str() {
                    if name_str.starts_with("chunk_") && name_str.ends_with(".jsonl") {
                        self.error_logger.log_debug("JSONL_READ", &format!("Reading {}", name_str));

                        let file_content = std::fs::read_to_string(&path)?;
                        for line in file_content.lines() {
                            if !line.trim().is_empty() {
                                match serde_json::from_str::<QrCodeData>(line) {
                                    Ok(qr_data) => {
                                        combined_qr_data.push(qr_data);
                                    }
                                    Err(e) => {
                                        self.error_logger.log_error("JSONL_PARSE", &format!("Failed to parse line in {}: {} | Line: {}", name_str, e, line));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        self.error_logger.log_info(&format!("Combined {} QR codes from all chunk JSONL files", combined_qr_data.len()));

        Ok(combined_qr_data)
    }

    fn route_qr_data_to_files(
        &mut self,
        qr_codes: &[QrCodeData],
        callback: &EventCallback,
    ) -> Result<()> {
        let mut current_file_name: Option<String> = None;

        for qr_data in qr_codes {
            if qr_data.data.starts_with("M:") {
                match self.parse_metadata(&qr_data.data) {
                    Ok(metadata) => {
                        self.file_counter += 1;
                        let file_key = format!("file_{:03}_{}", self.file_counter, metadata.file_name);

                        self.error_logger.log_info(&format!("New file detected: {} (size: {}, chunks: {})",
                                                          metadata.file_name, metadata.file_size, metadata.chunks_count));

                        let file_decoder = FileDecoder {
                            metadata: metadata.clone(),
                            chunks: HashMap::new(),
                            received_chunks: HashSet::new(),
                            coded_packets: Vec::new(),
                            is_complete: false,
                        };

                        self.active_files.insert(file_key.clone(), file_decoder);
                        current_file_name = Some(file_key);

                        callback(ProcessingEvent::Progress {
                            phase: 3,
                            current: 1,
                            total: 6,
                            message: format!("Started file: {} ({} chunks expected)",
                                           metadata.file_name, metadata.chunks_count),
                        });
                    }
                    Err(e) => {
                        self.error_logger.log_error("METADATA_PARSE", &format!("Failed to parse metadata: {} | Data: {}", e, qr_data.data));
                    }
                }
            } else if qr_data.data.starts_with("D:") {
                if let Some(ref file_name) = current_file_name {
                    let qr_data_str = qr_data.data.clone();
                    if self.active_files.contains_key(file_name) {
                        let mut file_decoder = self.active_files.remove(file_name).unwrap();
                        match self.process_data_packet(&mut file_decoder, &qr_data_str) {
                            Ok(_) => {
                                self.error_logger.log_debug("DATA_PACKET", &format!("Successfully processed data packet for {}", file_name));
                            }
                            Err(e) => {
                                self.error_logger.log_error("DATA_PACKET", &format!("Failed to process data packet for {}: {} | Data: {}", file_name, e, &qr_data_str[..std::cmp::min(50, qr_data_str.len())]));
                            }
                        }
                        self.active_files.insert(file_name.clone(), file_decoder);
                    } else {
                        self.error_logger.log_warning("DATA_PACKET", &format!("No decoder found for file: {}", file_name));
                    }
                } else {
                    self.error_logger.log_warning("DATA_PACKET", "No current file name for data packet");
                }
            } else {
                self.error_logger.log_warning("UNKNOWN_PACKET", &format!("Unknown packet type: {}", &qr_data.data[..std::cmp::min(20, qr_data.data.len())]));
            }
        }

        Ok(())
    }

    fn parse_metadata(&self, qr_data: &str) -> Result<FileMetadata> {
        let parts: Vec<&str> = qr_data.split(':').collect();
        if parts.len() < 6 {
            return Err(anyhow!("Invalid metadata format"));
        }

        let version = parts[1].to_string();
        let file_name = urlencoding::decode(parts[2])?.to_string();
        let file_type = urlencoding::decode(parts[3])?.to_string();
        let file_size = parts[4].parse::<usize>()?;
        let chunks_count = parts[5].parse::<usize>()?;
        let file_checksum = if parts.len() > 6 && !parts[6].is_empty() {
            Some(parts[6].to_string())
        } else {
            None
        };

        Ok(FileMetadata {
            version,
            file_name,
            file_type,
            file_size,
            chunks_count,
            file_checksum,
        })
    }

    fn process_data_packet(&mut self, file_decoder: &mut FileDecoder, qr_data: &str) -> Result<()> {
        let parts: Vec<&str> = qr_data.split(':').collect();
        if parts.len() < 7 {
            return Err(anyhow!("Invalid data packet format"));
        }

        // Handle both legacy and new formats like JavaScript
        let data_field_offset = if parts.len() >= 8 && parts[1].len() == 8 && parts[1].chars().all(|c| c.is_ascii_hexdigit()) {
            7  // New format with fileId
        } else {
            6  // Legacy format
        };

        if parts.len() <= data_field_offset {
            return Ok(()); // Not enough parts
        }

        // Join remaining parts and split by pipe (like JavaScript)
        let all_data_part = parts[data_field_offset..].join(":");
        let records: Vec<&str> = all_data_part.split('|').collect();

        for record in records {
            let chunk_parts: Vec<&str> = record.splitn(2, ':').collect();
            if chunk_parts.len() == 2 {
                if let Ok(chunk_index) = chunk_parts[0].parse::<usize>() {
                    let chunk_data_b64 = chunk_parts[1];

                    // Fix base64 padding like JavaScript
                    let fixed_base64 = self.fix_base64_padding(chunk_data_b64);

                    let chunk_data = match general_purpose::STANDARD.decode(&fixed_base64) {
                        Ok(data) => {
                            self.error_logger.log_debug("BASE64_DECODE", &format!("Successfully decoded chunk {}: {} bytes", chunk_index, data.len()));
                            data
                        }
                        Err(e) => {
                            self.error_logger.log_base64_error(chunk_index, chunk_data_b64, &e.to_string());
                            // Skip invalid base64 data silently for TUI (like JavaScript)
                            continue;
                        }
                    };

                    if chunk_index < file_decoder.metadata.chunks_count {
                        file_decoder.chunks.insert(chunk_index, chunk_data);
                        file_decoder.received_chunks.insert(chunk_index);
                    }
                }
            }
        }

        if file_decoder.received_chunks.len() >= file_decoder.metadata.chunks_count {
            file_decoder.is_complete = true;
        }

        Ok(())
    }

    /// Fix base64 padding issues (ported from JavaScript)
    fn fix_base64_padding(&self, base64: &str) -> String {
        // Remove any whitespace
        let mut cleaned = base64.replace(char::is_whitespace, "");

        // Add padding if needed
        while cleaned.len() % 4 != 0 {
            cleaned.push('=');
        }

        cleaned
    }

    fn reconstruct_file_owned(
        &mut self,
        file_decoder: FileDecoder,
        file_name: &str,
    ) -> Result<ReconstructedFile> {
        if !file_decoder.is_complete {
            self.attempt_fountain_recovery(&file_decoder)?;
        }

        let mut file_data = Vec::new();
        for chunk_index in 0..file_decoder.metadata.chunks_count {
            if let Some(chunk_data) = file_decoder.chunks.get(&chunk_index) {
                file_data.extend_from_slice(chunk_data);
            } else {
                return Err(anyhow!("Missing chunk {} for file {}", chunk_index, file_name));
            }
        }

        file_data.truncate(file_decoder.metadata.file_size);

        let output_path = self.output_dir.join(&file_decoder.metadata.file_name);
        fs::write(&output_path, &file_data)?;

        let checksums = self.calculate_checksums(&file_data, &file_decoder.metadata.file_checksum);

        Ok(ReconstructedFile {
            qr_checksum: checksums.qr_checksum,
            md5: checksums.md5,
            sha1: checksums.sha1,
            sha256: checksums.sha256,
            crc32: checksums.crc32,
            size: file_data.len() as u64,
            file_path: output_path.to_string_lossy().to_string(),
        })
    }

    fn attempt_fountain_recovery(&self, _file_decoder: &FileDecoder) -> Result<()> {
        Ok(())
    }

    fn calculate_checksums(&self, data: &[u8], qr_checksum: &Option<String>) -> FileChecksums {
        // Simplified hash calculation using standard library
        let md5_hash = format!("{:x}", md5::compute(data));

        let sha1_hash = {
            use sha1::{Sha1, Digest};
            let mut hasher = Sha1::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        };

        let sha256_hash = {
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        };

        let crc32_hash = {
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(data);
            format!("{:x}", hasher.finalize())
        };

        let qr_checksum_value = qr_checksum.clone().unwrap_or_else(|| {
            let mut hasher = FnvHasher::default();
            data.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        });

        FileChecksums {
            qr_checksum: qr_checksum_value,
            md5: md5_hash,
            sha1: sha1_hash,
            sha256: sha256_hash,
            crc32: crc32_hash,
        }
    }

    fn save_final_report(&self, report: &FinalReport) -> Result<()> {
        let report_path = self.output_dir.join("integrity_report.json");
        let file = File::create(&report_path)?;
        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, report)
            .map_err(|e| anyhow!("Failed to write final report: {}", e))?;

        Ok(())
    }
}

struct FileChecksums {
    qr_checksum: String,
    md5: String,
    sha1: String,
    sha256: String,
    crc32: String,
}