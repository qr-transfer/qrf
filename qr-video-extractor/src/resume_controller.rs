use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::fs;
use crate::resume_state::{ResumeState, ChunkState, ChunkProcessingStatus};
use crate::events::{EventCallback, ProcessingEvent};
use crate::error_logger::ErrorLogger;

pub struct ResumeController {
    state: ResumeState,
    output_dir: PathBuf,
    logger: ErrorLogger,
}

impl ResumeController {
    pub fn new(output_dir: &PathBuf, input_file: &str, chunk_count: usize, thread_count: usize, skip_frames: usize) -> Result<Self> {
        let log_path = output_dir.join("processing.log");
        let logger = ErrorLogger::new(&log_path.to_string_lossy())
            .unwrap_or_else(|_| ErrorLogger::new("/tmp/processing.log").unwrap());

        let state = ResumeState::load_or_create(output_dir, input_file, chunk_count, thread_count, skip_frames)?;

        logger.log_info(&format!("=== RESUME CONTROLLER INITIALIZED ==="));
        logger.log_info(&format!("Can resume from Phase: {}", state.can_resume_from_phase()));

        Ok(Self {
            state,
            output_dir: output_dir.clone(),
            logger,
        })
    }

    pub fn detect_resume_point(&mut self, callback: &EventCallback) -> Result<ResumePoint> {
        self.logger.log_info("Detecting resume point...");

        // Check Phase 3: All JSONL files complete
        if self.can_resume_phase_3() {
            callback(ProcessingEvent::InitializationProgress {
                stage: "Resume Detection".to_string(),
                message: "All chunks complete - resuming from Phase 3".to_string(),
            });
            return Ok(ResumePoint::Phase3);
        }

        // Check Phase 2: Video chunks exist, some JSONL incomplete
        if self.can_resume_phase_2() {
            let incomplete_chunks = self.get_incomplete_chunks();
            callback(ProcessingEvent::InitializationProgress {
                stage: "Resume Detection".to_string(),
                message: format!("Video chunks exist - resuming {} incomplete chunks in Phase 2", incomplete_chunks.len()),
            });
            return Ok(ResumePoint::Phase2(incomplete_chunks));
        }

        // Start from Phase 1
        callback(ProcessingEvent::InitializationProgress {
            stage: "Resume Detection".to_string(),
            message: "Starting fresh from Phase 1".to_string(),
        });
        Ok(ResumePoint::Phase1)
    }

    fn can_resume_phase_3(&mut self) -> bool {
        // Check if all JSONL files exist and have reasonable content
        for i in 1..=self.state.chunk_count {
            let jsonl_file = self.output_dir.join(format!("chunk_{:03}.jsonl", i));
            if !jsonl_file.exists() {
                self.logger.log_info(&format!("JSONL file missing: chunk_{:03}.jsonl", i));
                return false;
            }

            // Check if JSONL has reasonable content (not empty or corrupted)
            if let Ok(content) = fs::read_to_string(&jsonl_file) {
                let line_count = content.lines().count();
                if line_count < 10 {  // Expect at least 10 QR codes
                    self.logger.log_warning("INCOMPLETE_JSONL", &format!("chunk_{:03}.jsonl has only {} lines", i, line_count));
                    return false;
                }

                // Update state with discovered content
                self.state.update_chunk_progress(i - 1, line_count as u64, line_count, ChunkProcessingStatus::Completed);
            } else {
                return false;
            }
        }

        self.logger.log_info("All JSONL files exist and have content - can resume from Phase 3");
        true
    }

    fn can_resume_phase_2(&mut self) -> bool {
        // Check if video chunks exist
        for i in 1..=self.state.chunk_count {
            let chunk_file = self.output_dir.join(format!("chunk_{:03}.mp4", i));
            if !chunk_file.exists() {
                self.logger.log_info(&format!("Video chunk missing: chunk_{:03}.mp4", i));
                return false;
            }
        }

        self.logger.log_info("All video chunks exist - can resume from Phase 2");
        true
    }

