use anyhow::{anyhow, Result};
use ffmpeg_next as ffmpeg;
use image::{ImageBuffer, Rgb};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
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
                    if let Ok(_) = self.save_chunk_to_jsonl(&chunk_results, &jsonl_path.to_string_lossy()) {
                        cb(ProcessingEvent::ChunkCompleted {
                            chunk_id: chunk.id,
                            qr_codes_found: qr_count,
                            jsonl_file: jsonl_filename,
                            duration_ms,
                        });
                    } else {
                        cb(ProcessingEvent::Error {
                            phase: 2,
                            error: format!("Failed to save JSONL for chunk {}", chunk.id + 1),
                        });
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
            message: format!("Extracted {} QR codes from {} chunks in {}ms",
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
        use std::process::Command;
        use std::fs;

        let temp_dir = format!("temp_frames_{}", chunk.id);
        fs::create_dir_all(&temp_dir)?;

        // Extract frames using external ffmpeg with temporal sampling for speed
        let sample_rate = std::cmp::max(self.skip_frames, 30); // At least every 30 frames (1 second)
        let extract_cmd = Command::new("ffmpeg")
            .args([
                "-i", &chunk.path.to_string_lossy(),
                "-vf", &format!("fps=1/{}", sample_rate), // Sample every N seconds
                "-frames:v", "50", // Limit to 50 frames maximum per chunk
                "-y",
                &format!("{}/frame_%06d.png", temp_dir)
            ])
            .output()?;

        if !extract_cmd.status.success() {
            // Silently return empty if frame extraction fails
            return Ok(Vec::new());
        }

        // Process extracted frames
        let frame_files = fs::read_dir(&temp_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().extension()
                    .and_then(|ext| ext.to_str())
                    == Some("png")
            })
            .collect::<Vec<_>>();

        let mut qr_results = Vec::new();
        for (frame_idx, frame_file) in frame_files.iter().enumerate() {
            if let Ok(qr_codes) = self.extract_qr_from_image(&frame_file.path()) {
                for qr_data in qr_codes {
                    qr_results.push(QrCodeData {
                        frame_number: frame_idx as u64,
                        data: qr_data,
                        chunk_id: chunk.id,
                    });
                }
            }
        }

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);

        Ok(qr_results)
    }

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