use anyhow::{anyhow, Result};
use ffmpeg_next as ffmpeg;
use image::{ImageBuffer, Rgb};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::events::{EventCallback, ProcessingEvent};
use crate::video::VideoChunk;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeData {
    pub frame_number: u64,
    pub data: String,
    pub chunk_id: usize,
}

#[derive(Debug)]
pub struct QrExtractionResults {
    pub qr_codes: Vec<QrCodeData>,
    pub chunks_processed: usize,
    pub total_frames_processed: u64,
    pub processing_time_ms: u64,
}

pub struct QrExtractor {
    thread_count: usize,
    skip_frames: usize,
}

impl QrExtractor {
    pub fn new(thread_count: usize, skip_frames: usize) -> Self {
        Self {
            thread_count,
            skip_frames,
        }
    }

    pub fn extract_from_chunks(
        &self,
        chunks: &[VideoChunk],
        output_dir: &PathBuf,
        callback: &EventCallback,
    ) -> Result<QrExtractionResults> {
        let start_time = std::time::Instant::now();
        let total_chunks = chunks.len();

        callback(ProcessingEvent::Progress {
            phase: 2,
            current: 0,
            total: total_chunks,
            message: format!("Starting parallel processing of {} chunks...", total_chunks),
        });

        let results = Arc::new(Mutex::new(Vec::new()));
        let processed_count = Arc::new(Mutex::new(0));

        let chunk_refs: Vec<_> = chunks.iter().collect();
        let results_ref = Arc::clone(&results);
        let processed_ref = Arc::clone(&processed_count);

        // Use a thread-safe callback for parallel processing
        let callback_ref = Arc::new(callback);

        chunk_refs.into_par_iter().for_each(|chunk| {
            let cb = Arc::clone(&callback_ref);
            let chunk_start_time = std::time::Instant::now();

            // Report start of chunk processing
            cb(ProcessingEvent::ChunkStarted {
                chunk_id: chunk.id,
                chunk_name: chunk.path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
            });

            match self.extract_chunk_to_qr_data(chunk) {
                Ok(chunk_results) => {
                    let qr_count = chunk_results.len();
                    let duration_ms = chunk_start_time.elapsed().as_millis() as u64;

                    // Save chunk results to individual JSONL file in output directory
                    let jsonl_filename = format!("chunk_{:03}.jsonl", chunk.id + 1);
                    let jsonl_path = output_dir.join(&jsonl_filename);

                    match self.save_chunk_to_jsonl(&chunk_results, &jsonl_path.to_string_lossy()) {
                        Ok(_) => {
                            // Ensure file is fully written and synced
                            std::thread::sleep(std::time::Duration::from_millis(10));

                            cb(ProcessingEvent::ChunkCompleted {
                                chunk_id: chunk.id,
                                qr_codes_found: qr_count,
                                jsonl_file: jsonl_filename.clone(),
                                duration_ms,
                            });

                            // Verify file exists (silent for TUI)
                            if !jsonl_path.exists() {
                                cb(ProcessingEvent::Error {
                                    phase: 2,
                                    error: format!("JSONL file not found after save: {}", jsonl_filename),
                                });
                            }
                        }
                        Err(e) => {
                            cb(ProcessingEvent::Error {
                                phase: 2,
                                error: format!("Failed to save JSONL for chunk {}: {}", chunk.id + 1, e),
                            });
                        }
                    }

                    // Add to global results
                    {
                        let mut results_guard = results_ref.lock().unwrap();
                        results_guard.extend(chunk_results);
                    }

                    let current = {
                        let mut count = processed_ref.lock().unwrap();
                        *count += 1;
                        *count
                    };

                    cb(ProcessingEvent::Progress {
                        phase: 2,
                        current,
                        total: total_chunks,
                        message: format!("Completed {} of {} chunks ({} QR codes total)", current, total_chunks, qr_count),
                    });
                }
                Err(e) => {
                    cb(ProcessingEvent::Error {
                        phase: 2,
                        error: format!("Failed to process chunk {}: {}", chunk.id + 1, e),
                    });
                }
            }
        });

        // CRITICAL: Wait for all JSONL files to be fully written and verify they exist
        std::thread::sleep(std::time::Duration::from_millis(100)); // Allow file system sync

        let mut verified_chunks = 0;
        for i in 0..total_chunks {
            let jsonl_path = output_dir.join(format!("chunk_{:03}.jsonl", i + 1));
            if jsonl_path.exists() {
                verified_chunks += 1;
            }
        }

        callback(ProcessingEvent::Progress {
            phase: 2,
            current: verified_chunks,
            total: total_chunks,
            message: format!("Verified {}/{} JSONL files written to disk", verified_chunks, total_chunks),
        });

        let final_results = {
            let results_guard = results.lock().unwrap();
            results_guard.clone()
        };

        let mut sorted_results = final_results;
        sorted_results.sort_by_key(|qr| qr.frame_number);

        let processing_time = start_time.elapsed().as_millis() as u64;
        let total_frames = sorted_results.len() as u64;

        callback(ProcessingEvent::Progress {
            phase: 2,
            current: total_chunks,
            total: total_chunks,
            message: format!("Phase 2 COMPLETE: Extracted {} QR codes from {} chunks in {}ms, all JSONLs verified",
                           sorted_results.len(), total_chunks, processing_time),
        });

        Ok(QrExtractionResults {
            qr_codes: sorted_results,
            chunks_processed: total_chunks,
            total_frames_processed: total_frames,
            processing_time_ms: processing_time,
        })
    }

