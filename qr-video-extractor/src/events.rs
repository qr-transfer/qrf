use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessingEvent {
    PhaseStarted {
        phase: u8,
        description: String,
    },
    Progress {
        phase: u8,
        current: usize,
        total: usize,
        message: String,
    },
    PhaseCompleted {
        phase: u8,
        duration_ms: u64,
    },
    Error {
        phase: u8,
        error: String,
    },
    AllCompleted {
        total_duration_ms: u64,
        files_extracted: usize,
    },
    ChunkStarted {
        chunk_id: usize,
        chunk_name: String,
    },
    ChunkProgress {
        chunk_id: usize,
        frames_processed: usize,
        qr_codes_found: usize,
        status: String,
    },
    ChunkCompleted {
        chunk_id: usize,
        qr_codes_found: usize,
        jsonl_file: String,
        duration_ms: u64,
    },
    FileReconstructed {
        file_name: String,
        file_size: u64,
        checksum_valid: bool,
        output_path: String,
    },
    ChecksumValidation {
        file_name: String,
        checksum_type: String,
        expected: String,
        actual: String,
        valid: bool,
    },
    // System events for UI/output management
    SystemError {
        context: String,
        error: String,
    },
    InitializationProgress {
        stage: String,
        message: String,
    },
    FinalSummary {
        files_count: usize,
        output_dir: String,
        total_duration_ms: u64,
    },
    ModeTransition {
        from: String,
        to: String,
        reason: String,
    },
    // Frame-level progress tracking
    FrameProgress {
        chunk_id: usize,
        frames_processed: u64,
        total_frames: u64,
        qr_codes_found: usize,
    },
}

pub type EventCallback = Box<dyn Fn(ProcessingEvent) + Send + Sync>;

pub trait OutputHandler {
    fn handle_event(&self, event: &ProcessingEvent);
}

pub struct ConsoleOutputHandler;

impl OutputHandler for ConsoleOutputHandler {
    fn handle_event(&self, event: &ProcessingEvent) {
        match event {
            ProcessingEvent::PhaseStarted { phase, description } => {
                println!("Phase {}: {}", phase, description);
            }
            ProcessingEvent::Progress { phase, current, total, message } => {
                println!("Phase {} [{}/{}]: {}", phase, current, total, message);
            }
            ProcessingEvent::PhaseCompleted { phase, duration_ms } => {
                println!("Phase {} completed in {}ms", phase, duration_ms);
            }
            ProcessingEvent::Error { phase, error } => {
                eprintln!("Phase {} error: {}", phase, error);
            }
            ProcessingEvent::AllCompleted { total_duration_ms, files_extracted } => {
                println!("🎉 All processing completed! Extracted {} files in {}ms", files_extracted, total_duration_ms);
            }
            ProcessingEvent::ChunkStarted { chunk_id, chunk_name } => {
                println!("▶️  Started chunk {}: {}", chunk_id + 1, chunk_name);
            }
            ProcessingEvent::ChunkProgress { chunk_id, frames_processed, qr_codes_found, status } => {
                println!("⏳ Chunk {}: {} - {} frames, {} QR codes", chunk_id + 1, status, frames_processed, qr_codes_found);
            }
            ProcessingEvent::ChunkCompleted { chunk_id, qr_codes_found, jsonl_file, duration_ms } => {
                println!("✅ Chunk {} completed: {} QR codes → {} ({}ms)", chunk_id + 1, qr_codes_found, jsonl_file, duration_ms);
            }
            ProcessingEvent::FileReconstructed { file_name, file_size, checksum_valid, output_path } => {
                let status = if *checksum_valid { "✅" } else { "⚠️" };
                println!("{} File reconstructed: {} ({} bytes) → {}", status, file_name, file_size, output_path);
            }
            ProcessingEvent::ChecksumValidation { file_name, checksum_type, expected, actual, valid } => {
                let status = if *valid { "✅" } else { "❌" };
                println!("{} {}: {} (expected: {}, actual: {})", status, checksum_type, file_name, expected, actual);
            }
            ProcessingEvent::SystemError { context, error } => {
                eprintln!("Error in {}: {}", context, error);
            }
            ProcessingEvent::InitializationProgress { stage, message } => {
                println!("{}: {}", stage, message);
            }
            ProcessingEvent::FinalSummary { files_count, output_dir, total_duration_ms } => {
                println!("\nProcessing completed successfully!");
                println!("Files extracted: {}", files_count);
                println!("Output directory: {}", output_dir);
                println!("Total duration: {}ms", total_duration_ms);
            }
            ProcessingEvent::ModeTransition { from, to, reason } => {
                eprintln!("{} ({}), switching from {} to {} mode...", reason, reason, from, to);
            }
            ProcessingEvent::FrameProgress { chunk_id, frames_processed, total_frames, qr_codes_found } => {
                let progress = (*frames_processed as f64 / *total_frames as f64 * 100.0).min(100.0);
                println!("Chunk {}: Frame {}/{} ({:.1}%) - {} QR codes", chunk_id + 1, frames_processed, total_frames, progress, qr_codes_found);
            }
        }
    }
}

pub struct EventBus {
    callbacks: Vec<EventCallback>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    pub fn subscribe(&mut self, callback: EventCallback) {
        self.callbacks.push(callback);
    }

    pub fn emit(&self, event: ProcessingEvent) {
        for callback in &self.callbacks {
            callback(event.clone());
        }
    }
}