    fn get_incomplete_chunks(&mut self) -> Vec<ChunkResumeInfo> {
        let mut incomplete = Vec::new();

        for i in 0..self.state.chunk_count {
            let jsonl_file = self.output_dir.join(format!("chunk_{:03}.jsonl", i + 1));

            let (is_complete, last_frame, qr_count) = if jsonl_file.exists() {
                self.analyze_jsonl_completeness(&jsonl_file, i)
            } else {
                (false, 0, 0)
            };

            if !is_complete {
                incomplete.push(ChunkResumeInfo {
                    chunk_id: i,
                    last_frame_processed: last_frame,
                    qr_codes_found: qr_count,
                    needs_full_reprocess: last_frame == 0,
                });

                self.state.update_chunk_progress(i, last_frame, qr_count,
                    if last_frame > 0 { ChunkProcessingStatus::Interrupted } else { ChunkProcessingStatus::NotStarted });
            } else {
                self.state.update_chunk_progress(i, last_frame, qr_count, ChunkProcessingStatus::Completed);
            }
        }

        self.logger.log_info(&format!("Found {} incomplete chunks for resume", incomplete.len()));

        incomplete
    }

    fn analyze_jsonl_completeness(&self, jsonl_path: &PathBuf, chunk_id: usize) -> (bool, u64, usize) {
        if let Ok(content) = fs::read_to_string(jsonl_path) {
            let lines: Vec<&str> = content.lines().collect();
            let qr_count = lines.len();

            if qr_count == 0 {
                return (false, 0, 0);
            }

            // Find the highest frame number processed
            let mut max_frame = 0u64;
            for line in lines {
                if let Ok(qr_data) = serde_json::from_str::<crate::qr_extraction::QrCodeData>(line) {
                    if qr_data.frame_number > max_frame {
                        max_frame = qr_data.frame_number;
                    }
                }
            }

            // Heuristic for completeness - expect 500+ QR codes per chunk for full processing
            let is_complete = qr_count >= 500 && max_frame > 1000;

            self.logger.log_info(&format!("Chunk {}: {} QR codes, max frame {}, complete: {}",
                                 chunk_id + 1, qr_count, max_frame, is_complete));

            (is_complete, max_frame, qr_count)
        } else {
            (false, 0, 0)
        }
    }

    pub fn update_and_save(&mut self, chunk_id: usize, frame: u64, qr_codes: usize, status: ChunkProcessingStatus) -> Result<()> {
        self.state.update_chunk_progress(chunk_id, frame, qr_codes, status);
        self.state.save(&self.output_dir)?;
        Ok(())
    }

    pub fn mark_phase_completed(&mut self, phase: u8) -> Result<()> {
        self.state.phase_completed = phase;
        self.state.save(&self.output_dir)?;
        self.logger.log_info(&format!("Phase {} marked as completed in resume state", phase));
        Ok(())
    }

    pub fn handle_error(&mut self, chunk_id: Option<usize>, context: &str, error: &str, callback: &EventCallback) {
        self.logger.log_error(context, error);

        if let Some(id) = chunk_id {
            self.state.mark_chunk_error(id, error.to_string());
            let _ = self.state.save(&self.output_dir);
        }

        callback(ProcessingEvent::SystemError {
            context: context.to_string(),
            error: error.to_string(),
        });
    }

    pub fn handle_interruption(&mut self, callback: &EventCallback) -> Result<()> {
        self.logger.log_warning("INTERRUPTION", "Process interrupted - saving state for resume");

        // Mark all processing chunks as interrupted
        for chunk in self.state.chunks.values_mut() {
            if chunk.status == ChunkProcessingStatus::Processing {
                chunk.status = ChunkProcessingStatus::Interrupted;
            }
        }

        self.state.save(&self.output_dir)?;

        callback(ProcessingEvent::SystemError {
            context: "Process Interruption".to_string(),
            error: "Processing interrupted - state saved for resume".to_string(),
        });

        Ok(())
    }

    pub fn get_resume_summary(&self) -> String {
        let completed = self.state.chunks.values().filter(|c| c.status == ChunkProcessingStatus::Completed).count();
        let processing = self.state.chunks.values().filter(|c| c.status == ChunkProcessingStatus::Processing).count();
        let failed = self.state.chunks.values().filter(|c| c.status == ChunkProcessingStatus::Failed).count();
        let interrupted = self.state.chunks.values().filter(|c| c.status == ChunkProcessingStatus::Interrupted).count();

        format!("Resume Status: {}/{} completed, {} processing, {} failed, {} interrupted",
                completed, self.state.chunk_count, processing, failed, interrupted)
    }
}

#[derive(Debug)]
pub enum ResumePoint {
    Phase1,                                    // Start fresh
    Phase2(Vec<ChunkResumeInfo>),             // Resume specific chunks
    Phase3,                                   // Skip to file reconstruction
}

#[derive(Debug, Clone)]
pub struct ChunkResumeInfo {
    pub chunk_id: usize,
    pub last_frame_processed: u64,
    pub qr_codes_found: usize,
    pub needs_full_reprocess: bool,
}