use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use chrono::Utc;

pub struct ErrorLogger {
    log_file: Mutex<std::fs::File>,
}

impl ErrorLogger {
    pub fn new(log_path: &str) -> Result<Self, std::io::Error> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;

        let logger = Self {
            log_file: Mutex::new(file),
        };

        // Write session header
        logger.log_info("=== NEW SESSION STARTED ===");

        Ok(logger)
    }

    pub fn log_error(&self, context: &str, error: &str) {
        self.write_log("ERROR", context, error);
    }

    pub fn log_warning(&self, context: &str, message: &str) {
        self.write_log("WARN", context, message);
    }

    pub fn log_info(&self, message: &str) {
        self.write_log("INFO", "SYSTEM", message);
    }

    pub fn log_debug(&self, context: &str, details: &str) {
        self.write_log("DEBUG", context, details);
    }

    fn write_log(&self, level: &str, context: &str, message: &str) {
        if let Ok(mut file) = self.log_file.lock() {
            let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let log_line = format!("[{}] {} [{}]: {}\n", timestamp, level, context, message);
            let _ = file.write_all(log_line.as_bytes());
            let _ = file.flush();
        }
    }

    pub fn log_qr_data(&self, chunk_id: usize, qr_data: &str) {
        let preview = if qr_data.len() > 100 {
            format!("{}... (length: {})", &qr_data[..100], qr_data.len())
        } else {
            qr_data.to_string()
        };
        self.write_log("QR_DATA", &format!("CHUNK_{}", chunk_id), &preview);
    }

    pub fn log_base64_error(&self, chunk_id: usize, data: &str, error: &str) {
        let preview = if data.len() > 50 {
            format!("{}... (length: {})", &data[..50], data.len())
        } else {
            data.to_string()
        };
        self.write_log("BASE64_ERROR", &format!("CHUNK_{}", chunk_id),
                      &format!("Error: {} | Data: {}", error, preview));
    }

    pub fn log_processing_phase(&self, phase: &str, details: &str) {
        self.write_log("PHASE", phase, details);
    }
}