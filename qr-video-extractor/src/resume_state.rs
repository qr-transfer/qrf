use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeState {
    pub version: String,
    pub input_file: String,
    pub output_dir: String,
    pub chunk_count: usize,
    pub thread_count: usize,
    pub skip_frames: usize,
    pub phase_completed: u8,
    pub chunks: HashMap<usize, ChunkState>,
    pub total_frames: u64,
    pub start_time: Option<u64>, // Unix timestamp
    pub last_update: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkState {
    pub chunk_id: usize,
    pub video_file: String,
    pub jsonl_file: String,
    pub status: ChunkProcessingStatus,
    pub last_frame_processed: u64,
    pub qr_codes_found: usize,
    pub processing_time_ms: u64,
    pub error_count: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChunkProcessingStatus {
    NotStarted,
    Processing,
    Completed,
    Failed,
    Interrupted,
}

impl ResumeState {
    pub fn new(input_file: &str, output_dir: &str, chunk_count: usize, thread_count: usize, skip_frames: usize) -> Self {
        Self {
            version: "0.1.0".to_string(),
            input_file: input_file.to_string(),
            output_dir: output_dir.to_string(),
            chunk_count,
            thread_count,
            skip_frames,
            phase_completed: 0,
            chunks: HashMap::new(),
            total_frames: 0,
            start_time: Some(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_secs()),
            last_update: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_secs(),
        }
    }

    pub fn load_or_create(output_dir: &PathBuf, input_file: &str, chunk_count: usize, thread_count: usize, skip_frames: usize) -> Result<Self> {
        let state_file = output_dir.join("resume_state.json");

        if state_file.exists() {
            let content = fs::read_to_string(&state_file)?;
            let mut state: ResumeState = serde_json::from_str(&content)?;

            // Validate state compatibility
            if state.input_file != input_file || state.chunk_count != chunk_count {
                // Parameters changed - start fresh
                return Ok(Self::new(input_file, &output_dir.to_string_lossy(), chunk_count, thread_count, skip_frames));
            }

            // Update last_update timestamp
            state.last_update = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap().as_secs();

            Ok(state)
        } else {
            Ok(Self::new(input_file, &output_dir.to_string_lossy(), chunk_count, thread_count, skip_frames))
        }
    }

    pub fn save(&self, output_dir: &PathBuf) -> Result<()> {
        let state_file = output_dir.join("resume_state.json");
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&state_file, content)?;
        Ok(())
    }

    pub fn can_resume_from_phase(&self) -> u8 {
        // Detect which phase we can resume from

        // Check if Phase 3 can be resumed (all JSONLs exist)
        if self.all_chunks_completed() {
            return 3;
        }

        // Check if Phase 2 can be resumed (video chunks exist)
        if self.video_chunks_exist() {
            return 2;
        }

        // Start from Phase 1
        1
    }

    pub fn video_chunks_exist(&self) -> bool {
        let output_dir = PathBuf::from(&self.output_dir);
        for i in 1..=self.chunk_count {
            let chunk_file = output_dir.join(format!("chunk_{:03}.mp4", i));
            if !chunk_file.exists() {
                return false;
            }
        }
        true
    }

    pub fn all_chunks_completed(&self) -> bool {
        if self.chunks.len() != self.chunk_count {
            return false;
        }

        for chunk in self.chunks.values() {
            if chunk.status != ChunkProcessingStatus::Completed {
                return false;
            }
        }
        true
    }

    pub fn get_incomplete_chunks(&self) -> Vec<usize> {
        let output_dir = PathBuf::from(&self.output_dir);
        let mut incomplete = Vec::new();

        for i in 0..self.chunk_count {
            if let Some(chunk) = self.chunks.get(&i) {
                if chunk.status != ChunkProcessingStatus::Completed {
                    incomplete.push(i);
                }
            } else {
                // Chunk not tracked yet - check if JSONL exists and is complete
                let jsonl_file = output_dir.join(format!("chunk_{:03}.jsonl", i + 1));
                if !jsonl_file.exists() || self.is_jsonl_incomplete(&jsonl_file, i).unwrap_or(true) {
                    incomplete.push(i);
                }
            }
        }

        incomplete
    }

    fn is_jsonl_incomplete(&self, jsonl_path: &PathBuf, chunk_id: usize) -> Result<bool> {
        if !jsonl_path.exists() {
            return Ok(true);
        }

        let content = fs::read_to_string(jsonl_path)?;
        let line_count = content.lines().count();

        // Heuristic: if very few QR codes, might be incomplete
        // You could add more sophisticated checks here
        Ok(line_count < 100) // Expect at least 100 QR codes per chunk for completeness
    }

    pub fn update_chunk_progress(&mut self, chunk_id: usize, frame: u64, qr_codes: usize, status: ChunkProcessingStatus) {
        let chunk_state = self.chunks.entry(chunk_id).or_insert_with(|| ChunkState {
            chunk_id,
            video_file: format!("chunk_{:03}.mp4", chunk_id + 1),
            jsonl_file: format!("chunk_{:03}.jsonl", chunk_id + 1),
            status: ChunkProcessingStatus::NotStarted,
            last_frame_processed: 0,
            qr_codes_found: 0,
            processing_time_ms: 0,
            error_count: 0,
            last_error: None,
        });

        chunk_state.last_frame_processed = frame;
        chunk_state.qr_codes_found = qr_codes;
        chunk_state.status = status;

        self.last_update = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_secs();
    }

    pub fn mark_chunk_error(&mut self, chunk_id: usize, error: String) {
        if let Some(chunk) = self.chunks.get_mut(&chunk_id) {
            chunk.error_count += 1;
            chunk.last_error = Some(error);
            chunk.status = ChunkProcessingStatus::Failed;
        }
    }

    pub fn get_progress_summary(&self) -> String {
        let completed = self.chunks.values().filter(|c| c.status == ChunkProcessingStatus::Completed).count();
        let processing = self.chunks.values().filter(|c| c.status == ChunkProcessingStatus::Processing).count();
        let failed = self.chunks.values().filter(|c| c.status == ChunkProcessingStatus::Failed).count();
        let total_qr = self.chunks.values().map(|c| c.qr_codes_found).sum::<usize>();

        format!("Chunks: {}/{} completed, {} processing, {} failed | QR codes: {}",
                completed, self.chunk_count, processing, failed, total_qr)
    }

    pub fn can_resume_chunk(&self, chunk_id: usize) -> (bool, u64) {
        if let Some(chunk) = self.chunks.get(&chunk_id) {
            match chunk.status {
                ChunkProcessingStatus::Completed => (false, 0),
                ChunkProcessingStatus::Processing | ChunkProcessingStatus::Interrupted => {
                    (true, chunk.last_frame_processed)
                }
                ChunkProcessingStatus::Failed => {
                    // Can retry failed chunks
                    (true, 0)
                }
                ChunkProcessingStatus::NotStarted => (true, 0),
            }
        } else {
            (true, 0)
        }
    }
}