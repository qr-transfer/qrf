use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use crate::qr_extraction::QrCodeData;
use crate::error_logger::ErrorLogger;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkCompletionInfo {
    pub chunk_id: usize,
    pub expected_frames: u64,
    pub actual_frames_processed: u64,
    pub expected_duration_secs: f64,
    pub qr_codes_found: usize,
    pub jsonl_size_bytes: u64,
    pub completion_percentage: f64,
    pub is_complete: bool,
    pub completion_reason: String,
}

pub struct CompletionDetector {
    total_frames: u64,
    total_duration: f64,
    frame_rate: f64,
    chunk_count: usize,
    skip_frames: usize,
    logger: ErrorLogger,
}

impl CompletionDetector {
    pub fn new(total_frames: u64, total_duration: f64, frame_rate: f64, chunk_count: usize, skip_frames: usize, output_dir: &PathBuf) -> Result<Self> {
        let log_path = output_dir.join("processing.log");
        let logger = ErrorLogger::new(&log_path.to_string_lossy())
            .unwrap_or_else(|_| ErrorLogger::new("/tmp/processing.log").unwrap());

        logger.log_info(&format!("Completion Detector initialized: {} total frames, {:.1}s duration, {} chunks",
                               total_frames, total_duration, chunk_count));

        Ok(Self {
            total_frames,
            total_duration,
            frame_rate,
            chunk_count,
            skip_frames,
            logger,
        })
    }

