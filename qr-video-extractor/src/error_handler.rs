use anyhow::{anyhow, Result};
use crate::events::{EventCallback, ProcessingEvent};
use crate::error_logger::ErrorLogger;
use std::sync::Arc;
use std::path::PathBuf;

pub struct ErrorHandler {
    logger: Arc<ErrorLogger>,
    callback: Option<Arc<EventCallback>>,
}

impl ErrorHandler {
    pub fn new(output_dir: &PathBuf) -> Result<Self, std::io::Error> {
        let log_path = output_dir.join("processing.log");
        let logger = Arc::new(ErrorLogger::new(&log_path.to_string_lossy())?);

        Ok(Self {
            logger,
            callback: None,
        })
    }

    pub fn set_callback(&mut self, callback: Arc<EventCallback>) {
        self.callback = Some(callback);
    }

    pub fn handle_ffmpeg_error(&self, chunk_id: usize, operation: &str, error: &str) {
        let context = format!("FFmpeg Error - Chunk {} - {}", chunk_id + 1, operation);
        self.logger.log_error(&context, error);

        if let Some(ref cb) = self.callback {
            cb(ProcessingEvent::SystemError {
                context,
                error: error.to_string(),
            });
        }
    }

    pub fn handle_memory_error(&self, chunk_id: Option<usize>, operation: &str, error: &str) {
        let context = if let Some(id) = chunk_id {
            format!("Memory Error - Chunk {} - {}", id + 1, operation)
        } else {
            format!("Memory Error - {}", operation)
        };

        self.logger.log_error(&context, error);

        if let Some(ref cb) = self.callback {
            cb(ProcessingEvent::SystemError {
                context,
                error: error.to_string(),
            });
        }
    }

    pub fn handle_io_error(&self, operation: &str, file_path: &str, error: &str) {
        let context = format!("I/O Error - {} - {}", operation, file_path);
        self.logger.log_error(&context, error);

        if let Some(ref cb) = self.callback {
            cb(ProcessingEvent::SystemError {
                context,
                error: error.to_string(),
            });
        }
    }

    pub fn handle_qr_processing_error(&self, chunk_id: usize, frame_number: u64, error: &str) {
        let context = format!("QR Processing Error - Chunk {} - Frame {}", chunk_id + 1, frame_number);
        self.logger.log_error(&context, error);

        if let Some(ref cb) = self.callback {
            cb(ProcessingEvent::SystemError {
                context,
                error: error.to_string(),
            });
        }
    }

    pub fn handle_thread_error(&self, thread_name: &str, error: &str) {
        let context = format!("Thread Error - {}", thread_name);
        self.logger.log_error(&context, error);

        if let Some(ref cb) = self.callback {
            cb(ProcessingEvent::SystemError {
                context,
                error: error.to_string(),
            });
        }
    }

    pub fn handle_timeout(&self, chunk_id: usize, timeout_secs: u64) {
        let context = format!("Timeout - Chunk {}", chunk_id + 1);
        let error = format!("Processing timeout after {} seconds", timeout_secs);
        self.logger.log_warning("TIMEOUT", &error);

        if let Some(ref cb) = self.callback {
            cb(ProcessingEvent::SystemError {
                context,
                error,
            });
        }
    }

    pub fn handle_resource_exhaustion(&self, resource: &str, details: &str) {
        let context = format!("Resource Exhaustion - {}", resource);
        self.logger.log_error(&context, details);

        if let Some(ref cb) = self.callback {
            cb(ProcessingEvent::SystemError {
                context,
                error: details.to_string(),
            });
        }
    }

    pub fn log_progress(&self, message: &str) {
        self.logger.log_info(message);
    }

    pub fn log_debug(&self, context: &str, message: &str) {
        self.logger.log_debug(context, message);
    }
}

// Console output capture system
pub struct ConsoleOutputCapture;

impl ConsoleOutputCapture {
    pub fn audit_console_output() -> Vec<String> {
        // This would scan the codebase for problematic console output
        vec![
            "src/qr_extraction.rs:791 - println! in streaming function".to_string(),
            "src/events.rs:96-144 - ConsoleOutputHandler prints in TUI mode".to_string(),
            "src/main.rs:357-375 - Demo mode prints".to_string(),
        ]
    }

    pub fn check_for_thread_output_leaks() -> Vec<String> {
        // Detect thread output that might corrupt TUI
        vec![
            "Background thread errors not routed through events".to_string(),
            "FFmpeg stderr output not captured".to_string(),
            "Panic handlers not redirected".to_string(),
        ]
    }
}

// Interruption handling
pub fn setup_signal_handlers(error_handler: Arc<ErrorHandler>) -> Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc as StdArc;

    static INTERRUPTED: AtomicBool = AtomicBool::new(false);

    ctrlc::set_handler(move || {
        INTERRUPTED.store(true, Ordering::SeqCst);
        error_handler.log_progress("Interrupt signal received - initiating graceful shutdown");
    }).map_err(|e| anyhow!("Failed to set interrupt handler: {}", e))?;

    Ok(())
}

// Memory monitoring
pub fn check_memory_usage() -> Result<(u64, f64)> {
    // Get current memory usage in bytes and percentage
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        let output = Command::new("ps")
            .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()?;

        if let Ok(rss_str) = String::from_utf8(output.stdout) {
            if let Ok(rss_kb) = rss_str.trim().parse::<u64>() {
                let bytes = rss_kb * 1024;
                let percentage = (bytes as f64 / (8.0 * 1024.0 * 1024.0 * 1024.0)) * 100.0; // Assume 8GB total
                return Ok((bytes, percentage));
            }
        }
    }

    Ok((0, 0.0))
}

// Disk space monitoring
pub fn check_disk_space(output_dir: &PathBuf) -> Result<(u64, f64)> {
    use std::fs;

    let metadata = fs::metadata(output_dir)?;
    // This is a simplified version - would need platform-specific disk space checking
    Ok((0, 0.0)) // Placeholder
}