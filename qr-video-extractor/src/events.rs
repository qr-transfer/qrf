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
}

pub type EventCallback = Box<dyn Fn(ProcessingEvent) + Send + Sync>;

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