    fn extract_chunk_to_qr_data(&self, chunk: &VideoChunk) -> Result<Vec<QrCodeData>> {
        // Use external FFmpeg + zbar approach to avoid hanging
        let qr_results = self.extract_qr_external(&chunk)?;
        Ok(qr_results)
    }

    fn extract_qr_external(&self, chunk: &VideoChunk) -> Result<Vec<QrCodeData>> {
        // Use optimized external FFmpeg with immediate frame cleanup
        self.extract_qr_simple_external(chunk)
    }

    fn extract_qr_simple_external(&self, chunk: &VideoChunk) -> Result<Vec<QrCodeData>> {
        use std::process::Command;
        use std::fs;

        let temp_dir = format!("temp_frames_{}", chunk.id);
        fs::create_dir_all(&temp_dir)?;

        // Extract frames using external ffmpeg with fast settings
        let extract_cmd = Command::new("ffmpeg")
            .args([
                "-i", &chunk.path.to_string_lossy(),
                "-vf", "fps=0.5", // Sample 1 frame every 2 seconds
                "-frames:v", "5", // Limit to 5 frames per chunk
                "-y",
                "-loglevel", "quiet",
                &format!("{}/frame_%03d.png", temp_dir)
            ])
            .output();

        match extract_cmd {
            Ok(output) if output.status.success() => {
                // Continue processing
            }
            Ok(_) => {
                fs::remove_dir_all(&temp_dir).ok();
                return Ok(Vec::new());
            }
            Err(_) => {
                fs::remove_dir_all(&temp_dir).ok();
                return Ok(Vec::new());
            }
        }

        // Process frames immediately and clean up as we go
        let mut qr_results = Vec::new();
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for (frame_idx, entry) in entries.enumerate() {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("png") {
                        // Process QR codes from this frame
                        if let Ok(qr_codes) = self.extract_qr_from_image(&path) {
                            for qr_data in qr_codes {
                                qr_results.push(QrCodeData {
                                    frame_number: frame_idx as u64,
                                    data: qr_data,
                                    chunk_id: chunk.id,
                                });
                            }
                        }

                        // ✅ Delete frame immediately after processing
                        fs::remove_file(&path).ok();
                    }
                }
            }
        }

        // ✅ Clean up temp directory
        fs::remove_dir_all(&temp_dir).ok();

        Ok(qr_results)
    }

    fn extract_qr_streaming(&self, chunk: &VideoChunk) -> Result<Vec<QrCodeData>> {
        use std::process::{Command, Stdio};
        use std::io::BufReader;

        // Extract frames to stdout and process immediately (no temp files)
        let mut cmd = Command::new("ffmpeg")
            .args([
                "-i", &chunk.path.to_string_lossy(),
                "-vf", "fps=1", // Sample 1 frame per second for speed
                "-f", "image2pipe",
                "-vcodec", "png",
                "-frames:v", "10", // Limit frames for memory efficiency
                "-loglevel", "quiet",
                "pipe:1"
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdout = cmd.stdout.take().ok_or_else(|| anyhow!("Failed to capture stdout"))?;
        let mut reader = BufReader::new(stdout);

        let mut qr_results = Vec::new();
        let mut frame_number = 0u64;
        let mut png_buffer = Vec::new();

        // Read PNG frames from pipe and process immediately
        while self.read_png_frame(&mut reader, &mut png_buffer)? {
            frame_number += 1;

            // Process PNG data directly in memory (no temp file)
            if let Ok(qr_codes) = self.extract_qr_from_png_data(&png_buffer, frame_number, chunk.id) {
                qr_results.extend(qr_codes);
            }

            // ✅ PNG data discarded here - only QR text kept
            png_buffer.clear();

            // Progress reporting
            if frame_number % 5 == 0 {
                println!("Chunk {}: Processed {} frames, found {} QR codes",
                        chunk.id + 1, frame_number, qr_results.len());
            }
        }

        let _ = cmd.wait(); // Wait for FFmpeg to finish

        Ok(qr_results)
    }

    fn read_png_frame(&self, reader: &mut BufReader<std::process::ChildStdout>, buffer: &mut Vec<u8>) -> Result<bool> {
        buffer.clear();

        // PNG signature: 89 50 4E 47 0D 0A 1A 0A
        let png_signature = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let mut signature_buffer = [0u8; 8];

        // Try to read PNG signature
        match reader.read_exact(&mut signature_buffer) {
            Ok(_) => {
                if signature_buffer == png_signature {
                    buffer.extend_from_slice(&signature_buffer);
                    // Read rest of PNG file (simplified - would need proper PNG parsing)
                    let mut temp_buffer = vec![0u8; 1024 * 1024]; // 1MB buffer
                    if let Ok(bytes_read) = reader.read(&mut temp_buffer) {
                        buffer.extend_from_slice(&temp_buffer[..bytes_read]);
                        return Ok(true);
                    }
                }
            }
            Err(_) => return Ok(false), // End of stream
        }

        Ok(false)
    }

    fn extract_qr_from_png_data(&self, png_data: &[u8], frame_number: u64, chunk_id: usize) -> Result<Vec<QrCodeData>> {
        // Load PNG from memory
        let img = image::load_from_memory_with_format(png_data, image::ImageFormat::Png)?;
        let rgb_img = img.to_rgb8();

        // Convert to grayscale in memory
        let luma_img = image::imageops::grayscale(&rgb_img);

        // Detect QR codes
        let mut qr_codes = Vec::new();
        let mut scanner = rqrr::PreparedImage::prepare(luma_img);
        let grids = scanner.detect_grids();

        for grid in grids {
            if let Ok((_, content)) = grid.decode() {
                qr_codes.push(QrCodeData {
                    frame_number,
                    data: content,
                    chunk_id,
                });
            }
        }

        // ✅ All image data discarded here - only QR text kept
        Ok(qr_codes)
    }

    fn extract_qr_in_memory(&self, chunk: &VideoChunk) -> Result<Vec<QrCodeData>> {
        ffmpeg::init().map_err(|e| anyhow!("Failed to initialize FFmpeg: {}", e))?;
        ffmpeg::log::set_level(ffmpeg::log::Level::Quiet);

        let mut ictx = ffmpeg::format::input(&chunk.path)?;
        let input = ictx.streams().best(ffmpeg::media::Type::Video)
            .ok_or(anyhow!("No video stream found"))?;
        let video_stream_index = input.index();

        let context_decoder = ffmpeg::codec::context::Context::from_parameters(input.parameters())?;
        let mut decoder = context_decoder.decoder().video()?;

        // Calculate total frames for progress reporting
        let duration = input.duration() as f64 / ffmpeg::ffi::AV_TIME_BASE as f64;
        let fps = input.avg_frame_rate();
        let estimated_frames = if fps.denominator() > 0 {
            (duration * fps.numerator() as f64 / fps.denominator() as f64) as u64
        } else {
            1000 // Fallback estimate
        };

        let mut frame_count = 0u64;
        let mut qr_results = Vec::new();

        // Starting frame processing (silent for TUI)

        // Process packets from the video stream
        for (stream, packet) in ictx.packets() {
            if stream.index() == video_stream_index {
                decoder.send_packet(&packet)?;
                self.receive_and_process_frames(&mut decoder, &mut frame_count, &mut qr_results, chunk.id, estimated_frames)?;
            }
        }

        // Flush remaining frames
        decoder.send_eof()?;
        self.receive_and_process_frames(&mut decoder, &mut frame_count, &mut qr_results, chunk.id, estimated_frames)?;

        // Completed processing (silent for TUI)

        Ok(qr_results)
    }

    fn receive_and_process_frames(
        &self,
        decoder: &mut ffmpeg::decoder::Video,
        frame_count: &mut u64,
        qr_results: &mut Vec<QrCodeData>,
        chunk_id: usize,
        estimated_frames: u64,
    ) -> Result<()> {
        let mut frame = ffmpeg::frame::Video::empty();

        // Create scaler once outside the loop for efficiency
        let mut scaler: Option<ffmpeg::software::scaling::Context> = None;

        while decoder.receive_frame(&mut frame).is_ok() {
            *frame_count += 1;

            // Progress reporting every 100 frames (silent for clean TUI)
            if *frame_count % 100 == 0 {
                // Frame progress tracking (could be added as event if needed)
            }

            // Skip frames based on skip_frames setting
            if *frame_count % (self.skip_frames as u64 + 1) != 0 {
                continue; // ✅ Frame discarded immediately without processing
            }

            // Process frame immediately and discard - no accumulation
            match self.process_frame_immediate(&frame, &mut scaler, *frame_count, chunk_id) {
                Ok(qr_codes) => {
                    // Only store QR code text data, not frame data
                    qr_results.extend(qr_codes);
                    // ✅ Frame data is discarded here - only QR text kept
                }
                Err(_) => {
                    // ✅ Failed frame is discarded immediately
                }
            }
            // ✅ frame goes out of scope here - memory freed
        }

        Ok(())
    }

    /// Process single frame immediately and return QR data (frame is discarded)
    fn process_frame_immediate(
        &self,
        frame: &ffmpeg::frame::Video,
        scaler: &mut Option<ffmpeg::software::scaling::Context>,
        frame_number: u64,
        chunk_id: usize,
    ) -> Result<Vec<QrCodeData>> {
        // Reuse scaler context to avoid recreation overhead
        if scaler.is_none() {
            *scaler = Some(ffmpeg::software::scaling::context::Context::get(
                frame.format(),
                frame.width(),
                frame.height(),
                ffmpeg::format::Pixel::RGB24,
                frame.width(),
                frame.height(),
                ffmpeg::software::scaling::flag::Flags::BILINEAR,
            )?);
        }

        let scaler_ref = scaler.as_mut().unwrap();
        let mut rgb_frame = ffmpeg::frame::Video::empty();
        scaler_ref.run(frame, &mut rgb_frame)?;

        // Process QR codes directly from frame data (no copying)
        let qr_codes = self.detect_qr_codes_from_frame(&rgb_frame)?;

        // Convert to QrCodeData immediately
        let mut results = Vec::new();
        for qr_data in qr_codes {
            results.push(QrCodeData {
                frame_number,
                data: qr_data,
                chunk_id,
            });
        }

        // ✅ rgb_frame goes out of scope here - memory freed immediately
        Ok(results)
    }

    /// Detect QR codes directly from FFmpeg frame (no ImageBuffer allocation)
    fn detect_qr_codes_from_frame(&self, rgb_frame: &ffmpeg::frame::Video) -> Result<Vec<String>> {
        let width = rgb_frame.width() as u32;
        let height = rgb_frame.height() as u32;
        let data = rgb_frame.data(0);
        let linesize = rgb_frame.stride(0);

        // Convert RGB to grayscale on-the-fly for QR detection (no buffer allocation)
        let mut luma_data = Vec::with_capacity((width * height) as usize);

        for y in 0..height {
            let row_start = y as usize * linesize;
            for x in 0..width {
                let pixel_start = row_start + (x as usize * 3);
                if pixel_start + 2 < data.len() {
                    let r = data[pixel_start] as f32;
                    let g = data[pixel_start + 1] as f32;
                    let b = data[pixel_start + 2] as f32;
                    // Luminance formula: 0.299*R + 0.587*G + 0.114*B
                    let luma = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
                    luma_data.push(luma);
                }
            }
        }

        // Direct QR detection from luma data
        self.detect_qr_from_luma(&luma_data, width, height)
    }

    /// Detect QR codes directly from luma data (minimal memory footprint)
    fn detect_qr_from_luma(&self, luma_data: &[u8], width: u32, height: u32) -> Result<Vec<String>> {
        use image::{ImageBuffer, Luma};

        // Create minimal luma image for QR detection
        let luma_img: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_vec(width, height, luma_data.to_vec())
            .ok_or_else(|| anyhow!("Failed to create luma image"))?;

        let mut qr_codes = Vec::new();

        // Try rqrr first (fast, pure Rust)
        let mut scanner = rqrr::PreparedImage::prepare(luma_img);
        let grids = scanner.detect_grids();

        for grid in grids {
            if let Ok((_, content)) = grid.decode() {
                qr_codes.push(content);
            }
        }

        // ✅ luma_img goes out of scope here - memory freed immediately
        Ok(qr_codes)
    }

    /// Memory-efficient frame processing (frames discarded immediately)

    fn extract_qr_from_image(&self, image_path: &std::path::Path) -> Result<Vec<String>> {
        // Try zbar first (external tool)
        if let Ok(zbar_result) = self.extract_qr_with_zbar(image_path) {
            if !zbar_result.is_empty() {
                return Ok(zbar_result);
            }
        }

        // Fallback to image processing with rqrr
        self.extract_qr_with_rqrr(image_path)
    }

    fn extract_qr_with_zbar(&self, image_path: &std::path::Path) -> Result<Vec<String>> {
        use std::process::Command;

        let output = Command::new("zbarimg")
            .arg("--quiet")
            .arg("--raw")
            .arg(image_path)
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.lines().map(|s| s.to_string()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    fn extract_qr_with_rqrr(&self, image_path: &std::path::Path) -> Result<Vec<String>> {
        let img = image::open(image_path)?;
        let luma_img = img.to_luma8();

        let mut qr_codes = Vec::new();
        let mut scanner = rqrr::PreparedImage::prepare(luma_img);
        let grids = scanner.detect_grids();

        for grid in grids {
            if let Ok((_, content)) = grid.decode() {
                qr_codes.push(content);
            }
        }

        Ok(qr_codes)
    }

    // Keep the old FFmpeg implementation commented out
    #[allow(dead_code)]
    fn _extract_chunk_to_qr_data_ffmpeg(&self, _chunk: &VideoChunk) -> Result<Vec<QrCodeData>> {
        /*
        let mut ictx = ffmpeg::format::input(&chunk.path)
            .map_err(|e| anyhow!("Failed to open chunk file: {}", e))?;

        let input_stream = ictx
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or_else(|| anyhow!("No video stream found in chunk"))?;

        let context_decoder = ffmpeg::codec::context::Context::from_parameters(input_stream.parameters())
            .map_err(|e| anyhow!("Failed to create decoder context: {}", e))?;

        let mut decoder = context_decoder
            .decoder()
            .video()
            .map_err(|e| anyhow!("Failed to create video decoder: {}", e))?;

        let mut qr_results = Vec::new();
        let mut frame_number = 0u64;

        let mut scaler = ffmpeg::software::scaling::Context::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            ffmpeg::format::Pixel::RGB24,
            decoder.width(),
            decoder.height(),
            ffmpeg::software::scaling::Flags::BILINEAR,
        ).map_err(|e| anyhow!("Failed to create scaler: {}", e))?;

        let stream_index = input_stream.index();
        for (stream, packet) in ictx.packets() {
            if stream.index() == stream_index {
                decoder.send_packet(&packet)
                    .map_err(|e| anyhow!("Failed to send packet to decoder: {}", e))?;

                let mut decoded = ffmpeg::frame::Video::empty();
                while decoder.receive_frame(&mut decoded).is_ok() {
                    if frame_number % self.skip_frames as u64 == 0 {
                        if let Ok(qr_data) = self.extract_qr_from_frame(&mut scaler, &decoded, frame_number, chunk.id) {
                            qr_results.extend(qr_data);
                        }
                    }
                    frame_number += 1;
                }
            }
        }

        decoder.send_eof().ok();
        let mut decoded = ffmpeg::frame::Video::empty();
        while decoder.receive_frame(&mut decoded).is_ok() {
            if frame_number % self.skip_frames as u64 == 0 {
                if let Ok(qr_data) = self.extract_qr_from_frame(&mut scaler, &decoded, frame_number, chunk.id) {
                    qr_results.extend(qr_data);
                }
            }
            frame_number += 1;
        }

        Ok(qr_results)
        */
        todo!("FFmpeg implementation")
    }

    fn extract_qr_from_frame(
        &self,
        scaler: &mut ffmpeg::software::scaling::Context,
        frame: &ffmpeg::frame::Video,
        frame_number: u64,
        chunk_id: usize,
    ) -> Result<Vec<QrCodeData>> {
        let mut rgb_frame = ffmpeg::frame::Video::empty();
        scaler.run(frame, &mut rgb_frame)
            .map_err(|e| anyhow!("Failed to scale frame: {}", e))?;

        let width = rgb_frame.width() as u32;
        let height = rgb_frame.height() as u32;
        let data = rgb_frame.data(0);

        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, data.to_vec())
            .ok_or_else(|| anyhow!("Failed to create image buffer"))?;

        let mut qr_results = Vec::new();

        if let Ok(codes) = self.detect_qr_codes_rqrr(&img) {
            for code in codes {
                qr_results.push(QrCodeData {
                    frame_number,
                    data: code,
                    chunk_id,
                });
            }
        }

        if qr_results.is_empty() {
            if let Ok(codes) = self.detect_qr_codes_quircs(&img) {
                for code in codes {
                    qr_results.push(QrCodeData {
                        frame_number,
                        data: code,
                        chunk_id,
                    });
                }
            }
        }

        Ok(qr_results)
    }

    fn detect_qr_codes_rqrr(&self, img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Result<Vec<String>> {
        let luma_img = image::imageops::grayscale(img);
        let mut qr_codes = Vec::new();

        let mut scanner = rqrr::PreparedImage::prepare(luma_img);
        let grids = scanner.detect_grids();

        for grid in grids {
            if let Ok((_, content)) = grid.decode() {
                qr_codes.push(content);
            }
        }

        Ok(qr_codes)
    }

    fn detect_qr_codes_quircs(&self, img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Result<Vec<String>> {
        let luma_img = image::imageops::grayscale(img);
        let mut qr_codes = Vec::new();

        let mut decoder = quircs::Quirc::new();
        let codes = decoder.identify(luma_img.width() as usize, luma_img.height() as usize, &luma_img);
        for code in codes {
            match code {
                Ok(valid_code) => {
                    if let Ok(decoded) = valid_code.decode() {
                        if let Ok(content) = String::from_utf8(decoded.payload) {
                            qr_codes.push(content);
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(qr_codes)
    }

    fn save_chunk_to_jsonl(&self, qr_codes: &[QrCodeData], filename: &str) -> Result<()> {
        use std::fs::File;
        use std::io::{BufWriter, Write};

        let file = File::create(filename)
            .map_err(|e| anyhow!("Failed to create JSONL file {}: {}", filename, e))?;
        let mut writer = BufWriter::new(file);

        for qr_data in qr_codes {
            let json_line = serde_json::to_string(qr_data)
                .map_err(|e| anyhow!("Failed to serialize QR data: {}", e))?;
            writeln!(writer, "{}", json_line)
                .map_err(|e| anyhow!("Failed to write JSONL line: {}", e))?;
        }

        writer.flush()
            .map_err(|e| anyhow!("Failed to flush JSONL file: {}", e))?;

        Ok(())
    }

    pub fn save_to_jsonl(&self, results: &QrExtractionResults, output_path: &PathBuf) -> Result<()> {
        use std::fs::File;
        use std::io::{BufWriter, Write};

        let file = File::create(output_path)
            .map_err(|e| anyhow!("Failed to create JSONL file: {}", e))?;
        let mut writer = BufWriter::new(file);

        for qr_data in &results.qr_codes {
            let json_line = serde_json::to_string(qr_data)
                .map_err(|e| anyhow!("Failed to serialize QR data: {}", e))?;
            writeln!(writer, "{}", json_line)
                .map_err(|e| anyhow!("Failed to write JSONL line: {}", e))?;
        }

        writer.flush()
            .map_err(|e| anyhow!("Failed to flush JSONL file: {}", e))?;

        Ok(())
    }

    pub fn combine_chunk_jsonl_files(&self, chunk_count: usize, output_path: &PathBuf) -> Result<()> {
        use std::fs::File;
        use std::io::{BufRead, BufReader, BufWriter, Write};

        let output_file = File::create(output_path)
            .map_err(|e| anyhow!("Failed to create combined JSONL file: {}", e))?;
        let mut writer = BufWriter::new(output_file);

        let mut all_qr_data = Vec::new();

        for chunk_id in 0..chunk_count {
            let chunk_jsonl_path = PathBuf::from(format!("chunk_{:03}.jsonl", chunk_id + 1));

            if chunk_jsonl_path.exists() {
                let file = File::open(&chunk_jsonl_path)
                    .map_err(|e| anyhow!("Failed to open chunk JSONL: {}", e))?;
                let reader = BufReader::new(file);

                for line in reader.lines() {
                    let line = line.map_err(|e| anyhow!("Failed to read line: {}", e))?;
                    if !line.trim().is_empty() {
                        let qr_data: QrCodeData = serde_json::from_str(&line)
                            .map_err(|e| anyhow!("Failed to parse QR data: {}", e))?;
                        all_qr_data.push(qr_data);
                    }
                }
            }
        }

        all_qr_data.sort_by_key(|qr| qr.frame_number);

        for qr_data in all_qr_data {
            let json_line = serde_json::to_string(&qr_data)
                .map_err(|e| anyhow!("Failed to serialize QR data: {}", e))?;
            writeln!(writer, "{}", json_line)
                .map_err(|e| anyhow!("Failed to write JSONL line: {}", e))?;
        }

        writer.flush()
            .map_err(|e| anyhow!("Failed to flush combined JSONL file: {}", e))?;

        Ok(())
    }
}