    pub fn analyze_chunk_completion(&self, chunk_id: usize, output_dir: &PathBuf) -> Result<ChunkCompletionInfo> {
        let expected_frames = self.calculate_expected_frames(chunk_id);
        let expected_duration = self.calculate_expected_duration(chunk_id);

        let jsonl_file = output_dir.join(format!("chunk_{:03}.jsonl", chunk_id + 1));

        if !jsonl_file.exists() {
            return Ok(ChunkCompletionInfo {
                chunk_id,
                expected_frames,
                actual_frames_processed: 0,
                expected_duration_secs: expected_duration,
                qr_codes_found: 0,
                jsonl_size_bytes: 0,
                completion_percentage: 0.0,
                is_complete: false,
                completion_reason: "JSONL file does not exist".to_string(),
            });
        }

        let (actual_frames, qr_codes, max_frame, min_frame) = self.analyze_jsonl_content(&jsonl_file)?;
        let file_size = fs::metadata(&jsonl_file)?.len();

        // Multiple completion criteria
        let (is_complete, reason) = self.determine_completion(
            chunk_id, expected_frames, actual_frames, max_frame, min_frame, qr_codes, expected_duration
        );

        let completion_percentage = if expected_frames > 0 {
            (actual_frames as f64 / expected_frames as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        self.logger.log_info(&format!("Chunk {}: {:.1}% complete - {} frames processed, {} QR codes, {}",
                                    chunk_id + 1, completion_percentage, actual_frames, qr_codes, reason));

        Ok(ChunkCompletionInfo {
            chunk_id,
            expected_frames,
            actual_frames_processed: actual_frames,
            expected_duration_secs: expected_duration,
            qr_codes_found: qr_codes,
            jsonl_size_bytes: file_size,
            completion_percentage,
            is_complete,
            completion_reason: reason,
        })
    }

    fn calculate_expected_frames(&self, chunk_id: usize) -> u64 {
        let chunk_duration = self.total_duration / self.chunk_count as f64;

        // Last chunk might be shorter
        if chunk_id == self.chunk_count - 1 {
            let remaining_duration = self.total_duration - (chunk_duration * chunk_id as f64);
            (remaining_duration * self.frame_rate) as u64
        } else {
            (chunk_duration * self.frame_rate) as u64
        }
    }

    fn calculate_expected_duration(&self, chunk_id: usize) -> f64 {
        let chunk_duration = self.total_duration / self.chunk_count as f64;

        if chunk_id == self.chunk_count - 1 {
            self.total_duration - (chunk_duration * chunk_id as f64)
        } else {
            chunk_duration
        }
    }

    fn analyze_jsonl_content(&self, jsonl_file: &PathBuf) -> Result<(u64, usize, u64, u64)> {
        let content = fs::read_to_string(jsonl_file)?;
        let lines: Vec<&str> = content.lines().filter(|line| !line.trim().is_empty()).collect();

        let mut max_frame = 0u64;
        let mut min_frame = u64::MAX;
        let mut frames_seen = std::collections::HashSet::new();

        for line in &lines {
            if let Ok(qr_data) = serde_json::from_str::<QrCodeData>(line) {
                frames_seen.insert(qr_data.frame_number);
                if qr_data.frame_number > max_frame {
                    max_frame = qr_data.frame_number;
                }
                if qr_data.frame_number < min_frame {
                    min_frame = qr_data.frame_number;
                }
            }
        }

        if min_frame == u64::MAX {
            min_frame = 0;
        }

        Ok((frames_seen.len() as u64, lines.len(), max_frame, min_frame))
    }

    fn determine_completion(&self, chunk_id: usize, expected_frames: u64, actual_frames: u64,
                           max_frame: u64, min_frame: u64, qr_codes: usize, expected_duration: f64) -> (bool, String) {

        // Criterion 1: Frame count completeness
        let frame_completeness = actual_frames as f64 / expected_frames as f64;

        // Criterion 2: Frame range coverage (should span the full chunk duration)
        let expected_max_frame = if self.skip_frames > 0 {
            expected_frames / (self.skip_frames as u64 + 1)
        } else {
            expected_frames
        };

        let frame_range_coverage = if expected_max_frame > 0 {
            max_frame as f64 / expected_max_frame as f64
        } else {
            0.0
        };

        // Criterion 3: QR code density (expect reasonable number of QR codes)
        let expected_min_qr_codes = if chunk_id == self.chunk_count - 1 { 200 } else { 300 }; // Last chunk might be shorter
        let has_sufficient_qr_codes = qr_codes >= expected_min_qr_codes;

        // Criterion 4: Frame sequence continuity
        let frame_span = max_frame.saturating_sub(min_frame);
        let has_good_frame_span = frame_span >= (expected_max_frame * 80 / 100); // At least 80% span

        // Completion decision logic
        if frame_completeness >= 0.95 && frame_range_coverage >= 0.90 && has_sufficient_qr_codes && has_good_frame_span {
            (true, format!("COMPLETE: {:.1}% frames, {:.1}% range, {} QR codes",
                          frame_completeness * 100.0, frame_range_coverage * 100.0, qr_codes))
        } else if frame_completeness >= 0.80 && has_sufficient_qr_codes {
            (true, format!("ADEQUATE: {:.1}% frames, {} QR codes (sufficient for processing)",
                          frame_completeness * 100.0, qr_codes))
        } else {
            let issues = vec![
                if frame_completeness < 0.80 { Some(format!("low frame coverage {:.1}%", frame_completeness * 100.0)) } else { None },
                if frame_range_coverage < 0.80 { Some(format!("poor range coverage {:.1}%", frame_range_coverage * 100.0)) } else { None },
                if !has_sufficient_qr_codes { Some(format!("insufficient QR codes {}", qr_codes)) } else { None },
                if !has_good_frame_span { Some(format!("poor frame span {}", frame_span)) } else { None },
            ].into_iter().flatten().collect::<Vec<_>>();

            (false, format!("INCOMPLETE: {}", issues.join(", ")))
        }
    }

    pub fn analyze_all_chunks(&self, output_dir: &PathBuf) -> Result<Vec<ChunkCompletionInfo>> {
        let mut results = Vec::new();

        for i in 0..self.chunk_count {
            let chunk_info = self.analyze_chunk_completion(i, output_dir)?;
            results.push(chunk_info);
        }

        self.logger.log_info(&format!("Completion Analysis Summary:"));
        let complete_count = results.iter().filter(|c| c.is_complete).count();
        self.logger.log_info(&format!("  Complete chunks: {}/{}", complete_count, self.chunk_count));

        for chunk_info in &results {
            self.logger.log_info(&format!("  Chunk {}: {:.1}% - {} frames, {} QR codes - {}",
                                        chunk_info.chunk_id + 1,
                                        chunk_info.completion_percentage,
                                        chunk_info.actual_frames_processed,
                                        chunk_info.qr_codes_found,
                                        chunk_info.completion_reason));
        }

        Ok(results)
    }

    pub fn get_incomplete_chunks(&self, output_dir: &PathBuf) -> Result<Vec<usize>> {
        let analysis = self.analyze_all_chunks(output_dir)?;
        Ok(analysis.iter()
            .filter(|chunk| !chunk.is_complete)
            .map(|chunk| chunk.chunk_id)
            .collect())
    }

    pub fn verify_processing_completeness(&self, output_dir: &PathBuf) -> Result<(bool, String)> {
        let analysis = self.analyze_all_chunks(output_dir)?;

        let complete_chunks = analysis.iter().filter(|c| c.is_complete).count();
        let total_qr_codes: usize = analysis.iter().map(|c| c.qr_codes_found).sum();
        let total_frames_processed: u64 = analysis.iter().map(|c| c.actual_frames_processed).sum();

        let overall_completeness = complete_chunks as f64 / self.chunk_count as f64 * 100.0;

        if complete_chunks == self.chunk_count {
            Ok((true, format!("ALL COMPLETE: {} chunks, {} QR codes, {} frames processed",
                             complete_chunks, total_qr_codes, total_frames_processed)))
        } else if complete_chunks >= (self.chunk_count * 80 / 100) { // 80% threshold
            Ok((true, format!("MOSTLY COMPLETE: {}/{} chunks ({:.1}%), {} QR codes - sufficient for processing",
                             complete_chunks, self.chunk_count, overall_completeness, total_qr_codes)))
        } else {
            let incomplete: Vec<usize> = analysis.iter()
                .filter(|c| !c.is_complete)
                .map(|c| c.chunk_id + 1)
                .collect();

            Ok((false, format!("INCOMPLETE: {}/{} chunks complete, missing chunks: {:?}",
                              complete_chunks, self.chunk_count, incomplete)))
        }
    }

    // Real-time progress validation during processing
    pub fn validate_chunk_progress(&self, chunk_id: usize, current_frame: u64, qr_codes: usize) -> (f64, bool, String) {
        let expected_frames = self.calculate_expected_frames(chunk_id);
        let expected_min_qr = if chunk_id == self.chunk_count - 1 { 200 } else { 300 };

        let frame_progress = (current_frame as f64 / expected_frames as f64 * 100.0).min(100.0);
        let qr_rate = qr_codes as f64 / current_frame.max(1) as f64;

        let is_on_track = frame_progress >= 10.0 && qr_codes >= (expected_min_qr * current_frame as usize / expected_frames as usize).max(10);

        let status = if is_on_track {
            format!("ON_TRACK: {:.1}% frames, {} QR codes, {:.3} QR/frame", frame_progress, qr_codes, qr_rate)
        } else {
            format!("SLOW: {:.1}% frames, {} QR codes, {:.3} QR/frame", frame_progress, qr_codes, qr_rate)
        };

        (frame_progress, is_on_track, status)
    }

    /// Get resume point for specific chunk based on JSONL content
    pub fn get_chunk_resume_point(&self, chunk_id: usize, output_dir: &PathBuf) -> Result<ChunkResumePoint> {
        let jsonl_file = output_dir.join(format!("chunk_{:03}.jsonl", chunk_id + 1));
        let expected_frames = self.calculate_expected_frames(chunk_id);

        if !jsonl_file.exists() {
            return Ok(ChunkResumePoint {
                chunk_id,
                should_resume: true,
                resume_from_frame: 0,
                frames_already_processed: 0,
                qr_codes_already_found: 0,
                completion_status: "No JSONL file - start from beginning".to_string(),
            });
        }

        let (actual_frames, qr_codes, max_frame, min_frame) = self.analyze_jsonl_content(&jsonl_file)?;
        let (is_complete, reason) = self.determine_completion(
            chunk_id, expected_frames, actual_frames, max_frame, min_frame, qr_codes, 0.0
        );

        if is_complete {
            return Ok(ChunkResumePoint {
                chunk_id,
                should_resume: false,
                resume_from_frame: max_frame + 1,
                frames_already_processed: actual_frames,
                qr_codes_already_found: qr_codes,
                completion_status: format!("COMPLETE: {}", reason),
            });
        }

        // Calculate resume point - continue from last processed frame + 1
        let resume_frame = max_frame + 1;

        // Skip frames adjustment - if skip_frames=1, resume from next skip-aligned frame
        let aligned_resume_frame = if self.skip_frames > 0 {
            let skip_interval = self.skip_frames as u64 + 1;
            ((resume_frame + skip_interval - 1) / skip_interval) * skip_interval
        } else {
            resume_frame
        };

        self.logger.log_info(&format!("Chunk {}: Resume from frame {} (was at frame {}, {} QR codes)",
                                    chunk_id + 1, aligned_resume_frame, max_frame, qr_codes));

        Ok(ChunkResumePoint {
            chunk_id,
            should_resume: true,
            resume_from_frame: aligned_resume_frame,
            frames_already_processed: actual_frames,
            qr_codes_already_found: qr_codes,
            completion_status: format!("RESUME: from frame {} ({:.1}% complete)",
                                     aligned_resume_frame,
                                     actual_frames as f64 / expected_frames as f64 * 100.0),
        })
    }

    /// Get resume information for all chunks
    pub fn get_all_resume_points(&self, output_dir: &PathBuf) -> Result<Vec<ChunkResumePoint>> {
        let mut resume_points = Vec::new();

        for i in 0..self.chunk_count {
            let resume_point = self.get_chunk_resume_point(i, output_dir)?;
            resume_points.push(resume_point);
        }

        let incomplete_count = resume_points.iter().filter(|p| p.should_resume).count();
        self.logger.log_info(&format!("Resume Analysis: {} chunks need processing, {} already complete",
                                    incomplete_count, self.chunk_count - incomplete_count));

        Ok(resume_points)
    }
}

#[derive(Debug, Clone)]
pub struct ChunkResumePoint {
    pub chunk_id: usize,
    pub should_resume: bool,
    pub resume_from_frame: u64,
    pub frames_already_processed: u64,
    pub qr_codes_already_found: usize,
    pub completion_status: String,
}