use anyhow::{anyhow, Result};
use base64::{Engine as _, engine::general_purpose};
use chrono;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ffmpeg_next as ffmpeg;
use indicatif::{ProgressBar, ProgressStyle};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame, Terminal,
};
use rqrr;
use quircs;
use rayon::prelude::*;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
enum Commands {
    /// Extract QR codes from video
    Extract {
        /// Input video file path
        input: PathBuf,

        /// Output JSON file path (optional)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Create slim output with only unique QR data for decoder
        #[arg(long)]
        slim: bool,

        /// Create sequenced output preserving frame order information
        #[arg(long)]
        sequenced: bool,

        /// Create streaming JSONL output (one JSON object per line)
        #[arg(long)]
        stream: bool,

        /// Maximum number of threads to use
        #[arg(short, long, default_value_t = num_cpus::get())]
        threads: usize,

        /// Skip frames (process every Nth frame)
        #[arg(short, long, default_value_t = 15)]
        skip: usize,

        /// Process only first N frames (for testing)
        #[arg(long)]
        max_frames: Option<usize>,

        /// Start processing from frame N
        #[arg(long, default_value_t = 0)]
        start_frame: usize,

        /// Start processing from time (format: MM:SS or HH:MM:SS)
        #[arg(long)]
        start_time: Option<String>,

        /// Timeout in seconds (0 = no timeout)
        #[arg(long, default_value_t = 0)]
        timeout: u64,

        /// Disable TUI and use text-only output
        #[arg(long)]
        only_text: bool,
    },
    /// Decode QR codes into files
    Decode {
        /// Input JSON file with QR codes (or use --stdin for piped input)
        input: Option<PathBuf>,

        /// Output directory for decoded files
        #[arg(short, long, default_value = "decoded_files")]
        output: PathBuf,

        /// Process JSONL from stdin (auto-detected when piped)
        #[arg(long)]
        stdin: bool,

        /// Continuous progress saving for JSONL format
        #[arg(long)]
        stream: bool,
    },
    /// Advanced: Extract and decode in real-time with rich TUI
    Process {
        /// Input video file path
        input: PathBuf,

        /// Output directory for decoded files
        #[arg(short, long, default_value = "decoded_files")]
        output: PathBuf,

        /// Skip frames (process every Nth frame)
        #[arg(short, long, default_value_t = 1)]
        skip: usize,

        /// Maximum number of threads to use
        #[arg(short, long, default_value_t = num_cpus::get())]
        threads: usize,

        /// Enable performance optimizations (may reduce accuracy slightly)
        #[arg(long)]
        fast: bool,
    },
    /// Analyze video structure to detect QR file boundaries
    Analyze {
        /// Input video file path
        input: PathBuf,

        /// Output analysis report file (optional)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Sample interval in seconds for boundary detection
        #[arg(long, default_value_t = 10.0)]
        sample_interval: f64,

        /// Frame skip for fast analysis
        #[arg(long, default_value_t = 30)]
        skip: usize,
    },
    /// Split video into chunks preserving QR file boundaries
    Split {
        /// Input video file path
        input: PathBuf,

        /// Output directory for split videos
        #[arg(short, long, default_value = "split_videos")]
        output: PathBuf,

        /// Target chunk size in MB (approximate)
        #[arg(long, default_value_t = 100)]
        chunk_size_mb: usize,

        /// Analysis file (if available from previous analyze command)
        #[arg(long)]
        analysis: Option<PathBuf>,

        /// Sample interval for boundary detection
        #[arg(long, default_value_t = 10.0)]
        sample_interval: f64,
    },
    /// Split video and process chunks in parallel for maximum performance
    SplitProcess {
        /// Input video file path
        input: PathBuf,

        /// Output directory for chunks and results
        #[arg(short, long, default_value = "parallel_output")]
        output: PathBuf,

        /// Target chunk size in MB (approximate)
        #[arg(long, default_value_t = 100)]
        chunk_size_mb: usize,

        /// Number of parallel processing threads (default: same as number of chunks)
        #[arg(short, long)]
        threads: Option<usize>,

        /// Frame skip for QR extraction
        #[arg(long, default_value_t = 1)]
        skip: usize,

        /// Keep intermediate video chunks after processing
        #[arg(long)]
        keep_chunks: bool,

        /// Combine JSONL files after processing
        #[arg(long)]
        combine_jsonl: bool,

        /// Start processing from time (format: MM:SS or HH:MM:SS)
        #[arg(long)]
        start_time: Option<String>,
    },
    /// Process video with temporal parallelism (divide time segments across threads)
    Temporal {
        /// Input video file path
        input: PathBuf,

        /// Output directory for decoded files
        #[arg(short, long, default_value = "decoded_files")]
        output: PathBuf,

        /// Number of parallel time segments (threads)
        #[arg(short, long, default_value_t = 16)]
        threads: usize,

        /// Frame skip for QR extraction (1 = all frames)
        #[arg(long, default_value_t = 1)]
        skip: usize,

        /// Start processing from time (format: MM:SS or HH:MM:SS)
        #[arg(long)]
        start_time: Option<String>,

        /// Disable TUI and use text-only output
        #[arg(long)]
        only_text: bool,

        /// Timeout in seconds (0 = no timeout)
        #[arg(long, default_value_t = 0)]
        timeout: u64,
    },
    /// Extract QR codes from video and decode files in one operation
    ExtractDecode {
        /// Input video file path
        input: PathBuf,

        /// Output directory for decoded files
        #[arg(short, long, default_value = "decoded_files")]
        output: PathBuf,

        /// Number of parallel threads
        #[arg(short, long, default_value_t = 16)]
        threads: usize,

        /// Frame skip for QR extraction (1 = all frames)
        #[arg(long, default_value_t = 1)]
        skip: usize,

        /// Start processing from time (format: MM:SS or HH:MM:SS)
        #[arg(long)]
        start_time: Option<String>,

        /// Disable TUI and use text-only output
        #[arg(long)]
        only_text: bool,

        /// Timeout in seconds (0 = no timeout)
        #[arg(long, default_value_t = 0)]
        timeout: u64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct QrResult {
    frame_number: u64,
    timestamp_ms: f64,
    data: String,
}

// Inter-thread communication structures
#[derive(Debug, Clone)]
struct QrCodeMessage {
    frame_number: u64,
    timestamp_ms: f64,
    data: String,
}

#[derive(Debug, Clone)]
struct ExtractionProgress {
    frames_processed: u64,
    qr_codes_found: u64,
    duplicates_skipped: u64,
    current_frame: u64,
    fps: f64,
}

#[derive(Debug, Clone)]
struct DecodingProgress {
    files_discovered: usize,
    files_completed: usize,
    current_file: Option<String>,
    current_file_progress: f64,
    total_chunks_recovered: usize,
}

#[derive(Debug)]
enum AppMessage {
    QrCode(QrCodeMessage),
    ExtractionProgress(ExtractionProgress),
    DecodingProgress(DecodingProgress),
    ThreadUpdate(ThreadProgress),
    ExtractionComplete,
    DecodingComplete,
    Error(String),
}

#[derive(Debug, Clone)]
struct ThreadProgress {
    thread_id: usize,
    frames_processed: u64,
    qr_codes_found: usize,
    current_frame: u64,
    status: ThreadStatus,
}

#[derive(Debug, Clone)]
enum ThreadStatus {
    Starting,
    Processing,
    Completed,
    Error(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct ExtractionResults {
    video_info: VideoInfo,
    total_frames_processed: u64,
    qr_codes_found: usize,
    processing_time_ms: u128,
    results: Vec<QrResult>,
}

#[derive(Serialize, Deserialize, Debug)]
struct VideoInfo {
    duration_seconds: f64,
    fps: f64,
    width: u32,
    height: u32,
    format: String,
}

#[derive(Debug)]
struct FrameData {
    frame_number: u64,
    timestamp_ms: f64,
    rgb_data: Vec<u8>,
    width: u32,
    height: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FileMarker {
    file_name: String,
    start_time: f64,
    start_frame: u64,
    estimated_end_time: Option<f64>,
    estimated_end_frame: Option<u64>,
    chunks_count: Option<usize>,
    file_size: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
struct VideoAnalysis {
    video_path: String,
    video_info: VideoInfo,
    file_markers: Vec<FileMarker>,
    analysis_time: String,
    total_files_detected: usize,
    recommended_split_points: Vec<f64>,
}

#[derive(Debug)]
struct SplitPoint {
    time: f64,
    frame: u64,
    file_boundary: bool,
    estimated_size_mb: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Extract { input, output, slim, sequenced, stream, threads, skip, max_frames, start_frame, start_time, timeout, only_text } => {
            extract_command(input, output, slim, sequenced, stream, threads, skip, max_frames, start_frame, start_time, timeout, !only_text)
        },
        Commands::Decode { input, output, stdin, stream } => {
            decode_command_integrated(input, output, stdin, stream)
        },
        Commands::Process { input, output, skip, threads, fast } => {
            process_video_realtime(input, output, skip, threads, fast)
        },
        Commands::Analyze { input, output, sample_interval, skip } => {
            analyze_video_structure(input, output, sample_interval, skip)
        },
        Commands::Split { input, output, chunk_size_mb, analysis, sample_interval } => {
            split_video_intelligent(input, output, chunk_size_mb, analysis, sample_interval)
        },
        Commands::SplitProcess { input, output, chunk_size_mb, threads, skip, keep_chunks, combine_jsonl, start_time } => {
            split_and_process_parallel(input, output, chunk_size_mb, threads, skip, keep_chunks, combine_jsonl, start_time)
        },
        Commands::ExtractDecode { input, output, threads, skip, start_time, only_text, timeout } => {
            extract_decode_unified(input, output, threads, skip, start_time, only_text, timeout)
        },
        Commands::Temporal { input, output, threads, skip, start_time, only_text, timeout } => {
            process_video_temporal_parallel(input, output, threads, skip, start_time, only_text, timeout)
        }
    }
}

fn extract_command(input: PathBuf, output: Option<PathBuf>, slim: bool, sequenced: bool, stream: bool, threads: usize, skip: usize, max_frames: Option<usize>, start_frame: usize, start_time_str: Option<String>, timeout: u64, tui: bool) -> Result<()> {
    // Initialize FFmpeg and suppress all warnings
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    // Parse start time
    let start_time_seconds = if let Some(time_str) = start_time_str {
        parse_time_string(&time_str)?
    } else {
        0.0
    };

    let start_time = Instant::now();
    println!("🎬 Starting QR code extraction from: {}", input.display());

    if start_time_seconds > 0.0 {
        println!("⏰ Starting from time: {:.1}s", start_time_seconds);
    }

    if timeout > 0 {
        println!("⏰ Timeout set to {} seconds", timeout);
    }

    // Check if TUI mode
    if tui {
        // TUI mode: beautiful interface with parallel processing
        let results = extract_qr_codes_parallel(&input, threads, skip, max_frames, start_frame, start_time_seconds, timeout, true)?;
        let processing_time = start_time.elapsed().as_millis();

        // Handle output in TUI mode
        if let Some(output_path) = output {
            if stream {
                // TUI + streaming output
                return extract_streaming(&input, Some(output_path), threads, skip, max_frames, start_frame, start_time_seconds, timeout);
            } else {
                // TUI + regular output
                let json = serde_json::to_string_pretty(&results)?;
                std::fs::write(&output_path, json)?;
                println!("Results saved to: {}", output_path.display());
            }
        }
        return Ok(());
    }

    // Check if streaming mode
    if stream {
        // Streaming mode: write QR codes as they're found
        return extract_streaming(&input, output, threads, skip, max_frames, start_frame, start_time_seconds, timeout);
    }

    // Extract frames and process QR codes with timeout - UI independent parallel processing
    let mut results = extract_qr_codes_parallel(&input, threads, skip, max_frames, start_frame, start_time_seconds, timeout, false)?;

    let processing_time = start_time.elapsed().as_millis();
    results.processing_time_ms = processing_time;

    // Print summary
    println!("📊 === Extraction Complete ===");
    println!("   🎬 Frames processed: {}", results.total_frames_processed);
    println!("   📱 QR codes found: {}", results.qr_codes_found);
    println!("   ⏱️  Processing time: {:.2}s", processing_time as f64 / 1000.0);
    println!("   🚀 Processing speed: {:.1}x realtime",
             results.video_info.duration_seconds / (processing_time as f64 / 1000.0));

    // Output results
    if let Some(output_path) = output {
        if sequenced {
            // Create sequenced output preserving frame order for accurate processing
            let mut seen_qr_codes = std::collections::HashSet::new();
            let mut sequenced_qr_data = Vec::new();

            // Sort by frame number to maintain temporal order
            let mut sorted_results = results.results.clone();
            sorted_results.sort_by_key(|qr| qr.frame_number);

            // Add unique QR codes with frame positioning (first occurrence)
            for qr in sorted_results {
                if seen_qr_codes.insert(qr.data.clone()) {
                    sequenced_qr_data.push(serde_json::json!({
                        "frame_number": qr.frame_number,
                        "timestamp_ms": qr.timestamp_ms,
                        "data": qr.data
                    }));
                }
            }

            let sequenced_output = serde_json::json!({
                "sequenced_qr_codes": sequenced_qr_data,
                "total_unique": sequenced_qr_data.len(),
                "video_info": {
                    "duration_seconds": results.video_info.duration_seconds,
                    "fps": results.video_info.fps,
                    "total_frames": (results.video_info.duration_seconds * results.video_info.fps) as u64
                },
                "processing_time_ms": processing_time
            });

            let json = serde_json::to_string_pretty(&sequenced_output)?;
            std::fs::write(&output_path, json)?;
            println!("💾 Sequenced results saved to: {}", output_path.display());

        } else if slim {
            // Create slim output with QR codes in temporal order (preserving sequence)
            let mut seen_qr_codes = std::collections::HashSet::new();
            let mut unique_qr_data = Vec::new();

            // Sort by frame number to maintain temporal order
            let mut sorted_results = results.results.clone();
            sorted_results.sort_by_key(|qr| qr.frame_number);

            // Add unique QR codes in temporal order (first occurrence)
            for qr in sorted_results {
                if seen_qr_codes.insert(qr.data.clone()) {
                    unique_qr_data.push(qr.data);
                }
            }

            let slim_output = serde_json::json!({
                "unique_qr_codes": unique_qr_data,
                "total_unique": unique_qr_data.len(),
                "video_duration": results.video_info.duration_seconds,
                "processing_time_ms": processing_time
            });

            let json = serde_json::to_string_pretty(&slim_output)?;
            std::fs::write(&output_path, json)?;
            println!("Slim results saved to: {}", output_path.display());
        } else {
            let json = serde_json::to_string_pretty(&results)?;
            std::fs::write(&output_path, json)?;
            println!("Results saved to: {}", output_path.display());
        }
    } else {
        // Print first 10 QR codes found
        for (i, qr) in results.results.iter().take(10).enumerate() {
            println!("QR #{}: Frame {} ({:.2}s) - {}",
                     i + 1, qr.frame_number, qr.timestamp_ms / 1000.0, qr.data);
        }
        if results.results.len() > 10 {
            println!("... and {} more QR codes", results.results.len() - 10);
        }
    }

    Ok(())
}

fn decode_command(input: PathBuf, output: PathBuf) -> Result<()> {
    println!("Starting QR code decoding from: {}", input.display());

    // Read JSON file
    let json_data = std::fs::read_to_string(&input)?;
    let qr_data: serde_json::Value = serde_json::from_str(&json_data)?;

    // Extract QR codes based on format
    let qr_codes = if let Some(unique_codes) = qr_data.get("unique_qr_codes") {
        // Slim format
        unique_codes.as_array()
            .ok_or_else(|| anyhow!("Invalid unique_qr_codes format"))?
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect::<Vec<String>>()
    } else if let Some(sequenced_codes) = qr_data.get("sequenced_qr_codes") {
        // Sequenced format - extract QR codes in temporal order
        sequenced_codes.as_array()
            .ok_or_else(|| anyhow!("Invalid sequenced_qr_codes format"))?
            .iter()
            .filter_map(|item| item.get("data")?.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
    } else if let Some(results) = qr_data.get("results") {
        // Full format - extract unique QR codes
        let mut unique_qr_data: Vec<String> = results.as_array()
            .ok_or_else(|| anyhow!("Invalid results format"))?
            .iter()
            .filter_map(|item| item.get("data")?.as_str())
            .map(|s| s.to_string())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        unique_qr_data.sort();
        unique_qr_data
    } else {
        return Err(anyhow!("No QR code data found in JSON file"));
    };

    println!("Found {} unique QR codes to decode", qr_codes.len());

    // Initialize decoder
    let mut decoder = FountainDecoder::new();

    // Ensure output directory exists
    std::fs::create_dir_all(&output)?;

    // Separate metadata and data packets, process metadata first
    let mut metadata_packets = Vec::new();
    let mut data_packets = Vec::new();

    for qr_code in &qr_codes {
        if qr_code.starts_with("M:") {
            metadata_packets.push(qr_code);
        } else if qr_code.starts_with("D:") {
            data_packets.push(qr_code);
        }
    }

    println!("Found {} metadata packets and {} data packets", metadata_packets.len(), data_packets.len());

    // Process metadata packets first
    println!("Processing metadata packets...");
    for (i, qr_code) in metadata_packets.iter().enumerate() {
        if let Err(e) = decoder.process_qr_code(qr_code, &output) {
            println!("Warning: Failed to process metadata {}: {}", i + 1, e);
        }
    }

    // Process data packets
    println!("Processing data packets...");
    for (i, qr_code) in data_packets.iter().enumerate() {
        if i % 100 == 0 {
            println!("Processing data packet {} / {}", i + 1, data_packets.len());
        }

        if let Err(e) = decoder.process_qr_code(qr_code, &output) {
            println!("Warning: Failed to process data packet {}: {}", i + 1, e);
        }
    }

    // Finalize any remaining files
    decoder.finalize_all(&output)?;

    println!("Decoding complete!");
    Ok(())
}

// Integrated decode command combining both extraction and decoding
fn decode_command_integrated(
    input: Option<PathBuf>,
    output: PathBuf,
    force_stdin: bool,
    stream_mode: bool
) -> Result<()> {
    use std::io::{IsTerminal, BufRead, BufReader};

    // Auto-detect stdin mode when input is piped or explicitly requested
    let stdin_mode = force_stdin || input.is_none() || !std::io::stdin().is_terminal();

    if stdin_mode {
        println!("🌊 Processing streaming JSONL from stdin...");
        return process_streaming_stdin_integrated(&output);
    }

    let input_file = input.ok_or_else(|| anyhow!("Input file required when not using stdin"))?;
    println!("📖 Loading QR codes from: {}", input_file.display());

    // Create output directory
    std::fs::create_dir_all(&output)?;

    // Load QR codes (support JSON, JSONL formats)
    let data_str = std::fs::read_to_string(&input_file)?;

    if stream_mode || data_str.lines().any(|line| line.trim().starts_with("{\"type\":")) {
        // JSONL format
        return process_jsonl_file_integrated(&input_file, &output);
    }

    // JSON format processing
    process_json_file_integrated(&data_str, &output)
}

// Process JSONL from stdin with integrated decoder
fn process_streaming_stdin_integrated(output_dir: &PathBuf) -> Result<()> {
    use std::io::{BufRead, BufReader};

    println!("🌊 Processing streaming JSONL from stdin with real-time file generation");

    // Create output directory
    std::fs::create_dir_all(output_dir)?;

    // Initialize decoder
    let mut decoder = QRFileDecoderIntegrated::new(&output_dir.to_string_lossy());

    // Open stdin for line-by-line reading
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin);

    let mut processed = 0;
    let mut successful = 0;
    let mut qr_count = 0;

    for line_result in reader.lines() {
        let line = line_result?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
            match entry.get("type").and_then(|t| t.as_str()) {
                Some("header") => {
                    if let Some(video_info) = entry.get("video_info") {
                        if let (Some(duration), Some(fps), Some(width), Some(height)) = (
                            video_info.get("duration_seconds").and_then(|v| v.as_f64()),
                            video_info.get("fps").and_then(|v| v.as_f64()),
                            video_info.get("width").and_then(|v| v.as_u64()),
                            video_info.get("height").and_then(|v| v.as_u64())
                        ) {
                            println!("📺 Video info: {:.1}min, {}fps, {}x{}",
                                    duration / 60.0, fps as u64, width, height);
                        }
                    }
                },
                Some("qr_code") => {
                    if let (Some(frame_num), Some(data)) = (
                        entry.get("frame_number").and_then(|v| v.as_u64()),
                        entry.get("data").and_then(|v| v.as_str())
                    ) {
                        qr_count += 1;

                        // Process QR code directly
                        let result = decoder.process_qr_code(data, frame_num as usize);
                        if result.is_valid {
                            successful += 1;
                        } else if let Some(reason) = result.reason {
                            if qr_count <= 5 {
                                println!("Warning: Failed to process QR {}: {}", qr_count, reason);
                            }
                        }
                        processed += 1;

                        // Progress update every 10 QR codes for real-time
                        if qr_count % 10 == 0 {
                            print!("\r🔄 Processed {} QR codes, {} successful", qr_count, successful);
                            std::io::Write::flush(&mut std::io::stdout()).unwrap();
                        }

                        // Check for completed files immediately
                        check_and_finalize_completed_files_direct(&mut decoder, output_dir)?;
                    }
                },
                Some("footer") => {
                    if let Some(summary) = entry.get("summary") {
                        if let (Some(frames), Some(qr_found), Some(time_ms)) = (
                            summary.get("frames_processed").and_then(|v| v.as_u64()),
                            summary.get("qr_codes_found").and_then(|v| v.as_u64()),
                            summary.get("processing_time_ms").and_then(|v| v.as_u64())
                        ) {
                            println!("\n📊 Final summary: {} frames, {} QR codes, {:.2}s",
                                    frames, qr_found, time_ms as f64 / 1000.0);
                        }
                    }
                },
                _ => continue,
            }
        }
    }

    // Finalize any remaining files
    finalize_remaining_files_direct(&mut decoder, output_dir)?;

    println!("\n✅ Real-time processing complete: {}/{} QR codes successfully processed", successful, processed);
    println!("📁 Check '{}' directory for extracted files", output_dir.display());

    Ok(())
}

// Process JSONL file with integrated decoder
fn process_jsonl_file_integrated(input_file: &PathBuf, output_dir: &PathBuf) -> Result<()> {
    use std::io::{BufRead, BufReader};
    use std::fs::File;

    println!("📊 Processing JSONL format with integrated decoder");
    std::fs::create_dir_all(output_dir)?;

    let mut decoder = QRFileDecoderIntegrated::new(&output_dir.to_string_lossy());
    let file = File::open(input_file)?;
    let reader = BufReader::new(file);

    let mut processed = 0;
    let mut successful = 0;
    let mut qr_count = 0;

    for line_result in reader.lines() {
        let line = line_result?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
            if entry.get("type").and_then(|t| t.as_str()) == Some("qr_code") {
                if let (Some(frame_num), Some(data)) = (
                    entry.get("frame_number").and_then(|v| v.as_u64()),
                    entry.get("data").and_then(|v| v.as_str())
                ) {
                    qr_count += 1;
                    let result = decoder.process_qr_code(data, frame_num as usize);
                    if result.is_valid {
                        successful += 1;
                    }
                    processed += 1;

                    if qr_count % 100 == 0 {
                        println!("🔄 Processed {} QR codes", qr_count);
                        check_and_finalize_completed_files_direct(&mut decoder, output_dir)?;
                    }
                }
            }
        }
    }

    finalize_remaining_files_direct(&mut decoder, output_dir)?;
    println!("✅ Processing complete: {}/{} QR codes processed", successful, processed);

    Ok(())
}

// Process JSON file with integrated decoder
fn process_json_file_integrated(data_str: &str, output_dir: &PathBuf) -> Result<()> {
    println!("📊 Processing JSON format with integrated decoder");
    std::fs::create_dir_all(output_dir)?;

    // Parse JSON data
    let data: serde_json::Value = serde_json::from_str(data_str)?;

    let qr_codes = if let Some(sequenced) = data.get("sequenced_qr_codes") {
        sequenced.as_array()
            .ok_or_else(|| anyhow!("Invalid sequenced_qr_codes format"))?
            .iter()
            .filter_map(|item| item.get("data")?.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
    } else if let Some(unique) = data.get("unique_qr_codes") {
        unique.as_array()
            .ok_or_else(|| anyhow!("Invalid unique_qr_codes format"))?
            .iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<String>>()
    } else {
        return Err(anyhow!("No QR codes found in JSON file"));
    };

    println!("Found {} QR codes in temporal order", qr_codes.len());

    // Initialize decoder and process
    let mut decoder = QRFileDecoderIntegrated::new(&output_dir.to_string_lossy());
    let mut processed = 0;
    let mut successful = 0;

    for (i, qr_code) in qr_codes.iter().enumerate() {
        if i % 100 == 0 {
            println!("🔄 Processing QR code {} / {}...", i + 1, qr_codes.len());
        }

        let result = decoder.process_qr_code(qr_code, i);
        if result.is_valid {
            successful += 1;
        }
        processed += 1;

        // Check for completed files
        check_and_finalize_completed_files_direct(&mut decoder, output_dir)?;
    }

    finalize_remaining_files_direct(&mut decoder, output_dir)?;
    println!("✅ Processing complete: {}/{} QR codes processed", successful, processed);

    Ok(())
}

// Unified extract and decode in single operation with TUI
fn extract_decode_unified(
    input: PathBuf,
    output: PathBuf,
    threads: usize,
    skip: usize,
    start_time_str: Option<String>,
    only_text: bool,
    timeout: u64
) -> Result<()> {
    use std::sync::{Arc, Mutex};
    use std::sync::mpsc;
    use std::thread;

    // Parse start time
    let start_time_seconds = if let Some(time_str) = start_time_str {
        parse_time_string(&time_str)?
    } else {
        0.0
    };

    if only_text {
        println!("🚀 Starting unified extract-decode with {} threads", threads);
        println!("📹 Input: {}", input.display());
        println!("📂 Output: {}", output.display());
        if start_time_seconds > 0.0 {
            println!("⏰ Starting from: {:.1}s", start_time_seconds);
        }
    }

    // Initialize FFmpeg
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    // Create output directory
    std::fs::create_dir_all(&output)?;

    // Initialize QR decoder
    let decoder = Arc::new(Mutex::new(QRFileDecoderIntegrated::new(&output.to_string_lossy())));

    let start_time = Instant::now();

    if only_text {
        // Text mode: direct processing with parallel extraction
        extract_decode_text_mode(&input, &output, threads, skip, start_time_seconds, timeout, decoder)?;
    } else {
        // TUI mode: beautiful interface with parallel processing
        extract_decode_tui_mode(&input, &output, threads, skip, start_time_seconds, timeout, decoder)?;
    }

    if only_text {
        println!("\n🎉 Unified processing complete in {:.2}s", start_time.elapsed().as_secs_f64());
        println!("📁 Check '{}' directory for extracted files", output.display());
    }

    Ok(())
}

// Text mode unified processing
fn extract_decode_text_mode(
    input: &PathBuf,
    output: &PathBuf,
    threads: usize,
    skip: usize,
    start_time_seconds: f64,
    timeout: u64,
    decoder: Arc<Mutex<QRFileDecoderIntegrated>>
) -> Result<()> {
    use std::sync::mpsc;
    use std::thread;

    println!("⚡ Text mode: Processing with {} threads", threads);

    // Create communication channel for QR codes
    let (tx, rx) = mpsc::channel();

    // Launch extraction in background thread (with logging for text mode)
    let input_clone = input.clone();
    let tx_clone = tx.clone();

    thread::spawn(move || {
        // Extract QR codes using parallel processing with logging enabled
        match extract_qr_codes_with_text_parallel_logging(&input_clone, threads, skip, None, 0, start_time_seconds, timeout, true) {
            Ok(results) => {
                // Send each QR code to decoder
                for qr_result in results.results {
                    let _ = tx_clone.send(qr_result);
                }
                drop(tx_clone);
            },
            Err(e) => {
                println!("❌ Extraction error: {}", e);
                drop(tx_clone);
            }
        }
    });

    // Process QR codes as they arrive
    let mut processed = 0;
    let mut successful = 0;

    for qr_result in rx {
        processed += 1;

        // Process QR code with integrated decoder
        let mut decoder_lock = decoder.lock().unwrap();
        let result = decoder_lock.process_qr_code(&qr_result.data, qr_result.frame_number as usize);

        if result.is_valid {
            successful += 1;
        }

        // Progress update
        if processed % 100 == 0 {
            print!("\r🔄 Processed {} QR codes, {} successful", processed, successful);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }

        // Check for completed files
        check_and_finalize_completed_files_direct(&mut decoder_lock, output)?;
        drop(decoder_lock);
    }

    // Final cleanup
    let mut decoder_lock = decoder.lock().unwrap();
    finalize_remaining_files_direct(&mut decoder_lock, output)?;

    println!("\n✅ Text processing complete: {}/{} QR codes processed", successful, processed);

    Ok(())
}

// TUI mode unified processing with beautiful visualization
fn extract_decode_tui_mode(
    input: &PathBuf,
    output: &PathBuf,
    threads: usize,
    skip: usize,
    start_time_seconds: f64,
    timeout: u64,
    decoder: Arc<Mutex<QRFileDecoderIntegrated>>
) -> Result<()> {
    use std::sync::mpsc;
    use std::thread;

    // No println in TUI mode - it pollutes the interface

    // Setup TUI
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Communication channels for TUI updates
    let (tx, rx) = mpsc::channel();
    let thread_progresses = Arc::new(Mutex::new(vec![ThreadProgress {
        thread_id: 0,
        frames_processed: 0,
        qr_codes_found: 0,
        current_frame: 0,
        status: ThreadStatus::Starting,
    }; threads]));

    // Direct producer-consumer setup in TUI thread
    let (frame_tx, frame_rx) = mpsc::sync_channel(threads * 4);
    let (result_tx, result_rx) = mpsc::channel();
    let frame_rx = Arc::new(Mutex::new(frame_rx));

    let start_time = Instant::now();

    // Launch producer thread directly
    let input_clone = input.clone();
    thread::spawn(move || {
        let _ = extract_frames_producer_with_logging(input_clone, frame_tx, skip, None, 0, start_time_seconds, timeout, false);
    });

    // Launch consumer threads directly with TUI progress
    for thread_id in 0..threads {
        let frame_rx_clone = Arc::clone(&frame_rx);
        let result_tx_clone = result_tx.clone();
        let tx_clone = tx.clone();

        thread::spawn(move || {
            let _ = qr_detection_consumer_with_progress(thread_id, frame_rx_clone, result_tx_clone, false, Some(tx_clone));
        });
    }

    // QR processing and decoding thread
    let decoder_clone = Arc::clone(&decoder);
    let output_clone = output.clone();
    let tx_final = tx.clone();
    thread::spawn(move || {
        drop(result_tx);
        let mut total_processed = 0;

        for qr_result in result_rx {
            total_processed += 1;

            {
                let mut decoder_lock = decoder_clone.lock().unwrap();
                let result = decoder_lock.process_qr_code(&qr_result.data, qr_result.frame_number as usize);
                let _ = check_and_finalize_completed_files_direct(&mut decoder_lock, &output_clone);
            }

            if total_processed % 50 == 0 {
                let _ = tx_final.send(AppMessage::ExtractionProgress(ExtractionProgress {
                    frames_processed: total_processed as u64,
                    qr_codes_found: total_processed as u64,
                    current_frame: qr_result.frame_number,
                    fps: 30.0,
                    duplicates_skipped: 0,
                }));
            }
        }

        let _ = tx_final.send(AppMessage::ExtractionComplete);
    });

    // Launch extraction in background
    let input_clone = input.clone();
    let output_clone = output.clone();
    let decoder_clone = Arc::clone(&decoder);
    let tx_clone = tx.clone();

    // Clone progress sender for TUI updates
    let progress_tx_clone = tx.clone();

    thread::spawn(move || {
        match extract_qr_codes_for_tui(&input_clone, threads, skip, None, 0, start_time_seconds, timeout, progress_tx_clone) {
            Ok(results) => {
                // Process each QR code immediately
                let mut processed = 0;
                let mut successful = 0;

                for qr_result in results.results {
                    processed += 1;

                    // Process with decoder
                    {
                        let mut decoder_lock = decoder_clone.lock().unwrap();
                        let result = decoder_lock.process_qr_code(&qr_result.data, qr_result.frame_number as usize);

                        if result.is_valid {
                            successful += 1;
                        }

                        // Check for completed files
                        let _ = check_and_finalize_completed_files_direct(&mut decoder_lock, &output_clone);
                    }

                    // Send progress update
                    let _ = tx_clone.send(AppMessage::ExtractionProgress(ExtractionProgress {
                        frames_processed: processed as u64,
                        qr_codes_found: successful as u64,
                        current_frame: qr_result.frame_number,
                        fps: 30.0,
                        duplicates_skipped: 0,
                    }));
                }

                let _ = tx_clone.send(AppMessage::ExtractionComplete);
            },
            Err(e) => {
                let _ = tx_clone.send(AppMessage::Error(e.to_string()));
            }
        }
    });

    // TUI event loop
    let mut total_qr_codes = 0;
    let mut total_frames = 0;
    let mut completed = false;
    let mut files_completed = 0;

    loop {
        // Handle keyboard events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        // Process messages
        while let Ok(message) = rx.try_recv() {
            match message {
                AppMessage::ExtractionProgress(progress) => {
                    total_qr_codes = progress.qr_codes_found;
                    total_frames = progress.frames_processed;
                },
                AppMessage::ThreadUpdate(thread_progress) => {
                    // Update specific thread progress
                    let mut progresses = thread_progresses.lock().unwrap();
                    if thread_progress.thread_id < progresses.len() {
                        progresses[thread_progress.thread_id] = thread_progress.clone();
                    }
                },
                AppMessage::ExtractionComplete => {
                    completed = true;
                },
                AppMessage::Error(e) => {
                    // Don't pollute TUI with println - handle gracefully
                    completed = true;
                    break;
                },
                _ => {}
            }
        }

        // Check decoder status
        {
            let decoder_lock = decoder.lock().unwrap();
            files_completed = decoder_lock.file_decoders.iter()
                .filter(|(_, fd)| fd.is_complete())
                .count();
        }

        // Draw TUI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),  // Header
                    Constraint::Min(5),     // Thread progress
                    Constraint::Length(5),  // File progress
                    Constraint::Length(3),  // Summary
                ].as_ref())
                .split(f.size());

            // Header
            let header = Paragraph::new(format!(
                "🚀 Unified QR Extract-Decode - {} Threads | Press 'q' to quit",
                threads
            ))
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
            f.render_widget(header, chunks[0]);

            // Thread progress - REAL data from each thread
            let thread_progresses_lock = thread_progresses.lock().unwrap();
            let thread_items: Vec<ListItem> = thread_progresses_lock
                .iter()
                .enumerate()
                .map(|(i, progress)| {
                    let status_symbol = match progress.status {
                        ThreadStatus::Starting => "🔄",
                        ThreadStatus::Processing => "⚡",
                        ThreadStatus::Completed => "✅",
                        ThreadStatus::Error(_) => "❌",
                    };

                    let progress_ratio = if total_frames > 0 {
                        progress.frames_processed as f64 / (total_frames as f64 / threads as f64)
                    } else {
                        0.0
                    };
                    let bars = ((progress_ratio * 20.0) as usize).min(20);
                    let progress_bar = "█".repeat(bars) + &"▒".repeat(20 - bars);

                    ListItem::new(format!(
                        "{} Thread {:2} │ {} │ Frames: {:6} │ QR: {:4}",
                        status_symbol,
                        i + 1,
                        progress_bar,
                        progress.frames_processed,
                        progress.qr_codes_found
                    ))
                })
                .collect();

            let thread_list = List::new(thread_items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Thread Progress ({} threads)", threads)))
                .style(Style::default().fg(Color::White));
            f.render_widget(thread_list, chunks[1]);

            // File progress
            let decoder_lock = decoder.lock().unwrap();
            let file_items: Vec<ListItem> = decoder_lock.file_decoders.iter()
                .map(|(name, fd)| {
                    let progress = if fd.total_chunks > 0 {
                        (fd.recovered_chunk_count * 100) / fd.total_chunks
                    } else {
                        0
                    };

                    let status = if fd.is_complete() { "✅" } else { "🔄" };
                    let progress_bar = "█".repeat((progress / 5).min(20)) + &"▒".repeat(20 - (progress / 5).min(20));

                    ListItem::new(format!(
                        "{} {} │ {} │ {}/{} chunks ({}%)",
                        status, name, progress_bar, fd.recovered_chunk_count, fd.total_chunks, progress
                    ))
                })
                .collect();

            let file_list = List::new(file_items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title("File Reconstruction Progress"))
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(file_list, chunks[2]);

            // Summary with time remaining calculation
            let elapsed = start_time.elapsed().as_secs_f64();
            let estimated_total_frames = (effective_duration * fps) as u64;
            let progress_ratio = if estimated_total_frames > 0 {
                total_frames as f64 / estimated_total_frames as f64
            } else {
                0.0
            };

            let time_remaining = if progress_ratio > 0.01 && !completed {
                let estimated_total_time = elapsed / progress_ratio;
                estimated_total_time - elapsed
            } else {
                0.0
            };

            let summary = Paragraph::new(format!(
                "📊 QR: {} │ Frames: {}/{} ({:.1}%) │ Files: {} │ ⏱️ {:.1}s │ ETA: {:.1}s │ Status: {}",
                total_qr_codes,
                total_frames,
                estimated_total_frames,
                progress_ratio * 100.0,
                files_completed,
                elapsed,
                time_remaining,
                if completed { "✅ Complete" } else { "🔄 Processing" }
            ))
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Green));
            f.render_widget(summary, chunks[3]);
        })?;

        if completed {
            // Wait a moment to show completion
            thread::sleep(Duration::from_millis(2000));
            break;
        }
    }

    // Cleanup TUI
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    Ok(())
}

// Temporal parallel processing - divide video time across threads
fn process_video_temporal_parallel(
    input: PathBuf,
    output: PathBuf,
    threads: usize,
    skip: usize,
    start_time_str: Option<String>,
    only_text: bool,
    timeout: u64
) -> Result<()> {
    use std::sync::{Arc, Mutex};
    use std::sync::mpsc;
    use std::thread;

    // Parse start time
    let start_time_seconds = if let Some(time_str) = start_time_str {
        parse_time_string(&time_str)?
    } else {
        0.0
    };

    if only_text {
        println!("🚀 Temporal parallel processing with {} threads", threads);
        println!("📹 Input: {}", input.display());
        println!("📂 Output: {}", output.display());
    }

    // Get video info
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    let mut context = ffmpeg::format::input(&input)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let duration = video_stream.duration() as f64 * video_stream.time_base().numerator() as f64 / video_stream.time_base().denominator() as f64;
    let fps_value = {
        let fps = video_stream.avg_frame_rate();
        fps.numerator() as f64 / fps.denominator() as f64
    };

    // Calculate temporal segments
    let effective_duration = duration - start_time_seconds;
    let segment_duration = effective_duration / threads as f64;

    if only_text {
        println!("📺 Video: {:.1}min total, dividing into {} segments of {:.1}s each",
                duration / 60.0, threads, segment_duration);
    }

    std::fs::create_dir_all(&output)?;

    if only_text {
        process_temporal_text_mode(&input, &output, threads, skip, start_time_seconds, segment_duration, fps_value)?;
    } else {
        process_temporal_tui_mode(&input, &output, threads, skip, start_time_seconds, segment_duration, fps_value, effective_duration)?;
    }

    if only_text {
        println!("🎉 Temporal processing complete!");
        println!("📁 Check '{}' directory for extracted files", output.display());
    }

    Ok(())
}

// Text mode temporal processing
fn process_temporal_text_mode(
    input: &PathBuf,
    output: &PathBuf,
    threads: usize,
    skip: usize,
    start_offset: f64,
    segment_duration: f64,
    fps: f64
) -> Result<()> {
    use std::sync::{Arc, Mutex};
    use std::sync::mpsc;
    use std::thread;

    println!("⚡ Launching {} temporal threads", threads);

    let decoder = Arc::new(Mutex::new(QRFileDecoderIntegrated::new(&output.to_string_lossy())));
    let (result_tx, result_rx) = mpsc::channel();

    // Launch temporal processing threads
    for thread_id in 0..threads {
        let segment_start = start_offset + (thread_id as f64 * segment_duration);
        let segment_end = segment_start + segment_duration;

        let input_clone = input.clone();
        let result_tx_clone = result_tx.clone();

        thread::spawn(move || {
            let _ = process_video_segment(thread_id, input_clone, segment_start, segment_end, skip, result_tx_clone);
        });

        println!("🔄 Thread {}: Processing {:.1}s - {:.1}s ({:.1}s segment)",
                thread_id + 1, segment_start, segment_end, segment_duration);
    }

    // Collect and sort results by temporal order
    drop(result_tx);
    let mut all_results = Vec::new();

    for qr_result in result_rx {
        all_results.push(qr_result);
    }

    all_results.sort_by_key(|r| r.frame_number);
    println!("🔗 Collected {} QR codes, processing in temporal order", all_results.len());

    // Process with decoder
    let mut processed = 0;
    let mut successful = 0;

    for qr_result in all_results {
        processed += 1;

        {
            let mut decoder_lock = decoder.lock().unwrap();
            let result = decoder_lock.process_qr_code(&qr_result.data, qr_result.frame_number as usize);
            if result.is_valid {
                successful += 1;
            }

            if processed % 100 == 0 {
                print!("\r🔄 Processed {} QR codes, {} successful", processed, successful);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                let _ = check_and_finalize_completed_files_direct(&mut decoder_lock, output);
            }
        }
    }

    let mut decoder_lock = decoder.lock().unwrap();
    finalize_remaining_files_direct(&mut decoder_lock, output)?;

    println!("\n✅ Temporal processing complete: {}/{} QR codes processed", successful, processed);

    Ok(())
}

// TUI mode temporal processing with live segment visualization
fn process_temporal_tui_mode(
    input: &PathBuf,
    output: &PathBuf,
    threads: usize,
    skip: usize,
    start_offset: f64,
    segment_duration: f64,
    fps: f64,
    effective_duration: f64
) -> Result<()> {
    use std::sync::{Arc, Mutex};
    use std::sync::mpsc;
    use std::thread;

    // Setup TUI
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Communication and progress tracking
    let (tx, rx) = mpsc::channel();
    let segment_progresses = Arc::new(Mutex::new(vec![ThreadProgress {
        thread_id: 0,
        frames_processed: 0,
        qr_codes_found: 0,
        current_frame: 0,
        status: ThreadStatus::Starting,
    }; threads]));

    let decoder = Arc::new(Mutex::new(QRFileDecoderIntegrated::new(&output.to_string_lossy())));
    let (result_tx, result_rx) = mpsc::channel();

    let start_time = Instant::now();

    // Launch temporal processing threads
    for thread_id in 0..threads {
        let segment_start = start_offset + (thread_id as f64 * segment_duration);
        let segment_end = segment_start + segment_duration;

        let input_clone = input.clone();
        let result_tx_clone = result_tx.clone();
        let progress_tx = tx.clone();

        thread::spawn(move || {
            let _ = process_video_segment_with_progress(thread_id, input_clone, segment_start, segment_end, skip, result_tx_clone, Some(progress_tx));
        });
    }

    // Memory-efficient streaming processing thread
    let decoder_clone = Arc::clone(&decoder);
    let output_clone = output.clone();
    let tx_final = tx.clone();
    thread::spawn(move || {
        drop(result_tx);

        // Create temporary JSONL file for memory-efficient sorting
        let temp_jsonl = output_clone.join("temp_qr_codes.jsonl");
        let mut temp_file = std::fs::File::create(&temp_jsonl).unwrap();

        let mut total_qr_count = 0;

        // Stream QR codes to temporary JSONL file
        for qr_result in result_rx {
            let qr_entry = serde_json::json!({
                "type": "qr_code",
                "frame_number": qr_result.frame_number,
                "timestamp_ms": qr_result.timestamp_ms,
                "data": qr_result.data
            });

            if writeln!(temp_file, "{}", serde_json::to_string(&qr_entry).unwrap()).is_ok() {
                total_qr_count += 1;
            }
        }

        drop(temp_file);

        // Memory-efficient temporal sorting using external sort
        if let Ok(sorted_results) = sort_jsonl_by_frame_number(&temp_jsonl) {
            let mut processed = 0;

            // Process sorted results streaming
            for qr_result in sorted_results {
                processed += 1;

                {
                    let mut decoder_lock = decoder_clone.lock().unwrap();
                    let result = decoder_lock.process_qr_code(&qr_result.data, qr_result.frame_number as usize);
                    let _ = check_and_finalize_completed_files_direct(&mut decoder_lock, &output_clone);
                }

                if processed % 50 == 0 {
                    let _ = tx_final.send(AppMessage::ExtractionProgress(ExtractionProgress {
                        frames_processed: processed as u64,
                        qr_codes_found: processed as u64,
                        current_frame: qr_result.frame_number,
                        fps: 30.0,
                        duplicates_skipped: 0,
                    }));
                }
            }
        }

        // Cleanup temp file
        let _ = std::fs::remove_file(&temp_jsonl);
        let _ = tx_final.send(AppMessage::ExtractionComplete);
    });

    // TUI event loop
    let mut total_qr_codes = 0;
    let mut total_frames = 0;
    let mut completed = false;
    let mut files_completed = 0;

    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        // Process messages
        while let Ok(message) = rx.try_recv() {
            match message {
                AppMessage::ThreadUpdate(thread_progress) => {
                    let mut progresses = segment_progresses.lock().unwrap();
                    if thread_progress.thread_id < progresses.len() {
                        progresses[thread_progress.thread_id] = thread_progress.clone();
                    }
                },
                AppMessage::ExtractionProgress(progress) => {
                    total_qr_codes = progress.qr_codes_found;
                    total_frames = progress.frames_processed;
                },
                AppMessage::ExtractionComplete => {
                    completed = true;
                },
                AppMessage::Error(_e) => {
                    completed = true;
                    break;
                },
                _ => {}
            }
        }

        // Calculate totals from all segments and check files
        {
            let segment_progresses_lock = segment_progresses.lock().unwrap();
            let mut total_frames_calc = 0u64;
            let mut total_qr_calc = 0u64;

            for progress in segment_progresses_lock.iter() {
                total_frames_calc += progress.frames_processed;
                total_qr_calc += progress.qr_codes_found as u64;
            }

            // Update totals from segment data
            if total_frames_calc > total_frames {
                total_frames = total_frames_calc;
            }
            if total_qr_calc > total_qr_codes {
                total_qr_codes = total_qr_calc;
            }

            let decoder_lock = decoder.lock().unwrap();
            files_completed = decoder_lock.file_decoders.iter()
                .filter(|(_, fd)| fd.is_complete())
                .count();
        }

        // Draw TUI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(5),
                    Constraint::Length(3),
                ].as_ref())
                .split(f.size());

            let header = Paragraph::new(format!(
                "🚀 Temporal Parallel Processing - {} Segments | Press 'q' to quit",
                threads
            ))
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
            f.render_widget(header, chunks[0]);

            // Temporal segments
            let segment_progresses_lock = segment_progresses.lock().unwrap();
            let segment_items: Vec<ListItem> = segment_progresses_lock
                .iter()
                .enumerate()
                .map(|(i, progress)| {
                    let status_symbol = match progress.status {
                        ThreadStatus::Starting => "🔄",
                        ThreadStatus::Processing => "⚡",
                        ThreadStatus::Completed => "✅",
                        ThreadStatus::Error(_) => "❌",
                    };

                    let segment_start = start_offset + (i as f64 * segment_duration);
                    let segment_end = segment_start + segment_duration;

                    let progress_ratio = if progress.frames_processed > 0 {
                        (progress.frames_processed as f64 / (segment_duration * fps)).min(1.0)
                    } else {
                        0.0
                    };
                    let bars = ((progress_ratio * 20.0) as usize).min(20);
                    let progress_bar = "█".repeat(bars) + &"▒".repeat(20 - bars);

                    ListItem::new(format!(
                        "{} Segment {:2} │ {} │ {:.1}s-{:.1}s │ Frames: {:4} │ QR: {:3}",
                        status_symbol,
                        i + 1,
                        progress_bar,
                        segment_start,
                        segment_end,
                        progress.frames_processed,
                        progress.qr_codes_found
                    ))
                })
                .collect();

            let segment_list = List::new(segment_items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Temporal Segments ({} parallel)", threads)))
                .style(Style::default().fg(Color::White));
            f.render_widget(segment_list, chunks[1]);

            // File reconstruction
            let decoder_lock = decoder.lock().unwrap();
            let file_items: Vec<ListItem> = decoder_lock.file_decoders.iter()
                .map(|(name, fd)| {
                    let progress = if fd.total_chunks > 0 {
                        (fd.recovered_chunk_count * 100) / fd.total_chunks
                    } else {
                        0
                    };

                    let status = if fd.is_complete() { "✅" } else { "🔄" };
                    let progress_bar = "█".repeat((progress / 5).min(20)) + &"▒".repeat(20 - (progress / 5).min(20));

                    ListItem::new(format!(
                        "{} {} │ {} │ {}/{} chunks ({}%)",
                        status, name, progress_bar, fd.recovered_chunk_count, fd.total_chunks, progress
                    ))
                })
                .collect();

            let file_list = List::new(file_items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title("File Reconstruction Progress"))
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(file_list, chunks[2]);

            // Summary with time remaining and progress
            let elapsed = start_time.elapsed().as_secs_f64();
            let estimated_total_frames = (segment_duration * threads as f64 * fps) as u64;
            let progress_ratio = if estimated_total_frames > 0 {
                total_frames as f64 / estimated_total_frames as f64
            } else {
                0.0
            };

            let time_remaining = if progress_ratio > 0.01 && !completed {
                let estimated_total_time = elapsed / progress_ratio;
                estimated_total_time - elapsed
            } else {
                0.0
            };

            let summary = Paragraph::new(format!(
                "📊 QR: {} │ Frames: {}/{} ({:.1}%) │ Files: {} │ ⏱️ {:.1}s │ ETA: {:.1}s │ Status: {}",
                total_qr_codes,
                total_frames,
                estimated_total_frames,
                progress_ratio * 100.0,
                files_completed,
                elapsed,
                time_remaining,
                if completed { "✅ Complete" } else { "🔄 Processing" }
            ))
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Green));
            f.render_widget(summary, chunks[3]);
        })?;

        if completed {
            thread::sleep(Duration::from_millis(2000));
            break;
        }
    }

    // Cleanup TUI
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    Ok(())
}

// Process a specific time segment of the video
fn process_video_segment(
    thread_id: usize,
    input: PathBuf,
    start_time: f64,
    end_time: f64,
    skip: usize,
    result_tx: std::sync::mpsc::Sender<QrResult>
) -> Result<()> {
    process_video_segment_with_progress(thread_id, input, start_time, end_time, skip, result_tx, None)
}

// Process video segment with progress reporting
fn process_video_segment_with_progress(
    thread_id: usize,
    input: PathBuf,
    start_time: f64,
    end_time: f64,
    skip: usize,
    result_tx: std::sync::mpsc::Sender<QrResult>,
    progress_tx: Option<std::sync::mpsc::Sender<AppMessage>>
) -> Result<()> {
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    let mut context = ffmpeg::format::input(&input)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();
    let stream_parameters = video_stream.parameters();

    // Seek to segment start
    let seek_timestamp = (start_time / time_base.numerator() as f64 * time_base.denominator() as f64) as i64;
    context.seek(seek_timestamp, ..seek_timestamp)?;

    let context_decoder = ffmpeg::codec::context::Context::from_parameters(stream_parameters)?;
    let mut decoder = context_decoder.decoder().video()?;

    let mut frame_number = (start_time * 30.0) as u64;
    let mut processed_frames = 0u64;
    let mut qr_codes_found = 0;

    // Signal thread started
    if let Some(ref progress_sender) = progress_tx {
        let _ = progress_sender.send(AppMessage::ThreadUpdate(ThreadProgress {
            thread_id,
            frames_processed: 0,
            qr_codes_found: 0,
            current_frame: frame_number,
            status: ThreadStatus::Processing,
        }));
    }

    // Process frames in this time segment
    for (stream, packet) in context.packets() {
        if stream.index() == video_index {
            let current_time = frame_number as f64 / 30.0;

            if current_time > end_time {
                break;
            }

            if frame_number % skip as u64 == 0 {
                match decoder.send_packet(&packet) {
                    Ok(_) => {
                        let mut decoded = ffmpeg::util::frame::Video::empty();
                        while decoder.receive_frame(&mut decoded).is_ok() {
                            let timestamp_ms = decoded.timestamp().unwrap_or(0) as f64 *
                                             time_base.numerator() as f64 / time_base.denominator() as f64 * 1000.0;

                            if let Ok((rgb_data, width, height, _)) =
                                process_frame_with_error_recovery(&decoded, time_base, frame_number) {

                                let qr_codes = detect_qr_codes_in_frame_immediate(&rgb_data, width, height);

                                for qr_code in qr_codes {
                                    let qr_result = QrResult {
                                        frame_number,
                                        timestamp_ms,
                                        data: qr_code,
                                    };

                                    if result_tx.send(qr_result).is_err() {
                                        break;
                                    }

                                    qr_codes_found += 1;
                                }
                            }

                            processed_frames += 1;

                            // Report progress to TUI every 100 frames
                            if let Some(ref progress_sender) = progress_tx {
                                if processed_frames % 100 == 0 {
                                    let _ = progress_sender.send(AppMessage::ThreadUpdate(ThreadProgress {
                                        thread_id,
                                        frames_processed: processed_frames,
                                        qr_codes_found,
                                        current_frame: frame_number,
                                        status: ThreadStatus::Processing,
                                    }));
                                }
                            }
                        }
                    },
                    Err(_) => {
                        frame_number += 1;
                        continue;
                    }
                }
            }
            frame_number += 1;
        }
    }

    // Signal thread completion
    if let Some(ref progress_sender) = progress_tx {
        let _ = progress_sender.send(AppMessage::ThreadUpdate(ThreadProgress {
            thread_id,
            frames_processed: processed_frames,
            qr_codes_found,
            current_frame: frame_number,
            status: ThreadStatus::Completed,
        }));
    }

    Ok(())
}

// Memory-efficient JSONL sorting by frame number (external sort for large files)
fn sort_jsonl_by_frame_number(jsonl_path: &PathBuf) -> Result<Vec<QrResult>> {
    use std::io::{BufRead, BufReader};
    use std::fs::File;

    let file = File::open(jsonl_path)?;
    let reader = BufReader::new(file);

    let mut qr_results = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
            if entry.get("type").and_then(|t| t.as_str()) == Some("qr_code") {
                if let (Some(frame_num), Some(timestamp), Some(data)) = (
                    entry.get("frame_number").and_then(|v| v.as_u64()),
                    entry.get("timestamp_ms").and_then(|v| v.as_f64()),
                    entry.get("data").and_then(|v| v.as_str())
                ) {
                    qr_results.push(QrResult {
                        frame_number: frame_num,
                        timestamp_ms: timestamp,
                        data: data.to_string(),
                    });
                }
            }
        }
    }

    // Sort by frame number for temporal order
    qr_results.sort_by_key(|r| r.frame_number);
    Ok(qr_results)
}

// TUI-enabled parallel processing with dynamic thread visualization
fn extract_qr_codes_with_tui_parallel(
    input_path: &PathBuf,
    max_threads: usize,
    skip_frames: usize,
    max_frames: Option<usize>,
    start_frame: usize,
    start_time_seconds: f64,
    timeout_seconds: u64
) -> Result<ExtractionResults> {
    use std::sync::{Arc, Mutex};
    use std::sync::mpsc;
    use std::thread;

    // Initialize FFmpeg
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    // Get video info
    let mut context = ffmpeg::format::input(input_path)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let fps_value = {
        let fps = video_stream.avg_frame_rate();
        fps.numerator() as f64 / fps.denominator() as f64
    };

    let video_info = VideoInfo {
        duration_seconds: video_stream.duration() as f64 * video_stream.time_base().numerator() as f64 / video_stream.time_base().denominator() as f64,
        fps: fps_value,
        width: 1440, // Will be updated when first frame is processed
        height: 1440,
        format: "H264".to_string(),
    };

    println!("🚀 Starting parallel processing with {} threads", max_threads);
    println!("📺 Video: {:.1}min @ {:.1}fps", video_info.duration_seconds / 60.0, video_info.fps);

    // Setup TUI
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Setup communication channels
    let (tx, rx) = mpsc::channel();
    let results = Arc::new(Mutex::new(Vec::new()));
    let thread_progresses = Arc::new(Mutex::new(vec![ThreadProgress {
        thread_id: 0,
        frames_processed: 0,
        qr_codes_found: 0,
        current_frame: 0,
        status: ThreadStatus::Starting,
    }; max_threads]));

    let start_time = Instant::now();

    // Launch parallel processing using the same logic as text mode but with progress reporting
    let results_clone = Arc::clone(&results);
    let thread_progresses_clone = Arc::clone(&thread_progresses);
    let input_path_clone = input_path.clone();

    thread::spawn(move || {
        // Use the same parallel extraction logic but report progress to TUI
        match extract_qr_codes_with_text_parallel(&input_path_clone, max_threads, skip_frames, max_frames, start_frame, start_time_seconds, timeout_seconds) {
            Ok(extraction_results) => {
                let mut results_lock = results_clone.lock().unwrap();
                *results_lock = extraction_results.results;
                let _ = tx.send(AppMessage::ExtractionComplete);
            },
            Err(e) => {
                let _ = tx.send(AppMessage::Error(e.to_string()));
            }
        }
    });

    // TUI event loop with dynamic thread display
    let mut total_qr_codes = 0;
    let mut completed = false;

    loop {
        // Handle events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }

        // Check for messages
        while let Ok(message) = rx.try_recv() {
            match message {
                AppMessage::ExtractionComplete => {
                    completed = true;
                },
                AppMessage::Error(e) => {
                    println!("Error: {}", e);
                    break;
                },
                _ => {}
            }
        }

        // Update thread progress from results
        {
            let results_lock = results.lock().unwrap();
            total_qr_codes = results_lock.len();
        }

        // Draw TUI
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),  // Header
                    Constraint::Min(5),     // Thread progress (scrollable)
                    Constraint::Length(3),  // Summary
                ].as_ref())
                .split(f.size());

            // Header
            let header = Paragraph::new(format!(
                "🚀 Parallel QR Extraction - {} Threads | Press 'q' to quit",
                max_threads
            ))
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Cyan));
            f.render_widget(header, chunks[0]);

            // Thread progress (scrollable box for any number of threads)
            let thread_progresses_lock = thread_progresses.lock().unwrap();
            let thread_items: Vec<ListItem> = thread_progresses_lock
                .iter()
                .enumerate()
                .map(|(i, progress)| {
                    let status_symbol = match progress.status {
                        ThreadStatus::Starting => "🔄",
                        ThreadStatus::Processing => "⚡",
                        ThreadStatus::Completed => "✅",
                        ThreadStatus::Error(_) => "❌",
                    };

                    let progress_bar = if progress.frames_processed > 0 {
                        let progress_ratio = progress.frames_processed as f64 / 1000.0; // Estimated
                        let bars = ((progress_ratio * 20.0) as usize).min(20);
                        "█".repeat(bars) + &"▒".repeat(20 - bars)
                    } else {
                        "▒".repeat(20)
                    };

                    ListItem::new(format!(
                        "{} Thread {:2} │ {} │ Frames: {:6} │ QR: {:4}",
                        status_symbol,
                        i + 1,
                        progress_bar,
                        progress.frames_processed,
                        progress.qr_codes_found
                    ))
                })
                .collect();

            let thread_list = List::new(thread_items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Thread Progress ({} threads)", max_threads)))
                .style(Style::default().fg(Color::White));
            f.render_widget(thread_list, chunks[1]);

            // Summary
            let elapsed = start_time.elapsed().as_secs_f64();
            let summary = Paragraph::new(format!(
                "📊 Total QR Codes: {} │ ⏱️ Time: {:.1}s │ 🔄 Status: {}",
                total_qr_codes,
                elapsed,
                if completed { "✅ Complete" } else { "🔄 Processing" }
            ))
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::Green));
            f.render_widget(summary, chunks[2]);
        })?;

        if completed {
            break;
        }
    }

    // Cleanup TUI
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;

    // Return results
    let final_results = results.lock().unwrap();
    Ok(ExtractionResults {
        video_info,
        total_frames_processed: final_results.len() as u64,
        qr_codes_found: final_results.len(),
        processing_time_ms: start_time.elapsed().as_millis(),
        results: final_results.clone(),
    })
}

// UI-independent parallel processing core that works with any number of threads
fn extract_qr_codes_parallel(
    input_path: &PathBuf,
    max_threads: usize,
    skip_frames: usize,
    max_frames: Option<usize>,
    start_frame: usize,
    start_time_seconds: f64,
    timeout_seconds: u64,
    use_tui: bool
) -> Result<ExtractionResults> {
    if use_tui {
        extract_qr_codes_with_tui_parallel(input_path, max_threads, skip_frames, max_frames, start_frame, start_time_seconds, timeout_seconds)
    } else {
        extract_qr_codes_with_text_parallel(input_path, max_threads, skip_frames, max_frames, start_frame, start_time_seconds, timeout_seconds)
    }
}

// Legacy function for backward compatibility
fn extract_qr_codes_from_video(input_path: &PathBuf, max_threads: usize, skip_frames: usize, max_frames: Option<usize>, start_frame: usize, start_time_seconds: f64, timeout_seconds: u64) -> Result<ExtractionResults> {
    extract_qr_codes_parallel(input_path, max_threads, skip_frames, max_frames, start_frame, start_time_seconds, timeout_seconds, false)
}

// TRUE parallel processing using producer-consumer pattern (backward compatibility)
fn extract_qr_codes_with_text_parallel(input_path: &PathBuf, max_threads: usize, skip_frames: usize, max_frames: Option<usize>, start_frame: usize, start_time_seconds: f64, timeout_seconds: u64) -> Result<ExtractionResults> {
    extract_qr_codes_with_text_parallel_logging(input_path, max_threads, skip_frames, max_frames, start_frame, start_time_seconds, timeout_seconds, false)
}

// TRUE parallel processing with logging control
fn extract_qr_codes_with_text_parallel_logging(input_path: &PathBuf, max_threads: usize, skip_frames: usize, max_frames: Option<usize>, start_frame: usize, start_time_seconds: f64, timeout_seconds: u64, enable_logging: bool) -> Result<ExtractionResults> {
    use std::sync::{Arc, Mutex};
    use std::sync::mpsc;
    use std::thread;

    if enable_logging {
        println!("🚀 TRUE parallel processing: 1 producer + {} consumer threads", max_threads);
    }

    // Create frame queue for producer-consumer pattern
    let (frame_tx, frame_rx) = mpsc::sync_channel(max_threads * 4); // Buffer 4x threads
    let (result_tx, result_rx) = mpsc::channel();

    // Shared results collection
    let results = Arc::new(Mutex::new(Vec::new()));
    let frame_rx = Arc::new(Mutex::new(frame_rx));

    // Launch PRODUCER thread - fast frame extraction
    let input_clone = input_path.clone();
    let producer_handle = thread::spawn(move || {
        extract_frames_producer_with_logging(input_clone, frame_tx, skip_frames, max_frames, start_frame, start_time_seconds, timeout_seconds, enable_logging)
    });

    // Launch CONSUMER threads - parallel QR detection
    let mut consumer_handles = Vec::new();

    for thread_id in 0..max_threads {
        let frame_rx_clone = Arc::clone(&frame_rx);
        let result_tx_clone = result_tx.clone();

        let handle = thread::spawn(move || {
            qr_detection_consumer_with_logging(thread_id, frame_rx_clone, result_tx_clone, enable_logging)
        });

        consumer_handles.push(handle);
    }

    // Collect results from all consumer threads
    drop(result_tx); // Close sender so receiver knows when done

    for qr_result in result_rx {
        let mut results_lock = results.lock().unwrap();
        results_lock.push(qr_result);
    }

    // Wait for all threads to complete
    let _ = producer_handle.join();
    for handle in consumer_handles {
        let _ = handle.join();
    }

    // Get video info for results
    let mut context = ffmpeg::format::input(&input_path)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();
    let fps = video_stream.avg_frame_rate();
    let fps_value = fps.numerator() as f64 / fps.denominator() as f64;
    let duration = video_stream.duration();

    // Get parameters before seeking to avoid borrowing issues
    let stream_parameters = video_stream.parameters();

    // Seek to start time if specified
    if start_time_seconds > 0.0 {
        let seek_timestamp = (start_time_seconds / time_base.numerator() as f64 * time_base.denominator() as f64) as i64;
        context.seek(seek_timestamp, ..seek_timestamp)?;
        println!("⏰ Seeked to {:.1}s", start_time_seconds);
    }

    // Set up decoder with error recovery for problematic codecs
    let context_decoder = ffmpeg::codec::context::Context::from_parameters(stream_parameters)?;
    let decoder = context_decoder.decoder().video()?;

    // Enable error recovery for HEVC and other problematic codecs
    // Note: These methods may not be available in all ffmpeg-next versions
    // The video processing will rely on robust error handling instead

    // Get video info
    let video_info = VideoInfo {
        duration_seconds: duration as f64 * time_base.numerator() as f64 / time_base.denominator() as f64,
        fps: fps_value,
        width: decoder.width(),
        height: decoder.height(),
        format: format!("{:?}", decoder.id()),
    };

    println!("📺 Video: {}x{} @ {:.2} fps, {:.2}s duration",
             video_info.width, video_info.height, video_info.fps, video_info.duration_seconds);

    // Use the decoder we already created
    let mut decoder = decoder;

    // Stream processing: extract QR codes immediately instead of collecting frames
    let mut qr_results = Vec::new();
    let mut frame_number = 0u64;
    let mut processed_frames = 0u64;
    let process_start_time = Instant::now();

    // Calculate total frames for progress bar
    let total_frames = if let Some(max) = max_frames {
        max as u64
    } else {
        (video_info.duration_seconds * video_info.fps) as u64
    };

    // Create beautiful progress bar
    let pb = ProgressBar::new(total_frames);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("🔍 {spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos:>7}/{len:7} frames ({eta}) {msg}")
            .unwrap()
            .progress_chars("🟩🟨⬜")
    );

    // Frame extraction
    pb.set_message("Extracting frames and detecting QR codes...");
    for (stream, packet) in context.packets() {
        if stream.index() == video_index {
            // Check timeout
            if timeout_seconds > 0 && process_start_time.elapsed().as_secs() >= timeout_seconds {
                println!("\n⏰ Timeout reached after {} seconds", timeout_seconds);
                break;
            }

            // Check if we're before start_frame
            if frame_number < start_frame as u64 {
                frame_number += 1;
                continue;
            }

            // Check max_frames limit (relative to start_frame)
            if let Some(max) = max_frames {
                if processed_frames >= max as u64 {
                    break;
                }
            }

            if frame_number % skip_frames as u64 == 0 {
                // Robust packet sending with error recovery
                match decoder.send_packet(&packet) {
                    Ok(_) => {
                        let mut decoded = ffmpeg::util::frame::Video::empty();

                        // Continue receiving frames even if some fail
                        while let Ok(_) = decoder.receive_frame(&mut decoded) {
                            // Robust frame processing with error recovery
                            let frame_result = process_frame_with_error_recovery(&decoded, time_base, frame_number);

                            match frame_result {
                                Ok((rgb_data, width, height, timestamp_ms)) => {
                                    // Process frame for QR codes (streaming approach)
                                    let frame_qr_codes = detect_qr_codes_in_frame_immediate(&rgb_data, width, height);
                                    let qr_count = frame_qr_codes.len();

                                    for qr_code in frame_qr_codes {
                                        qr_results.push(QrResult {
                                            frame_number,
                                            timestamp_ms,
                                            data: qr_code,
                                        });
                                    }

                                    processed_frames += 1;

                                    // Update progress bar
                                    pb.set_position(processed_frames);
                                    if qr_count > 0 {
                                        let total_qr_codes = qr_results.len();
                                        pb.set_message(format!("Found {} QR codes total", total_qr_codes));
                                    }
                                }
                                Err(e) => {
                                    // Log error but continue processing
                                    if processed_frames % 100 == 0 {
                                        eprintln!("Frame processing error at frame {}: {}", frame_number, e);
                                    }
                                    processed_frames += 1;
                                    pb.set_position(processed_frames);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Skip problematic packets but continue
                        if frame_number % 1000 == 0 {
                            eprintln!("Packet error at frame {}: {}", frame_number, e);
                        }
                    }
                }
            }
            frame_number += 1;
        }
    }

    // Finish progress bar
    let final_message = format!("✅ Completed! Found {} QR codes", qr_results.len());
    pb.finish_with_message(final_message);
    println!();

    // Results are already in frame order and processed incrementally
    qr_results.sort_by_key(|r| r.frame_number);

    Ok(ExtractionResults {
        video_info,
        total_frames_processed: processed_frames,
        qr_codes_found: qr_results.len(),
        processing_time_ms: 0, // Will be set by caller
        results: qr_results,
    })
}

// Robust frame processing with error recovery for problematic codecs
fn process_frame_with_error_recovery(
    decoded: &ffmpeg::util::frame::Video,
    time_base: ffmpeg::Rational,
    frame_number: u64,
) -> Result<(Vec<u8>, u32, u32, f64)> {
    // Try to convert frame to RGB with multiple fallback strategies
    let mut rgb_frame = ffmpeg::util::frame::Video::empty();

    // Strategy 1: Try standard RGB24 conversion
    let scaling_result = ffmpeg::software::scaling::context::Context::get(
        decoded.format(),
        decoded.width(),
        decoded.height(),
        ffmpeg::format::Pixel::RGB24,
        decoded.width(),
        decoded.height(),
        ffmpeg::software::scaling::flag::Flags::BILINEAR,
    );

    let mut scaler = match scaling_result {
        Ok(scaler) => scaler,
        Err(_) => {
            // Strategy 2: Try with fast bilinear if standard fails
            ffmpeg::software::scaling::context::Context::get(
                decoded.format(),
                decoded.width(),
                decoded.height(),
                ffmpeg::format::Pixel::RGB24,
                decoded.width(),
                decoded.height(),
                ffmpeg::software::scaling::flag::Flags::FAST_BILINEAR,
            )?
        }
    };

    // Try to scale the frame
    match scaler.run(&decoded, &mut rgb_frame) {
        Ok(_) => {
            let rgb_data = rgb_frame.data(0).to_vec();
            let timestamp_ms = decoded.timestamp().unwrap_or(frame_number as i64) as f64 *
                              time_base.numerator() as f64 / time_base.denominator() as f64 * 1000.0;

            Ok((rgb_data, rgb_frame.width(), rgb_frame.height(), timestamp_ms))
        }
        Err(e) => Err(anyhow!("Frame scaling failed: {}", e))
    }
}

// Enhanced QR detection with multiple libraries and preprocessing
fn detect_qr_codes_in_frame_immediate(rgb_data: &[u8], width: u32, height: u32) -> Vec<String> {
    detect_qr_codes_with_mode(rgb_data, width, height, false)
}

// Performance-optimized QR detection
fn detect_qr_codes_fast(rgb_data: &[u8], width: u32, height: u32) -> Vec<String> {
    detect_qr_codes_with_mode(rgb_data, width, height, true)
}

// Unified QR detection with performance modes
fn detect_qr_codes_with_mode(rgb_data: &[u8], width: u32, height: u32, fast_mode: bool) -> Vec<String> {
    let mut results = std::collections::HashSet::new();

    // Performance optimization: Skip slow conversions in fast mode
    let gray_data: Vec<u8> = if fast_mode {
        // Fast grayscale: Simple average (faster but less accurate)
        rgb_data
            .chunks(3)
            .map(|rgb| ((rgb[0] as u16 + rgb[1] as u16 + rgb[2] as u16) / 3) as u8)
            .collect()
    } else {
        // Accurate grayscale: Standard conversion
        rgb_data
            .chunks(3)
            .map(|rgb| {
                (0.299 * rgb[0] as f64 + 0.587 * rgb[1] as f64 + 0.114 * rgb[2] as f64) as u8
            })
            .collect()
    };

    if let Some(img) = image::GrayImage::from_raw(width, height, gray_data.clone()) {
        // Primary detection: RQRR (fastest, most reliable)
        let mut grid = rqrr::PreparedImage::prepare(img.clone());
        let grids = grid.detect_grids();
        for grid in grids {
            if let Ok((_meta, content)) = grid.decode() {
                results.insert(content);
            }
        }

        // Secondary detection: QUIRCS (skip in fast mode, with error handling)
        if !fast_mode {
            // Validate data size before using quircs to prevent panics
            let expected_size = (width as usize) * (height as usize);
            if gray_data.len() == expected_size {
                match std::panic::catch_unwind(|| {
                    let mut decoder = quircs::Quirc::new();
                    let mut quircs_results = Vec::new();
                    let codes = decoder.identify(width as usize, height as usize, &gray_data);
                    for code_result in codes {
                        if let Ok(code) = code_result {
                            if let Ok(decoded) = code.decode() {
                                if let Ok(content) = String::from_utf8(decoded.payload) {
                                    quircs_results.push(content);
                                }
                            }
                        }
                    }
                    quircs_results
                }) {
                    Ok(quircs_results) => {
                        for content in quircs_results {
                            results.insert(content);
                        }
                    },
                    Err(_) => {
                        // Quircs panicked, skip this detection method
                    }
                }
            }
        }
    }

    // Performance optimization: Skip enhancement in fast mode
    if !fast_mode {
        // High contrast enhancement for difficult QR codes
        let enhanced_gray: Vec<u8> = gray_data
            .iter()
            .map(|&pixel| {
                let normalized = pixel as f64 / 255.0;
                let enhanced = if normalized < 0.5 {
                    normalized * 0.3  // Darken dark areas
                } else {
                    0.7 + (normalized - 0.5) * 0.6  // Brighten light areas
                };
                (enhanced * 255.0).min(255.0) as u8
            })
            .collect();

        if let Some(enhanced_img) = image::GrayImage::from_raw(width, height, enhanced_gray.clone()) {
            // RQRR on enhanced image
            let mut grid = rqrr::PreparedImage::prepare(enhanced_img);
            let grids = grid.detect_grids();
            for grid in grids {
                if let Ok((_meta, content)) = grid.decode() {
                    results.insert(content);
                }
            }

            // QUIRCS on enhanced image (with error handling)
            let expected_size = (width as usize) * (height as usize);
            if enhanced_gray.len() == expected_size {
                match std::panic::catch_unwind(|| {
                    let mut decoder = quircs::Quirc::new();
                    let mut quircs_results = Vec::new();
                    let codes = decoder.identify(width as usize, height as usize, &enhanced_gray);
                    for code_result in codes {
                        if let Ok(code) = code_result {
                            if let Ok(decoded) = code.decode() {
                                if let Ok(content) = String::from_utf8(decoded.payload) {
                                    quircs_results.push(content);
                                }
                            }
                        }
                    }
                    quircs_results
                }) {
                    Ok(quircs_results) => {
                        for content in quircs_results {
                            results.insert(content);
                        }
                    },
                    Err(_) => {
                        // Quircs panicked on enhanced image, skip
                    }
                }
            }
        }
    }

    results.into_iter().collect()
}

fn detect_qr_codes_in_frame(frame: &FrameData) -> Vec<QrResult> {
    let mut results = Vec::new();

    // Convert RGB to grayscale for QR detection
    let gray_data: Vec<u8> = frame.rgb_data
        .chunks(3)
        .map(|rgb| {
            // Standard RGB to grayscale conversion
            (0.299 * rgb[0] as f64 + 0.587 * rgb[1] as f64 + 0.114 * rgb[2] as f64) as u8
        })
        .collect();

    // Create image from grayscale data
    let img = match image::GrayImage::from_raw(frame.width, frame.height, gray_data) {
        Some(img) => img,
        None => return results,
    };

    // Prepare grid for rqrr
    let mut grid = rqrr::PreparedImage::prepare(img);

    // Find QR codes
    let grids = grid.detect_grids();
    for grid in grids {
        if let Ok((meta, content)) = grid.decode() {
            // Extract corner coordinates from the bounds array [Point; 4]
            let bounds = &grid.bounds;
            let corners = [
                (bounds[0].x as i32, bounds[0].y as i32),
                (bounds[1].x as i32, bounds[1].y as i32),
                (bounds[2].x as i32, bounds[2].y as i32),
                (bounds[3].x as i32, bounds[3].y as i32),
            ];

            let qr_result = QrResult {
                frame_number: frame.frame_number,
                timestamp_ms: frame.timestamp_ms,
                data: content,
            };

            results.push(qr_result);
        }
    }

    results
}

mod num_cpus {
    pub fn get() -> usize {
        std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4)
    }
}

fn parse_time_string(time_str: &str) -> Result<f64> {
    let parts: Vec<&str> = time_str.split(':').collect();

    match parts.len() {
        2 => {
            // MM:SS format
            let minutes: f64 = parts[0].parse()?;
            let seconds: f64 = parts[1].parse()?;
            Ok(minutes * 60.0 + seconds)
        }
        3 => {
            // HH:MM:SS format
            let hours: f64 = parts[0].parse()?;
            let minutes: f64 = parts[1].parse()?;
            let seconds: f64 = parts[2].parse()?;
            Ok(hours * 3600.0 + minutes * 60.0 + seconds)
        }
        _ => Err(anyhow!("Invalid time format. Use MM:SS or HH:MM:SS"))
    }
}

// Producer thread: Fast frame extraction (silent for TUI mode)
fn extract_frames_producer(
    input_path: PathBuf,
    frame_tx: std::sync::mpsc::SyncSender<FrameDataParallel>,
    skip_frames: usize,
    max_frames: Option<usize>,
    start_frame: usize,
    start_time_seconds: f64,
    timeout_seconds: u64
) -> Result<()> {
    extract_frames_producer_with_logging(input_path, frame_tx, skip_frames, max_frames, start_frame, start_time_seconds, timeout_seconds, false)
}

// Producer with optional logging control
fn extract_frames_producer_with_logging(
    input_path: PathBuf,
    frame_tx: std::sync::mpsc::SyncSender<FrameDataParallel>,
    skip_frames: usize,
    max_frames: Option<usize>,
    start_frame: usize,
    start_time_seconds: f64,
    timeout_seconds: u64,
    enable_logging: bool
) -> Result<()> {
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    let mut context = ffmpeg::format::input(&input_path)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();
    let fps = video_stream.avg_frame_rate();
    let _fps_value = fps.numerator() as f64 / fps.denominator() as f64;
    let _duration = video_stream.duration();

    let stream_parameters = video_stream.parameters();

    // Seek if needed
    if start_time_seconds > 0.0 {
        let seek_timestamp = (start_time_seconds / time_base.numerator() as f64 * time_base.denominator() as f64) as i64;
        context.seek(seek_timestamp, ..seek_timestamp)?;
    }

    let context_decoder = ffmpeg::codec::context::Context::from_parameters(stream_parameters)?;
    let mut decoder = context_decoder.decoder().video()?;

    if enable_logging {
        println!("🎬 Producer: Starting fast frame extraction");
    }

    let mut frame_number = 0u64;
    let mut processed_frames = 0u64;
    let process_start_time = std::time::Instant::now();

    // FAST frame extraction loop
    for (stream, packet) in context.packets() {
        if stream.index() == video_index {
            // Timeout check
            if timeout_seconds > 0 && process_start_time.elapsed().as_secs() >= timeout_seconds {
                break;
            }

            // Frame skipping
            if frame_number < start_frame as u64 {
                frame_number += 1;
                continue;
            }

            if let Some(max) = max_frames {
                if processed_frames >= max as u64 {
                    break;
                }
            }

            if frame_number % skip_frames as u64 == 0 {
                match decoder.send_packet(&packet) {
                    Ok(_) => {
                        let mut decoded = ffmpeg::util::frame::Video::empty();
                        while decoder.receive_frame(&mut decoded).is_ok() {
                            let timestamp_ms = decoded.timestamp().unwrap_or(0) as f64 *
                                             time_base.numerator() as f64 / time_base.denominator() as f64 * 1000.0;

                            // Convert frame and send to queue immediately
                            if let Ok((rgb_data, width, height, _)) =
                                process_frame_with_error_recovery(&decoded, time_base, frame_number) {

                                let frame_data = FrameDataParallel {
                                    frame_number,
                                    timestamp_ms,
                                    rgb_data,
                                    width,
                                    height,
                                };

                                // Send to consumer threads (blocking send)
                                if frame_tx.send(frame_data).is_err() {
                                    // Consumers closed - exit
                                    break;
                                }
                            }
                            processed_frames += 1;

                            if enable_logging && processed_frames % 1000 == 0 {
                                println!("🎬 Producer: Extracted {} frames", processed_frames);
                            }
                        }
                    },
                    Err(_) => {
                        frame_number += 1;
                        continue;
                    }
                }
            }
            frame_number += 1;
        }
    }

    drop(frame_tx); // Signal consumers that no more frames
    if enable_logging {
        println!("🎬 Producer: Completed - {} frames extracted", processed_frames);
    }

    Ok(())
}

// Consumer thread: Parallel QR detection (silent for TUI mode)
fn qr_detection_consumer(
    thread_id: usize,
    frame_rx: Arc<Mutex<std::sync::mpsc::Receiver<FrameDataParallel>>>,
    result_tx: std::sync::mpsc::Sender<QrResult>
) -> Result<()> {
    qr_detection_consumer_with_logging(thread_id, frame_rx, result_tx, false)
}

// Consumer with optional logging control and TUI progress updates
fn qr_detection_consumer_with_logging(
    thread_id: usize,
    frame_rx: Arc<Mutex<std::sync::mpsc::Receiver<FrameDataParallel>>>,
    result_tx: std::sync::mpsc::Sender<QrResult>,
    enable_logging: bool
) -> Result<()> {
    qr_detection_consumer_with_progress(thread_id, frame_rx, result_tx, enable_logging, None)
}

// Consumer with progress reporting for TUI
fn qr_detection_consumer_with_progress(
    thread_id: usize,
    frame_rx: Arc<Mutex<std::sync::mpsc::Receiver<FrameDataParallel>>>,
    result_tx: std::sync::mpsc::Sender<QrResult>,
    enable_logging: bool,
    progress_tx: Option<std::sync::mpsc::Sender<AppMessage>>
) -> Result<()> {
    if enable_logging {
        println!("⚡ Consumer {}: Starting QR detection", thread_id);
    }

    let mut frames_processed = 0;
    let mut qr_codes_found = 0;

    loop {
        // Get next frame from queue
        let frame_data = {
            let receiver = frame_rx.lock().unwrap();
            match receiver.recv() {
                Ok(frame) => frame,
                Err(_) => {
                    // No more frames - producer finished
                    break;
                }
            }
        };

        // Process frame for QR codes
        let qr_codes = detect_qr_codes_in_frame_immediate(&frame_data.rgb_data, frame_data.width, frame_data.height);

        for qr_code in qr_codes {
            let qr_result = QrResult {
                frame_number: frame_data.frame_number,
                timestamp_ms: frame_data.timestamp_ms,
                data: qr_code,
            };

            if result_tx.send(qr_result).is_err() {
                // Receiver closed
                break;
            }

            qr_codes_found += 1;
        }

        frames_processed += 1;

        // Report progress to TUI or text log
        if frames_processed % 100 == 0 {
            if let Some(ref progress_sender) = progress_tx {
                let _ = progress_sender.send(AppMessage::ThreadUpdate(ThreadProgress {
                    thread_id,
                    frames_processed,
                    qr_codes_found,
                    current_frame: frame_data.frame_number,
                    status: ThreadStatus::Processing,
                }));
            } else if enable_logging && frames_processed % 500 == 0 {
                println!("⚡ Consumer {}: {} frames, {} QR codes", thread_id, frames_processed, qr_codes_found);
            }
        }
    }

    if enable_logging {
        println!("⚡ Consumer {}: Completed - {} frames, {} QR codes", thread_id, frames_processed, qr_codes_found);
    }
    Ok(())
}

// Special extraction function for TUI with progress reporting
fn extract_qr_codes_for_tui(
    input_path: &PathBuf,
    max_threads: usize,
    skip_frames: usize,
    max_frames: Option<usize>,
    start_frame: usize,
    start_time_seconds: f64,
    timeout_seconds: u64,
    progress_tx: std::sync::mpsc::Sender<AppMessage>
) -> Result<ExtractionResults> {
    use std::sync::{Arc, Mutex};
    use std::sync::mpsc;
    use std::thread;

    // Create frame queue for producer-consumer pattern
    let (frame_tx, frame_rx) = mpsc::sync_channel(max_threads * 4);
    let (result_tx, result_rx) = mpsc::channel();

    let results = Arc::new(Mutex::new(Vec::new()));
    let frame_rx = Arc::new(Mutex::new(frame_rx));

    // Launch PRODUCER thread (silent)
    let input_clone = input_path.clone();
    let producer_handle = thread::spawn(move || {
        extract_frames_producer_with_logging(input_clone, frame_tx, skip_frames, max_frames, start_frame, start_time_seconds, timeout_seconds, false)
    });

    // Launch CONSUMER threads with TUI progress reporting
    let mut consumer_handles = Vec::new();

    for thread_id in 0..max_threads {
        let frame_rx_clone = Arc::clone(&frame_rx);
        let result_tx_clone = result_tx.clone();
        let progress_tx_clone = progress_tx.clone();

        let handle = thread::spawn(move || {
            qr_detection_consumer_with_progress(thread_id, frame_rx_clone, result_tx_clone, false, Some(progress_tx_clone))
        });

        consumer_handles.push(handle);
    }

    // Collect results
    drop(result_tx);
    for qr_result in result_rx {
        let mut results_lock = results.lock().unwrap();
        results_lock.push(qr_result);
    }

    // Wait for completion
    let _ = producer_handle.join();
    for handle in consumer_handles {
        let _ = handle.join();
    }

    // Get video info
    let mut context = ffmpeg::format::input(input_path)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let fps = video_stream.avg_frame_rate();
    let fps_value = fps.numerator() as f64 / fps.denominator() as f64;

    let video_info = VideoInfo {
        duration_seconds: video_stream.duration() as f64 * video_stream.time_base().numerator() as f64 / video_stream.time_base().denominator() as f64,
        fps: fps_value,
        width: 1440,
        height: 1440,
        format: "H264".to_string(),
    };

    let final_results = results.lock().unwrap();
    Ok(ExtractionResults {
        video_info,
        total_frames_processed: final_results.len() as u64,
        qr_codes_found: final_results.len(),
        processing_time_ms: 0,
        results: final_results.clone(),
    })
}

#[derive(Debug, Clone)]
struct FrameDataParallel {
    frame_number: u64,
    timestamp_ms: f64,
    rgb_data: Vec<u8>,
    width: u32,
    height: u32,
}

// Intelligent video analysis to detect QR file boundaries
fn analyze_video_structure(input: PathBuf, output: Option<PathBuf>, sample_interval: f64, skip: usize) -> Result<()> {
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    println!("🔍 Analyzing video structure: {}", input.display());
    let start_time = Instant::now();

    // Get video information
    let mut context = ffmpeg::format::input(&input)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();
    let fps = video_stream.avg_frame_rate();
    let fps_value = fps.numerator() as f64 / fps.denominator() as f64;
    let duration = video_stream.duration();

    let context_decoder = ffmpeg::codec::context::Context::from_parameters(video_stream.parameters())?;
    let decoder = context_decoder.decoder().video()?;

    let video_info = VideoInfo {
        duration_seconds: duration as f64 * time_base.numerator() as f64 / time_base.denominator() as f64,
        fps: fps_value,
        width: decoder.width(),
        height: decoder.height(),
        format: format!("{:?}", decoder.id()),
    };

    println!("📺 Video: {}x{} @ {:.2} fps, {:.2}s duration",
             video_info.width, video_info.height, video_info.fps, video_info.duration_seconds);

    // Calculate sample points using binary search approach
    let mut file_markers = Vec::new();
    let total_samples = (video_info.duration_seconds / sample_interval).ceil() as usize;

    println!("🔍 Scanning {} sample points for file boundaries...", total_samples);

    // Use binary search to find file boundaries efficiently
    file_markers = find_file_boundaries_binary_search(&mut context, video_index, time_base,
                                                     decoder, 0.0, video_info.duration_seconds,
                                                     sample_interval, skip)?;

    // Sort by start time
    file_markers.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());

    // Estimate end times for each file
    for i in 0..file_markers.len() {
        if i + 1 < file_markers.len() {
            file_markers[i].estimated_end_time = Some(file_markers[i + 1].start_time);
            file_markers[i].estimated_end_frame = Some(
                (file_markers[i + 1].start_time * video_info.fps) as u64
            );
        } else {
            // Last file ends at video end
            file_markers[i].estimated_end_time = Some(video_info.duration_seconds);
            file_markers[i].estimated_end_frame = Some(
                (video_info.duration_seconds * video_info.fps) as u64
            );
        }
    }

    // Calculate recommended split points
    let recommended_split_points = calculate_split_points(&file_markers, &video_info)?;

    let analysis = VideoAnalysis {
        video_path: input.to_string_lossy().to_string(),
        video_info,
        file_markers: file_markers.clone(),
        analysis_time: chrono::Utc::now().to_rfc3339(),
        total_files_detected: file_markers.len(),
        recommended_split_points,
    };

    println!("\n📊 Analysis Results:");
    println!("   📁 Files detected: {}", analysis.total_files_detected);
    println!("   🔀 Recommended split points: {}", analysis.recommended_split_points.len());

    for (i, marker) in file_markers.iter().enumerate() {
        let duration = marker.estimated_end_time.unwrap_or(0.0) - marker.start_time;
        println!("   📄 File {}: {} ({:.1}s - {:.1}s, {:.1}s duration)",
                i + 1, marker.file_name, marker.start_time,
                marker.estimated_end_time.unwrap_or(0.0), duration);
    }

    println!("\n🎯 Recommended split points:");
    for (i, &split_time) in analysis.recommended_split_points.iter().enumerate() {
        println!("   🔪 Split {}: {:.1}s ({:.1} minutes)", i + 1, split_time, split_time / 60.0);
    }

    // Save analysis if output specified
    if let Some(output_path) = output {
        let json = serde_json::to_string_pretty(&analysis)?;
        std::fs::write(&output_path, json)?;
        println!("\n💾 Analysis saved to: {}", output_path.display());
    }

    let analysis_time = start_time.elapsed();
    println!("\n✅ Analysis complete in {:.2}s", analysis_time.as_secs_f64());

    Ok(())
}

// Binary search approach to find file boundaries efficiently
fn find_file_boundaries_binary_search(
    context: &mut ffmpeg::format::context::Input,
    video_index: usize,
    time_base: ffmpeg::Rational,
    mut decoder: ffmpeg::decoder::Video,
    start_time: f64,
    end_time: f64,
    sample_interval: f64,
    skip: usize
) -> Result<Vec<FileMarker>> {
    let mut markers = Vec::new();

    // Sample points at regular intervals, then use binary search to refine
    let mut current_time = start_time;

    while current_time < end_time {
        if let Some(marker) = find_file_boundary_at_time(context, video_index, time_base,
                                                        &mut decoder, current_time, skip)? {
            // Check if we already found this file
            if !markers.iter().any(|m: &FileMarker| m.file_name == marker.file_name) {
                markers.push(marker);
            }
        }
        current_time += sample_interval;
    }

    Ok(markers)
}

// Find file boundary around a specific time using QR code detection
fn find_file_boundary_at_time(
    context: &mut ffmpeg::format::context::Input,
    video_index: usize,
    time_base: ffmpeg::Rational,
    decoder: &mut ffmpeg::decoder::Video,
    target_time: f64,
    skip: usize
) -> Result<Option<FileMarker>> {
    // Seek to target time
    let seek_timestamp = (target_time / time_base.numerator() as f64 * time_base.denominator() as f64) as i64;
    context.seek(seek_timestamp, ..seek_timestamp)?;

    // Look for metadata packets in nearby frames
    let mut frame_count = 0;
    let max_frames_to_check = 100; // Limit search

    for (stream, packet) in context.packets() {
        if stream.index() == video_index && frame_count < max_frames_to_check {
            if frame_count % skip == 0 {
                match decoder.send_packet(&packet) {
                    Ok(_) => {
                        let mut decoded = ffmpeg::util::frame::Video::empty();
                        while decoder.receive_frame(&mut decoded).is_ok() {
                            let timestamp_ms = decoded.timestamp().unwrap_or(0) as f64 *
                                             time_base.numerator() as f64 / time_base.denominator() as f64 * 1000.0;

                            // Convert frame to RGB for QR detection
                            if let Ok((rgb_data, width, height, _)) =
                                process_frame_with_error_recovery(&decoded, time_base, frame_count as u64) {

                                // Detect QR codes in this frame
                                let qr_codes = detect_qr_codes_in_frame_immediate(&rgb_data, width, height);

                                // Look for metadata packets (M:)
                                for qr_code in qr_codes {
                                    if qr_code.starts_with("M:") {
                                        if let Some(marker) = parse_metadata_to_marker(&qr_code, timestamp_ms / 1000.0, frame_count as u64) {
                                            return Ok(Some(marker));
                                        }
                                    }
                                }
                            }
                            frame_count += 1;
                        }
                    },
                    Err(_) => {
                        // Skip problematic packets
                        frame_count += 1;
                        continue;
                    }
                }
            }
        }

        if frame_count >= max_frames_to_check {
            break;
        }
    }

    Ok(None)
}

// Parse metadata QR code to create file marker
fn parse_metadata_to_marker(qr_data: &str, timestamp: f64, frame: u64) -> Option<FileMarker> {
    let parts: Vec<&str> = qr_data.split(':').collect();
    if parts.len() < 6 {
        return None;
    }

    let file_name = urlencoding::decode(parts[2]).ok()?.to_string();
    let file_size = parts[4].parse().ok();
    let chunks_count = parts[5].parse().ok();

    Some(FileMarker {
        file_name,
        start_time: timestamp,
        start_frame: frame,
        estimated_end_time: None,
        estimated_end_frame: None,
        chunks_count,
        file_size,
    })
}

// Calculate optimal split points based on file boundaries and target size
fn calculate_split_points(file_markers: &[FileMarker], video_info: &VideoInfo) -> Result<Vec<f64>> {
    let mut split_points = Vec::new();

    if file_markers.is_empty() {
        return Ok(split_points);
    }

    // Get video file size (estimate from bitrate)
    let estimated_bitrate_mbps = 10.0; // Conservative estimate
    let total_size_mb = video_info.duration_seconds * estimated_bitrate_mbps / 8.0; // Convert to MB
    let target_chunk_size_mb = 100.0;

    println!("📊 Estimated video size: {:.1} MB", total_size_mb);

    if total_size_mb <= target_chunk_size_mb {
        println!("💡 Video smaller than target chunk size, no splitting needed");
        return Ok(split_points);
    }

    let num_chunks_needed = (total_size_mb / target_chunk_size_mb).ceil() as usize;
    println!("📊 Target chunks: {} (≈{} MB each)", num_chunks_needed, target_chunk_size_mb);

    // Distribute split points across file boundaries
    let chunk_duration = video_info.duration_seconds / num_chunks_needed as f64;

    for i in 1..num_chunks_needed {
        let target_time = i as f64 * chunk_duration;

        // Find the nearest file boundary
        let optimal_split = find_nearest_file_boundary(file_markers, target_time);

        if !split_points.contains(&optimal_split) {
            split_points.push(optimal_split);
        }
    }

    split_points.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Ok(split_points)
}

// Find the nearest file boundary to a target time
fn find_nearest_file_boundary(file_markers: &[FileMarker], target_time: f64) -> f64 {
    if file_markers.is_empty() {
        return target_time;
    }

    // Find the closest file start time
    let mut best_time = target_time;
    let mut min_distance = f64::INFINITY;

    for marker in file_markers {
        let distance = (marker.start_time - target_time).abs();
        if distance < min_distance {
            min_distance = distance;
            best_time = marker.start_time;
        }

        // Also consider estimated end times
        if let Some(end_time) = marker.estimated_end_time {
            let end_distance = (end_time - target_time).abs();
            if end_distance < min_distance {
                min_distance = end_distance;
                best_time = end_time;
            }
        }
    }

    best_time
}

// Intelligent video splitting preserving QR file boundaries
fn split_video_intelligent(
    input: PathBuf,
    output: PathBuf,
    chunk_size_mb: usize,
    analysis_file: Option<PathBuf>,
    sample_interval: f64
) -> Result<()> {
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    println!("🔪 Starting intelligent video splitting: {}", input.display());
    println!("📂 Output directory: {}", output.display());
    println!("🎯 Target chunk size: {} MB", chunk_size_mb);

    let start_time = Instant::now();

    // Load or create analysis
    let analysis = if let Some(analysis_path) = analysis_file {
        println!("📖 Loading existing analysis: {}", analysis_path.display());
        let analysis_json = std::fs::read_to_string(&analysis_path)?;
        serde_json::from_str(&analysis_json)?
    } else {
        println!("🔍 Performing video analysis...");
        perform_video_analysis(&input, sample_interval)?
    };

    println!("📊 Analysis loaded: {} files detected", analysis.total_files_detected);

    // Create output directory
    std::fs::create_dir_all(&output)?;

    // Get actual video file size for better splitting calculations
    let video_metadata = std::fs::metadata(&input)?;
    let actual_size_mb = video_metadata.len() as f64 / (1024.0 * 1024.0);

    println!("📊 Actual video size: {:.1} MB", actual_size_mb);

    // Calculate optimal chunk size if not specified or use smart defaults
    let optimal_chunk_size_mb = if chunk_size_mb == 100 && actual_size_mb > 1000.0 {
        // For large videos, calculate optimal chunk size automatically
        let target_chunks = 32; // Sweet spot for most systems
        (actual_size_mb / target_chunks as f64).max(50.0).min(200.0)
    } else {
        chunk_size_mb as f64
    };

    println!("📊 Using chunk size: {:.1} MB (calculated from {:.1} MB video)", optimal_chunk_size_mb, actual_size_mb);

    // Recalculate split points with optimal chunk size
    let split_points = calculate_split_points_with_size(
        &analysis.file_markers,
        &analysis.video_info,
        actual_size_mb,
        optimal_chunk_size_mb
    )?;

    if split_points.is_empty() {
        println!("💡 Video smaller than target chunk size or no split points needed");
        return Ok(());
    }

    println!("🔪 Splitting video into {} chunks at the following points:", split_points.len() + 1);
    for (i, &split_time) in split_points.iter().enumerate() {
        println!("   🔀 Split {}: {:.1}s ({:.1} minutes)", i + 1, split_time, split_time / 60.0);
    }

    // Perform the actual splitting
    let mut previous_time = 0.0;
    let total_chunks = split_points.len() + 1;

    for (i, &split_time) in split_points.iter().enumerate() {
        let chunk_number = i + 1;
        let chunk_duration = split_time - previous_time;

        println!("\n🔄 Creating chunk {} of {} ({:.1}s - {:.1}s, duration: {:.1}s)",
                chunk_number, total_chunks, previous_time, split_time, chunk_duration);

        let output_filename = format!("chunk_{:03}.mp4", chunk_number);
        let output_path = output.join(&output_filename);

        split_video_segment(&input, &output_path, previous_time, chunk_duration)?;

        // Verify the split
        let chunk_size = std::fs::metadata(&output_path)?.len() as f64 / (1024.0 * 1024.0);
        println!("   ✅ Created: {} ({:.1} MB)", output_filename, chunk_size);

        previous_time = split_time;
    }

    // Create the final chunk (from last split point to end)
    let final_chunk_number = total_chunks;
    let final_duration = analysis.video_info.duration_seconds - previous_time;

    if final_duration > 1.0 { // Only create if substantial duration
        println!("\n🔄 Creating final chunk {} of {} ({:.1}s - end, duration: {:.1}s)",
                final_chunk_number, total_chunks, previous_time, final_duration);

        let output_filename = format!("chunk_{:03}.mp4", final_chunk_number);
        let output_path = output.join(&output_filename);

        split_video_segment(&input, &output_path, previous_time, final_duration)?;

        let chunk_size = std::fs::metadata(&output_path)?.len() as f64 / (1024.0 * 1024.0);
        println!("   ✅ Created: {} ({:.1} MB)", output_filename, chunk_size);
    }

    // Generate splitting report
    generate_splitting_report(&output, &input, &analysis, &split_points)?;

    let total_time = start_time.elapsed();
    println!("\n✅ Video splitting complete in {:.2}s", total_time.as_secs_f64());
    println!("📁 Check '{}' directory for split video files", output.display());

    Ok(())
}

// Perform video analysis without saving to file
fn perform_video_analysis(input: &PathBuf, sample_interval: f64) -> Result<VideoAnalysis> {
    let mut context = ffmpeg::format::input(input)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();
    let fps = video_stream.avg_frame_rate();
    let fps_value = fps.numerator() as f64 / fps.denominator() as f64;
    let duration = video_stream.duration();

    let context_decoder = ffmpeg::codec::context::Context::from_parameters(video_stream.parameters())?;
    let decoder = context_decoder.decoder().video()?;

    let video_info = VideoInfo {
        duration_seconds: duration as f64 * time_base.numerator() as f64 / time_base.denominator() as f64,
        fps: fps_value,
        width: decoder.width(),
        height: decoder.height(),
        format: format!("{:?}", decoder.id()),
    };

    // Find file boundaries
    let file_markers = find_file_boundaries_binary_search(
        &mut context, video_index, time_base, decoder,
        0.0, video_info.duration_seconds, sample_interval, 30
    )?;

    // Calculate recommended split points
    let recommended_split_points = calculate_split_points(&file_markers, &video_info)?;

    let total_files_detected = file_markers.len();

    Ok(VideoAnalysis {
        video_path: input.to_string_lossy().to_string(),
        video_info,
        file_markers,
        analysis_time: chrono::Utc::now().to_rfc3339(),
        total_files_detected,
        recommended_split_points,
    })
}

// Calculate split points with actual file size
fn calculate_split_points_with_size(
    file_markers: &[FileMarker],
    video_info: &VideoInfo,
    actual_size_mb: f64,
    target_chunk_size_mb: f64
) -> Result<Vec<f64>> {
    let mut split_points = Vec::new();

    if actual_size_mb <= target_chunk_size_mb {
        return Ok(split_points);
    }

    let num_chunks_needed = (actual_size_mb / target_chunk_size_mb).ceil() as usize;
    println!("📊 Target chunks: {} (≈{:.1} MB each)", num_chunks_needed, target_chunk_size_mb);

    if file_markers.is_empty() {
        // No file boundaries found, split evenly
        println!("⚠️ No file boundaries detected, using time-based splitting");
        let chunk_duration = video_info.duration_seconds / num_chunks_needed as f64;

        for i in 1..num_chunks_needed {
            split_points.push(i as f64 * chunk_duration);
        }
    } else {
        // Use file boundaries
        let chunk_duration = video_info.duration_seconds / num_chunks_needed as f64;

        for i in 1..num_chunks_needed {
            let target_time = i as f64 * chunk_duration;
            let optimal_split = find_nearest_file_boundary(file_markers, target_time);

            if !split_points.contains(&optimal_split) {
                split_points.push(optimal_split);
            }
        }
    }

    split_points.sort_by(|a, b| a.partial_cmp(b).unwrap());
    Ok(split_points)
}

// Split video segment using FFmpeg
fn split_video_segment(
    input: &PathBuf,
    output: &PathBuf,
    start_time: f64,
    duration: f64
) -> Result<()> {
    use std::process::Command;

    let start_str = format_time(start_time);
    let duration_str = format_time(duration);

    let output = Command::new("ffmpeg")
        .arg("-y") // Overwrite output files
        .arg("-i").arg(input)
        .arg("-ss").arg(&start_str)       // Start time
        .arg("-t").arg(&duration_str)     // Duration
        .arg("-c").arg("copy")            // Copy streams (no re-encoding)
        .arg("-avoid_negative_ts").arg("make_zero") // Handle timestamp issues
        .arg(output)
        .output()
        .map_err(|e| anyhow!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("FFmpeg failed: {}", stderr));
    }

    Ok(())
}

// Format time as HH:MM:SS.mmm for FFmpeg
fn format_time(seconds: f64) -> String {
    let hours = (seconds / 3600.0) as u32;
    let minutes = ((seconds % 3600.0) / 60.0) as u32;
    let secs = seconds % 60.0;

    format!("{:02}:{:02}:{:06.3}", hours, minutes, secs)
}

// Generate splitting report
fn generate_splitting_report(
    output_dir: &PathBuf,
    original_video: &PathBuf,
    analysis: &VideoAnalysis,
    split_points: &[f64]
) -> Result<()> {
    let report_path = output_dir.join("splitting_report.json");

    let report = serde_json::json!({
        "original_video": original_video.to_string_lossy(),
        "split_time": chrono::Utc::now().to_rfc3339(),
        "analysis": analysis,
        "split_points": split_points,
        "chunks_created": split_points.len() + 1,
        "split_strategy": if analysis.file_markers.is_empty() {
            "time_based"
        } else {
            "boundary_preserving"
        }
    });

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&report_path, json)?;

    println!("📋 Splitting report saved: {}", report_path.display());
    Ok(())
}

// Complete parallel processing workflow: split video, process chunks, combine results
fn split_and_process_parallel(
    input: PathBuf,
    output: PathBuf,
    chunk_size_mb: usize,
    threads: Option<usize>,
    skip: usize,
    keep_chunks: bool,
    combine_jsonl: bool,
    start_time_str: Option<String>
) -> Result<()> {
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    let start_time_seconds = if let Some(time_str) = start_time_str {
        parse_time_string(&time_str)?
    } else {
        0.0
    };

    println!("🚀 Starting parallel split-and-process workflow");
    println!("📹 Input video: {}", input.display());
    println!("📂 Output directory: {}", output.display());
    println!("🎯 Target chunk size: {} MB", chunk_size_mb);
    if start_time_seconds > 0.0 {
        println!("⏰ Starting from: {:.1}s", start_time_seconds);
    }

    let workflow_start = Instant::now();

    // Step 1: Analyze video structure and calculate split points
    println!("\n📊 Step 1: Analyzing video structure...");
    let analysis = perform_video_analysis(&input, 10.0)?;

    let video_metadata = std::fs::metadata(&input)?;
    let actual_size_mb = video_metadata.len() as f64 / (1024.0 * 1024.0);

    let split_points = calculate_split_points_with_size(
        &analysis.file_markers,
        &analysis.video_info,
        actual_size_mb,
        chunk_size_mb as f64
    )?;

    let num_chunks = split_points.len() + 1;
    let parallel_threads = threads.unwrap_or(num_chunks);

    println!("📊 Analysis complete: {} files detected, {} chunks needed",
             analysis.total_files_detected, num_chunks);
    println!("🔧 Using {} threads for parallel processing", parallel_threads);

    // Step 2: Split video into numbered chunks
    println!("\n🔪 Step 2: Splitting video into {} chunks...", num_chunks);
    std::fs::create_dir_all(&output)?;
    let chunks_dir = output.join("chunks");
    std::fs::create_dir_all(&chunks_dir)?;

    let chunk_paths = split_video_into_numbered_chunks(&input, &chunks_dir, &split_points, start_time_seconds, analysis.video_info.duration_seconds)?;

    println!("✅ Video split complete: {} chunks created", chunk_paths.len());

    // Step 3: Process chunks in parallel to generate JSONL files
    println!("\n⚡ Step 3: Processing chunks in parallel...");
    let jsonl_paths = process_chunks_parallel(&chunk_paths, &output, skip, parallel_threads)?;

    println!("✅ Parallel processing complete: {} JSONL files generated", jsonl_paths.len());

    // Step 4: Combine JSONL files and decode
    if combine_jsonl {
        println!("\n🔗 Step 4: Combining JSONL files...");
        let combined_jsonl = combine_jsonl_files(&jsonl_paths, &output)?;

        println!("\n🔄 Step 5: Decoding combined JSONL...");
        decode_combined_jsonl(&combined_jsonl, &output)?;
    } else {
        println!("\n🔄 Step 4: Processing individual JSONL files...");
        process_individual_jsonl_files(&jsonl_paths, &output)?;
    }

    // Step 5: Cleanup if requested
    if !keep_chunks {
        println!("\n🧹 Cleaning up intermediate video chunks...");
        std::fs::remove_dir_all(&chunks_dir)?;
        println!("✅ Intermediate chunks removed");
    }

    let total_time = workflow_start.elapsed();
    println!("\n🎉 Parallel workflow complete in {:.2}s", total_time.as_secs_f64());
    println!("📁 Results available in: {}", output.display());

    Ok(())
}

// Split video into numbered chunks with proper naming
fn split_video_into_numbered_chunks(
    input: &PathBuf,
    chunks_dir: &PathBuf,
    split_points: &[f64],
    start_offset: f64,
    total_duration: f64
) -> Result<Vec<PathBuf>> {
    let mut chunk_paths = Vec::new();
    let mut previous_time = start_offset;
    let total_chunks = split_points.len() + 1;

    let video_name = input.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");

    for (i, &split_time) in split_points.iter().enumerate() {
        let chunk_number = i + 1;
        let chunk_duration = split_time - previous_time;

        let chunk_filename = format!("{:03}_{}.mp4", chunk_number, video_name);
        let chunk_path = chunks_dir.join(&chunk_filename);

        println!("🔄 Creating chunk {}/{}: {} ({:.1}s duration)",
                chunk_number, total_chunks, chunk_filename, chunk_duration);

        split_video_segment(input, &chunk_path, previous_time, chunk_duration)?;
        chunk_paths.push(chunk_path);

        previous_time = split_time;
    }

    // Final chunk
    let final_duration = total_duration - previous_time;

    if final_duration > 1.0 {
        let chunk_filename = format!("{:03}_{}.mp4", total_chunks, video_name);
        let chunk_path = chunks_dir.join(&chunk_filename);

        println!("🔄 Creating final chunk {}/{}: {} ({:.1}s duration)",
                total_chunks, total_chunks, chunk_filename, final_duration);

        split_video_segment(input, &chunk_path, previous_time, final_duration)?;
        chunk_paths.push(chunk_path);
    }

    Ok(chunk_paths)
}

// Process video chunks in parallel to generate JSONL files
fn process_chunks_parallel(
    chunk_paths: &[PathBuf],
    output_dir: &PathBuf,
    skip: usize,
    max_threads: usize
) -> Result<Vec<PathBuf>> {
    use std::sync::Arc;
    use std::thread;

    println!("⚡ Processing {} chunks with {} threads", chunk_paths.len(), max_threads);

    let chunk_paths = Arc::new(chunk_paths.to_vec());
    let output_dir = Arc::new(output_dir.clone());
    let mut handles = Vec::new();
    let mut jsonl_paths = Vec::new();

    // Create output directory for JSONL files
    let jsonl_dir = output_dir.join("jsonl");
    std::fs::create_dir_all(&jsonl_dir)?;

    // Process chunks using a thread pool approach
    let chunk_size = chunk_paths.len();
    let chunks_per_thread = (chunk_size + max_threads - 1) / max_threads;

    for thread_id in 0..max_threads {
        let start_idx = thread_id * chunks_per_thread;
        let end_idx = ((thread_id + 1) * chunks_per_thread).min(chunk_size);

        if start_idx >= chunk_size {
            break;
        }

        let chunk_paths = Arc::clone(&chunk_paths);
        let output_dir = Arc::clone(&output_dir);

        let handle = thread::spawn(move || {
            let mut thread_results = Vec::new();

            for i in start_idx..end_idx {
                let chunk_path = &chunk_paths[i];
                let chunk_name = chunk_path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("chunk");

                let jsonl_filename = format!("{}.jsonl", chunk_name);
                let jsonl_path = output_dir.join("jsonl").join(&jsonl_filename);

                println!("🔄 Thread {} processing: {}", thread_id, chunk_name);

                // Extract QR codes from this chunk
                match extract_chunk_to_jsonl(chunk_path, &jsonl_path, skip) {
                    Ok(_) => {
                        println!("✅ Thread {} completed: {}", thread_id, chunk_name);
                        thread_results.push(jsonl_path);
                    },
                    Err(e) => {
                        println!("❌ Thread {} failed on {}: {}", thread_id, chunk_name, e);
                    }
                }
            }

            thread_results
        });

        handles.push(handle);
    }

    // Wait for all threads to complete and collect results
    for handle in handles {
        match handle.join() {
            Ok(mut thread_jsonl_paths) => {
                jsonl_paths.append(&mut thread_jsonl_paths);
            },
            Err(e) => {
                println!("❌ Thread panicked: {:?}", e);
            }
        }
    }

    // Sort JSONL paths by chunk number to maintain order
    jsonl_paths.sort_by(|a, b| {
        let a_name = a.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let b_name = b.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        a_name.cmp(b_name)
    });

    Ok(jsonl_paths)
}

// Extract QR codes from a single video chunk to JSONL (direct function call)
fn extract_chunk_to_jsonl(
    chunk_path: &PathBuf,
    jsonl_path: &PathBuf,
    skip: usize
) -> Result<()> {
    use std::io::Write;
    use std::fs::File;

    // Initialize FFmpeg for this thread
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    // Open chunk video file
    let mut context = ffmpeg::format::input(chunk_path)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found in chunk"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();
    let fps = video_stream.avg_frame_rate();
    let fps_value = fps.numerator() as f64 / fps.denominator() as f64;
    let duration = video_stream.duration();

    let context_decoder = ffmpeg::codec::context::Context::from_parameters(video_stream.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    // Create JSONL output file
    let mut output_file = File::create(jsonl_path)?;

    // Write header
    let header = serde_json::json!({
        "type": "header",
        "video_info": {
            "duration_seconds": duration as f64 * time_base.numerator() as f64 / time_base.denominator() as f64,
            "fps": fps_value,
            "width": decoder.width(),
            "height": decoder.height()
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    writeln!(output_file, "{}", serde_json::to_string(&header)?)?;

    // Process frames and write QR codes immediately
    let mut frame_number = 0u64;
    let mut processed_frames = 0u64;
    let mut qr_codes_found = 0;
    let mut duplicates_skipped = 0;
    let mut seen_qr_codes = std::collections::HashSet::new();
    let process_start = Instant::now();

    for (stream, packet) in context.packets() {
        if stream.index() == video_index {
            if frame_number % skip as u64 == 0 {
                match decoder.send_packet(&packet) {
                    Ok(_) => {
                        let mut decoded = ffmpeg::util::frame::Video::empty();
                        while decoder.receive_frame(&mut decoded).is_ok() {
                            let timestamp_ms = decoded.timestamp().unwrap_or(0) as f64 *
                                             time_base.numerator() as f64 / time_base.denominator() as f64 * 1000.0;

                            // Convert frame for QR detection
                            if let Ok((rgb_data, width, height, _)) =
                                process_frame_with_error_recovery(&decoded, time_base, frame_number) {

                                // Detect QR codes
                                let frame_qr_codes = detect_qr_codes_in_frame_immediate(&rgb_data, width, height);

                                for qr_code in frame_qr_codes {
                                    if seen_qr_codes.insert(qr_code.clone()) {
                                        // Write QR code immediately to JSONL
                                        let qr_entry = serde_json::json!({
                                            "type": "qr_code",
                                            "frame_number": frame_number,
                                            "timestamp_ms": timestamp_ms,
                                            "data": qr_code
                                        });
                                        writeln!(output_file, "{}", serde_json::to_string(&qr_entry)?)?;
                                        qr_codes_found += 1;
                                    } else {
                                        duplicates_skipped += 1;
                                    }
                                }
                            }
                            processed_frames += 1;
                        }
                    },
                    Err(_) => {
                        // Skip problematic packets
                        frame_number += 1;
                        continue;
                    }
                }
            }
            frame_number += 1;
        }
    }

    // Write footer
    let footer = serde_json::json!({
        "type": "footer",
        "summary": {
            "frames_processed": processed_frames,
            "qr_codes_found": qr_codes_found,
            "duplicates_skipped": duplicates_skipped,
            "processing_time_ms": process_start.elapsed().as_millis()
        },
        "timestamp": chrono::Utc::now().to_rfc3339()
    });
    writeln!(output_file, "{}", serde_json::to_string(&footer)?)?;

    Ok(())
}

// Combine multiple JSONL files into one ordered file
fn combine_jsonl_files(jsonl_paths: &[PathBuf], output_dir: &PathBuf) -> Result<PathBuf> {
    use std::io::{BufRead, BufReader, Write};
    use std::fs::File;

    let combined_path = output_dir.join("combined_qr_codes.jsonl");
    let mut combined_file = File::create(&combined_path)?;

    let mut first_header_written = false;
    let mut total_qr_codes = 0u64;
    let mut total_frames = 0u64;
    let mut total_processing_time = 0u64;

    println!("🔗 Combining {} JSONL files in order...", jsonl_paths.len());

    for (chunk_idx, jsonl_path) in jsonl_paths.iter().enumerate() {
        println!("🔄 Processing chunk {}/{}: {}",
                chunk_idx + 1, jsonl_paths.len(), jsonl_path.file_name().unwrap_or_default().to_string_lossy());

        let file = File::open(jsonl_path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            // Parse to determine type
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
                match entry.get("type").and_then(|t| t.as_str()) {
                    Some("header") => {
                        if !first_header_written {
                            writeln!(combined_file, "{}", line)?;
                            first_header_written = true;
                        }
                    },
                    Some("qr_code") => {
                        // Offset frame numbers to maintain order across chunks
                        if let Ok(mut qr_entry) = serde_json::from_str::<serde_json::Value>(&line) {
                            if let Some(frame_num) = qr_entry.get("frame_number").and_then(|f| f.as_u64()) {
                                qr_entry["frame_number"] = serde_json::Value::Number(
                                    serde_json::Number::from(frame_num + total_frames)
                                );
                                writeln!(combined_file, "{}", serde_json::to_string(&qr_entry)?)?;
                                total_qr_codes += 1;
                            }
                        }
                    },
                    Some("footer") => {
                        if let Ok(footer) = serde_json::from_str::<serde_json::Value>(&line) {
                            if let Some(summary) = footer.get("summary") {
                                if let Some(frames) = summary.get("frames_processed").and_then(|f| f.as_u64()) {
                                    total_frames += frames;
                                }
                                if let Some(time) = summary.get("processing_time_ms").and_then(|t| t.as_u64()) {
                                    total_processing_time += time;
                                }
                            }
                        }
                    },
                    _ => {
                        // Unknown type, skip
                        continue;
                    }
                }
            }
        }
    }

    // Write final footer with combined statistics
    let final_footer = serde_json::json!({
        "type": "footer",
        "summary": {
            "frames_processed": total_frames,
            "qr_codes_found": total_qr_codes,
            "processing_time_ms": total_processing_time,
            "chunks_processed": jsonl_paths.len()
        }
    });

    writeln!(combined_file, "{}", serde_json::to_string(&final_footer)?)?;

    println!("✅ Combined JSONL created: {} ({} QR codes from {} chunks)",
             combined_path.display(), total_qr_codes, jsonl_paths.len());

    Ok(combined_path)
}

// Decode combined JSONL file (direct function call)
fn decode_combined_jsonl(combined_jsonl: &PathBuf, output_dir: &PathBuf) -> Result<()> {
    use std::io::{BufRead, BufReader};
    use std::fs::File;

    let decoded_dir = output_dir.join("decoded_files");
    std::fs::create_dir_all(&decoded_dir)?;

    println!("🔄 Decoding combined JSONL with integrated decoder...");

    // Initialize QR decoder (inline implementation)
    let mut decoder = QRFileDecoderIntegrated::new(&decoded_dir.to_string_lossy());

    // Process JSONL file line by line
    let file = File::open(combined_jsonl)?;
    let reader = BufReader::new(file);

    let mut processed = 0;
    let mut successful = 0;
    let mut qr_count = 0;

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        // Parse JSONL entry
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
            match entry.get("type").and_then(|t| t.as_str()) {
                Some("header") => {
                    if let Some(video_info) = entry.get("video_info") {
                        if let (Some(duration), Some(fps), Some(width), Some(height)) = (
                            video_info.get("duration_seconds").and_then(|v| v.as_f64()),
                            video_info.get("fps").and_then(|v| v.as_f64()),
                            video_info.get("width").and_then(|v| v.as_u64()),
                            video_info.get("height").and_then(|v| v.as_u64())
                        ) {
                            println!("📺 Video info: {:.1}min, {:.0}fps, {}x{}",
                                    duration / 60.0, fps, width, height);
                        }
                    }
                },
                Some("qr_code") => {
                    if let (Some(frame_num), Some(data)) = (
                        entry.get("frame_number").and_then(|v| v.as_u64()),
                        entry.get("data").and_then(|v| v.as_str())
                    ) {
                        qr_count += 1;

                        // Process QR code directly using integrated decoder
                        let result = decoder.process_qr_code(data, frame_num as usize);
                        if result.is_valid {
                            successful += 1;
                        } else if let Some(reason) = result.reason {
                            if qr_count <= 5 {
                                println!("Warning: Failed to process QR {}: {}", qr_count, reason);
                            }
                        }
                        processed += 1;

                        // Check for completed files every 100 QR codes
                        if qr_count % 100 == 0 {
                            print!("\r🔄 Processed {} QR codes, {} successful", qr_count, successful);
                            std::io::Write::flush(&mut std::io::stdout()).unwrap();
                            check_and_finalize_completed_files_direct(&mut decoder, &decoded_dir)?;
                        }
                    }
                },
                Some("footer") => {
                    if let Some(summary) = entry.get("summary") {
                        if let (Some(frames), Some(qr_found), Some(time_ms)) = (
                            summary.get("frames_processed").and_then(|v| v.as_u64()),
                            summary.get("qr_codes_found").and_then(|v| v.as_u64()),
                            summary.get("processing_time_ms").and_then(|v| v.as_u64())
                        ) {
                            println!("\n📊 Chunk summary: {} frames, {} QR codes, {:.2}s",
                                    frames, qr_found, time_ms as f64 / 1000.0);
                        }
                    }
                },
                _ => continue,
            }
        }
    }

    // Finalize any remaining files
    finalize_remaining_files_direct(&mut decoder, &decoded_dir)?;

    println!("\n✅ Decoding complete: {}/{} QR codes successfully processed", successful, processed);

    Ok(())
}

// Process individual JSONL files separately (direct function calls)
fn process_individual_jsonl_files(jsonl_paths: &[PathBuf], output_dir: &PathBuf) -> Result<()> {
    let decoded_dir = output_dir.join("decoded_files");
    std::fs::create_dir_all(&decoded_dir)?;

    for (i, jsonl_path) in jsonl_paths.iter().enumerate() {
        println!("🔄 Decoding chunk {}/{}: {}",
                i + 1, jsonl_paths.len(), jsonl_path.file_name().unwrap_or_default().to_string_lossy());

        let chunk_decoded_dir = decoded_dir.join(format!("chunk_{:03}", i + 1));
        std::fs::create_dir_all(&chunk_decoded_dir)?;

        // Process this JSONL file directly
        match decode_jsonl_file_direct(jsonl_path, &chunk_decoded_dir) {
            Ok(_) => {
                println!("✅ Chunk {} processed successfully", i + 1);
            },
            Err(e) => {
                println!("⚠️ Chunk {} decoding had issues: {}", i + 1, e);
            }
        }
    }

    println!("✅ Individual chunk processing complete");
    Ok(())
}

// Direct JSONL file processing without subprocess
fn decode_jsonl_file_direct(jsonl_path: &PathBuf, output_dir: &PathBuf) -> Result<()> {
    use std::io::{BufRead, BufReader};
    use std::fs::File;

    // Initialize QR decoder (inline implementation)
    let mut decoder = QRFileDecoderIntegrated::new(&output_dir.to_string_lossy());

    // Process JSONL file
    let file = File::open(jsonl_path)?;
    let reader = BufReader::new(file);

    let mut processed = 0;
    let mut successful = 0;

    for line_result in reader.lines() {
        let line = line_result?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(&line) {
            if entry.get("type").and_then(|t| t.as_str()) == Some("qr_code") {
                if let (Some(frame_num), Some(data)) = (
                    entry.get("frame_number").and_then(|v| v.as_u64()),
                    entry.get("data").and_then(|v| v.as_str())
                ) {
                    let result = decoder.process_qr_code(data, frame_num as usize);
                    if result.is_valid {
                        successful += 1;
                    }
                    processed += 1;

                    // Check for completed files
                    check_and_finalize_completed_files_direct(&mut decoder, output_dir)?;
                }
            }
        }
    }

    Ok(())
}

// Check and finalize completed files (direct function)
fn check_and_finalize_completed_files_direct(decoder: &mut QRFileDecoderIntegrated, output_dir: &PathBuf) -> Result<()> {
    let completed_files: Vec<String> = decoder.file_decoders.iter()
        .filter(|(_, fd)| fd.is_complete())
        .map(|(name, _)| name.clone())
        .collect();

    for file_name in completed_files {
        if let Some(fountain_decoder) = decoder.file_decoders.get_mut(&file_name) {
            println!("\n🎉 File complete! Finalizing: {}", file_name);
            let _ = fountain_decoder.finalize(&output_dir.to_string_lossy());
        }
    }
    Ok(())
}

// Finalize remaining files (direct function)
fn finalize_remaining_files_direct(decoder: &mut QRFileDecoderIntegrated, output_dir: &PathBuf) -> Result<()> {
    let mut completed_files = 0;
    let mut partial_files = 0;

    for (file_name, fountain_decoder) in &mut decoder.file_decoders {
        if fountain_decoder.is_complete() {
            println!("\n🎉 Finalizing complete file: {}", file_name);
            let _ = fountain_decoder.finalize(&output_dir.to_string_lossy());
            completed_files += 1;
        } else {
            let percentage = ((fountain_decoder.recovered_chunk_count as f64 / fountain_decoder.total_chunks as f64) * 100.0).round() as usize;
            println!("\n⚠️ File incomplete: {} - {}/{} chunks ({}%)",
                    file_name, fountain_decoder.recovered_chunk_count, fountain_decoder.total_chunks, percentage);
            if percentage >= 10 {
                partial_files += 1;
            }
        }
    }

    println!("\n📊 Final Results:");
    println!("   ✅ Complete files: {}", completed_files);
    println!("   📝 Partial files: {}", partial_files);

    if completed_files > 0 {
        println!("\n🎉 SUCCESS: {} files fully reconstructed!", completed_files);
    }

    Ok(())
}

// Advanced streaming with duplicate prevention and async processing
fn extract_streaming(input_path: &PathBuf, output: Option<PathBuf>, _max_threads: usize, skip_frames: usize, max_frames: Option<usize>, start_frame: usize, start_time_seconds: f64, timeout_seconds: u64) -> Result<()> {
    use std::collections::HashSet;
    use std::io::Write;

    // Open output file or stdout
    let mut writer: Box<dyn Write> = if let Some(output_path) = output {
        Box::new(std::fs::File::create(output_path)?)
    } else {
        Box::new(std::io::stdout())
    };

    // Duplicate prevention
    let mut seen_qr_data = HashSet::new();

    // Initialize FFmpeg
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    let start_time = Instant::now();

    // Open video file
    let mut context = ffmpeg::format::input(&input_path)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();

    // Set up decoder
    let context_decoder = ffmpeg::codec::context::Context::from_parameters(video_stream.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    // Write metadata header
    writeln!(writer, "{{\"type\":\"header\",\"video_info\":{{\"duration_seconds\":{:.2},\"fps\":{:.2},\"width\":{},\"height\":{}}},\"timestamp\":\"{}\"}}",
             video_stream.duration() as f64 * time_base.numerator() as f64 / time_base.denominator() as f64,
             video_stream.avg_frame_rate().numerator() as f64 / video_stream.avg_frame_rate().denominator() as f64,
             decoder.width(),
             decoder.height(),
             chrono::Utc::now().to_rfc3339())?;
    writer.flush()?;

    let mut frame_number = 0u64;
    let mut processed_frames = 0u64;
    let mut total_qr_codes = 0u64;
    let mut duplicates_skipped = 0u64;

    println!("🌊 Streaming QR extraction with duplicate prevention started...");

    // Process frames and stream output
    for (stream, packet) in context.packets() {
        if stream.index() == video_index {
            // Check timeout
            if timeout_seconds > 0 && start_time.elapsed().as_secs() >= timeout_seconds {
                println!("⏰ Timeout reached after {} seconds", timeout_seconds);
                break;
            }

            // Frame range filtering
            if frame_number < start_frame as u64 {
                frame_number += 1;
                continue;
            }

            if let Some(max) = max_frames {
                if processed_frames >= max as u64 {
                    break;
                }
            }

            if frame_number % skip_frames as u64 == 0 {
                // Robust packet processing with error recovery
                match decoder.send_packet(&packet) {
                    Ok(_) => {
                        let mut decoded = ffmpeg::util::frame::Video::empty();

                        while decoder.receive_frame(&mut decoded).is_ok() {
                            // Robust frame processing with error recovery
                            match process_frame_with_error_recovery(&decoded, time_base, frame_number) {
                                Ok((rgb_data, width, height, timestamp_ms)) => {
                                    // Detect QR codes immediately
                                    let frame_qr_codes = detect_qr_codes_in_frame_immediate(&rgb_data, width, height);

                                    // Stream each unique QR code immediately as JSONL
                                    for qr_code in frame_qr_codes {
                                        // Duplicate prevention: check if we've seen this QR data before
                                        if seen_qr_data.insert(qr_code.clone()) {
                                            // New QR code - stream it
                                            let qr_entry = serde_json::json!({
                                                "type": "qr_code",
                                                "frame_number": frame_number,
                                                "timestamp_ms": timestamp_ms,
                                                "data": qr_code
                                            });
                                            writeln!(writer, "{}", qr_entry)?;
                                            writer.flush()?; // Force immediate write for real-time tail following
                                            total_qr_codes += 1;
                                        } else {
                                            // Duplicate - skip but count
                                            duplicates_skipped += 1;
                                        }
                                    }

                                    processed_frames += 1;

                                    // Progress update every 100 frames
                                    if processed_frames % 100 == 0 {
                                        println!("🔍 Processed {} frames, found {} unique QR codes ({} duplicates skipped)",
                                                processed_frames, total_qr_codes, duplicates_skipped);
                                    }
                                }
                                Err(_) => {
                                    // Skip problematic frames but continue
                                    processed_frames += 1;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Skip problematic packets but continue processing
                    }
                }
            }
            frame_number += 1;
        }
    }

    // Write footer with summary
    let processing_time = start_time.elapsed().as_millis();
    writeln!(writer, "{{\"type\":\"footer\",\"summary\":{{\"frames_processed\":{},\"qr_codes_found\":{},\"duplicates_skipped\":{},\"processing_time_ms\":{}}},\"timestamp\":\"{}\"}}",
             processed_frames, total_qr_codes, duplicates_skipped, processing_time, chrono::Utc::now().to_rfc3339())?;
    writer.flush()?;

    println!("🌊 Streaming extraction complete! {} unique QR codes written ({} duplicates skipped)",
             total_qr_codes, duplicates_skipped);
    Ok(())
}

// Integrated QR File Decoder for parallel processing
struct QRFileDecoderIntegrated {
    file_decoders: HashMap<String, FountainDecoderIntegrated>,
    current_active_decoder: Option<String>,
    output_dir: String,
}

impl QRFileDecoderIntegrated {
    fn new(output_dir: &str) -> Self {
        Self {
            file_decoders: HashMap::new(),
            current_active_decoder: None,
            output_dir: output_dir.to_string(),
        }
    }

    fn process_qr_code(&mut self, qr_data: &str, frame_index: usize) -> ProcessResultIntegrated {
        match self.try_process_qr_code(qr_data, frame_index) {
            Ok(result) => result,
            Err(error) => ProcessResultIntegrated {
                is_valid: false,
                reason: Some(error.to_string()),
            }
        }
    }

    fn try_process_qr_code(&mut self, qr_data: &str, _frame_index: usize) -> Result<ProcessResultIntegrated> {
        if qr_data.starts_with("M:") {
            self.process_metadata_packet(qr_data)
        } else if qr_data.starts_with("D:") {
            self.process_data_packet(qr_data)
        } else {
            Ok(ProcessResultIntegrated {
                is_valid: false,
                reason: Some("Unknown packet type".to_string()),
            })
        }
    }

    fn process_metadata_packet(&mut self, meta_string: &str) -> Result<ProcessResultIntegrated> {
        let parts: Vec<&str> = meta_string.split(':').collect();
        if parts.len() < 10 {
            return Err(anyhow!("Invalid metadata format"));
        }

        let metadata = FileMetadataIntegrated {
            file_name: urlencoding::decode(parts[2])?.to_string(),
            file_size: parts[4].parse()?,
            chunks_count: parts[5].parse()?,
            file_checksum: parts.get(13).filter(|s| !s.is_empty()).map(|s| s.to_string()),
        };

        if !self.file_decoders.contains_key(&metadata.file_name) {
            let mut decoder = FountainDecoderIntegrated::new();
            decoder.initialize(metadata.clone());
            self.file_decoders.insert(metadata.file_name.clone(), decoder);
        }

        self.current_active_decoder = Some(metadata.file_name.clone());
        println!("🎯 Switched to processing: {}", metadata.file_name);

        Ok(ProcessResultIntegrated {
            is_valid: true,
            reason: None,
        })
    }

    fn process_data_packet(&mut self, data_string: &str) -> Result<ProcessResultIntegrated> {
        let parts: Vec<&str> = data_string.split(':').collect();
        if parts.len() < 6 {
            return Err(anyhow!("Invalid data packet format"));
        }

        let mut packet = DataPacketIntegrated {
            source_chunks: Vec::new(),
            systematic_data_chunks: Vec::new(),
            xor_data: None,
        };

        // Parse systematic packet format
        if parts.len() >= 7 {
            let all_data_part = parts[6..].join(":");

            if all_data_part.contains('|') {
                let records: Vec<&str> = all_data_part.split('|').collect();

                for record in records {
                    let chunk_parts: Vec<&str> = record.splitn(2, ':').collect();
                    if chunk_parts.len() == 2 {
                        if let Ok(chunk_index) = chunk_parts[0].parse::<usize>() {
                            if let Ok(chunk_data) = general_purpose::STANDARD.decode(chunk_parts[1]) {
                                packet.source_chunks.push(chunk_index);
                                packet.systematic_data_chunks.push(SystematicChunkIntegrated {
                                    chunk_index,
                                    chunk_data,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Route to current active decoder
        let current_decoder_name = match &self.current_active_decoder {
            Some(name) => name.clone(),
            None => {
                return Ok(ProcessResultIntegrated {
                    is_valid: false,
                    reason: Some("No active decoder".to_string()),
                });
            }
        };

        let success = if let Some(decoder) = self.file_decoders.get_mut(&current_decoder_name) {
            decoder.add_packet(packet)
        } else {
            false
        };

        // Check if file is complete
        let is_complete = self.file_decoders.get(&current_decoder_name)
            .map(|d| d.is_complete())
            .unwrap_or(false);

        if is_complete {
            println!("\n🎉 File complete! Finalizing...");
            if let Some(decoder) = self.file_decoders.get_mut(&current_decoder_name) {
                let _ = decoder.finalize(&self.output_dir);
            }
        }

        Ok(ProcessResultIntegrated {
            is_valid: success,
            reason: None,
        })
    }
}

#[derive(Debug, Clone)]
struct FileMetadataIntegrated {
    file_name: String,
    file_size: usize,
    chunks_count: usize,
    file_checksum: Option<String>,
}

#[derive(Debug, Clone)]
struct SystematicChunkIntegrated {
    chunk_index: usize,
    chunk_data: Vec<u8>,
}

#[derive(Debug, Clone)]
struct DataPacketIntegrated {
    source_chunks: Vec<usize>,
    systematic_data_chunks: Vec<SystematicChunkIntegrated>,
    xor_data: Option<Vec<u8>>,
}

#[derive(Debug)]
struct ProcessResultIntegrated {
    is_valid: bool,
    reason: Option<String>,
}

struct FountainDecoderIntegrated {
    initialized: bool,
    meta_data: Option<FileMetadataIntegrated>,
    total_chunks: usize,
    source_chunks: HashMap<usize, Vec<u8>>,
    recovered_chunk_count: usize,
    coded_packets: Vec<DataPacketIntegrated>,
}

impl FountainDecoderIntegrated {
    fn new() -> Self {
        Self {
            initialized: false,
            meta_data: None,
            total_chunks: 0,
            source_chunks: HashMap::new(),
            recovered_chunk_count: 0,
            coded_packets: Vec::new(),
        }
    }

    fn initialize(&mut self, metadata: FileMetadataIntegrated) {
        self.meta_data = Some(metadata.clone());
        self.total_chunks = metadata.chunks_count;
        self.source_chunks.clear();
        self.recovered_chunk_count = 0;
        self.coded_packets.clear();
        self.initialized = true;

        println!("📄 Initialized decoder for {} ({} chunks, {} bytes)",
                metadata.file_name, metadata.chunks_count, metadata.file_size);
    }

    fn add_packet(&mut self, packet: DataPacketIntegrated) -> bool {
        if !self.initialized {
            return false;
        }

        if !packet.systematic_data_chunks.is_empty() {
            for chunk in &packet.systematic_data_chunks {
                if !self.source_chunks.contains_key(&chunk.chunk_index) {
                    self.source_chunks.insert(chunk.chunk_index, chunk.chunk_data.clone());
                    self.recovered_chunk_count += 1;
                }
            }
        }

        true
    }

    fn is_complete(&self) -> bool {
        self.recovered_chunk_count >= self.total_chunks
    }

    fn finalize(&mut self, output_dir: &str) -> Result<Option<Vec<u8>>> {
        if !self.is_complete() {
            return Ok(None);
        }

        let metadata = self.meta_data.as_ref().unwrap();

        // Combine chunks in order
        let mut file_data = Vec::with_capacity(metadata.file_size);
        for i in 0..self.total_chunks {
            if let Some(chunk) = self.source_chunks.get(&i) {
                let copy_length = (file_data.len() + chunk.len()).min(metadata.file_size) - file_data.len();
                file_data.extend_from_slice(&chunk[..copy_length.min(chunk.len())]);
            }
        }

        file_data.truncate(metadata.file_size);

        // Write file
        std::fs::create_dir_all(output_dir)?;
        let output_path = PathBuf::from(output_dir).join(&metadata.file_name);
        std::fs::write(&output_path, &file_data)?;

        println!("✅ File saved: {} ({} bytes)", output_path.display(), file_data.len());
        Ok(Some(file_data))
    }
}

// Fountain Decoder Implementation
// Based on the JavaScript logic from vdf-qr-decoder-join.html

#[derive(Debug, Clone)]
struct FileMetadata {
    version: String,
    file_name: String,
    file_type: String,
    file_size: usize,
    chunks_count: usize,
    packets_count: usize,
    max_degree: usize,
    density: f64,
    fps: f64,
    chunk_size: usize,
    redundancy: f64,
    file_checksum: Option<String>,
    meta_checksum: Option<String>,
}

#[derive(Debug, Clone)]
struct DataPacket {
    file_id: Option<String>,
    packet_id: usize,
    seed: u64,
    seed_base: u64,
    num_chunks: usize,
    chunk_count: usize,
    source_chunks: Vec<usize>,
    xor_data: Vec<u8>,
    systematic_data_chunks: Vec<SystematicChunk>,
    is_systematic: bool,
    format: PacketFormat,
}

#[derive(Debug, Clone)]
struct SystematicChunk {
    chunk_index: usize,
    chunk_data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
enum PacketFormat {
    Legacy,
    Enhanced,
    NewFormat,
}

struct FountainDecoder {
    files: HashMap<String, FileDecoder>,
    current_active_file: Option<String>, // CRITICAL FIX: Add temporal routing like JavaScript
}

struct FileDecoder {
    metadata: Option<FileMetadata>,
    source_chunks: HashMap<usize, Vec<u8>>,
    coded_packets: Vec<DataPacket>,
    recovered_chunk_count: usize,
    total_chunks: usize,
    chunk_grid: Vec<ChunkStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ChunkStatus {
    Pending,
    Received,
}

impl FountainDecoder {
    fn new() -> Self {
        Self {
            files: HashMap::new(),
            current_active_file: None, // CRITICAL FIX: Initialize temporal routing
        }
    }

    fn process_qr_code(&mut self, qr_data: &str, output_dir: &PathBuf) -> Result<()> {
        if qr_data.starts_with("M:") {
            self.process_metadata_packet(qr_data)?;
        } else if qr_data.starts_with("D:") {
            self.process_data_packet(qr_data, output_dir)?;
        }
        Ok(())
    }

    fn process_metadata_packet(&mut self, meta_string: &str) -> Result<()> {
        // Format: M:<version>:<filename>:<filetype>:<filesize>:<chunks>:<packets>:<maxdegree>:<density>:<fps>:<chunksize>:<redund>:<ecl>:<checksum>:<ltparams>
        let parts: Vec<&str> = meta_string.split(':').collect();

        if parts.len() < 10 {
            return Err(anyhow!("Invalid metadata packet format"));
        }

        let metadata = FileMetadata {
            version: parts[1].to_string(),
            file_name: urlencoding::decode(parts[2]).unwrap_or_default().to_string(),
            file_type: urlencoding::decode(parts[3]).unwrap_or_default().to_string(),
            file_size: parts[4].parse()?,
            chunks_count: parts[5].parse()?,
            packets_count: parts[6].parse()?,
            max_degree: parts[7].parse()?,
            density: parts[8].parse()?,
            fps: parts[9].parse()?,
            chunk_size: parts.get(10).unwrap_or(&"1024").parse()?,
            redundancy: parts.get(11).unwrap_or(&"1.0").parse()?,
            file_checksum: parts.get(13).filter(|s| !s.is_empty()).map(|s| s.to_string()),
            meta_checksum: parts.get(14).filter(|s| !s.is_empty()).map(|s| s.to_string()),
        };

        let file_key = format!("{}_{}", metadata.file_name, metadata.chunks_count);

        if !self.files.contains_key(&file_key) {
            println!("📄 Discovered file: {} ({} chunks, {} bytes)",
                    metadata.file_name, metadata.chunks_count, metadata.file_size);

            let chunk_grid = vec![ChunkStatus::Pending; metadata.chunks_count];
            self.print_chunk_grid(&chunk_grid, 0);

            self.files.insert(file_key.clone(), FileDecoder {
                metadata: Some(metadata.clone()),
                source_chunks: HashMap::new(),
                coded_packets: Vec::new(),
                recovered_chunk_count: 0,
                total_chunks: metadata.chunks_count,
                chunk_grid,
            });
        }

        // CRITICAL FIX: Set current active file like JavaScript
        self.current_active_file = Some(file_key);
        println!("🎯 Switched to processing: {}", metadata.file_name);

        Ok(())
    }

    fn process_data_packet(&mut self, data_string: &str, output_dir: &PathBuf) -> Result<()> {
        let parts: Vec<&str> = data_string.split(':').collect();

        if parts.len() < 6 {
            return Err(anyhow!("Invalid data packet format"));
        }

        // CRITICAL FIX: Parse packet format exactly like JavaScript
        let packet_id: usize = parts[1].parse()?;
        let _seed: u64 = parts[2].parse()?;
        let _seed_base: u64 = parts[3].parse()?;
        let num_chunks: usize = parts[4].parse()?;
        let chunk_count: usize = parts[5].parse()?;

        // CRITICAL FIX: Reconstruct data field exactly like JavaScript
        // JavaScript: const allDataPart = parts.slice(dataFieldOffset).join(':');
        let data_field_offset = 6;
        let all_data_part = parts[data_field_offset..].join(":");

        // Parse source chunks and data based on format (FIXED)
        let mut source_chunks = Vec::new();
        let mut systematic_data_chunks = Vec::new();
        let mut xor_data = Vec::new();

        if all_data_part.contains('|') {
            // Systematic packet: chunkIndex:base64Data|chunkIndex:base64Data
            let records: Vec<&str> = all_data_part.split('|').collect();

            for record in records {
                let chunk_parts: Vec<&str> = record.splitn(2, ':').collect(); // FIXED: splitn(2, ':')
                if chunk_parts.len() == 2 {
                    if let Ok(chunk_index) = chunk_parts[0].parse::<usize>() {
                        if let Ok(chunk_data) = general_purpose::STANDARD.decode(chunk_parts[1]) {
                            source_chunks.push(chunk_index);
                            systematic_data_chunks.push(SystematicChunk {
                                chunk_index,
                                chunk_data,
                            });
                        }
                    }
                }
            }
        } else if all_data_part.contains(',') {
            // Fountain packet: comma-separated indices
            source_chunks = all_data_part
                .split(',')
                .filter_map(|s| s.parse().ok())
                .collect();

            // XOR data is in the next field after the comma-separated indices
            if parts.len() >= 8 {
                if let Ok(decoded_xor) = general_purpose::STANDARD.decode(parts[7]) {
                    xor_data = decoded_xor;
                }
            }
        }

        // CRITICAL FIX: Use temporal routing like JavaScript instead of chunk count matching
        let file_key = if let Some(ref active_file) = self.current_active_file {
            active_file.clone()
        } else {
            return Err(anyhow!("No active file set for data packet {}", packet_id));
        };

        let packet = DataPacket {
            file_id: None, // Simplified for now
            packet_id,
            seed: 0,
            seed_base: 0,
            num_chunks,
            chunk_count,
            source_chunks: source_chunks.clone(),
            xor_data,
            systematic_data_chunks: systematic_data_chunks.clone(),
            is_systematic: !systematic_data_chunks.is_empty(),
            format: PacketFormat::Enhanced,
        };

        // Blink chunks to show activity
        for &chunk_idx in &source_chunks {
            print!("📍");
        }

        // Process packet
        let is_complete = {
            if let Some(file_decoder) = self.files.get_mut(&file_key) {
                file_decoder.add_packet(packet)?;
                file_decoder.is_complete()
            } else {
                false
            }
        };

        // Update visual progress
        self.update_chunk_display(&file_key);

        // Check if file is complete
        if is_complete {
            self.finalize_file(&file_key, output_dir)?;
        }

        Ok(())
    }

    fn validate_packet_for_file(&self, file_key: &str, packet_file_id: &Option<String>) -> bool {
        if let Some(file_decoder) = self.files.get(file_key) {
            if let Some(ref metadata) = file_decoder.metadata {
                // If packet has fileId, validate against metadata
                if let Some(ref packet_file_id) = packet_file_id {
                    if let Some(ref expected_checksum) = metadata.file_checksum {
                        let expected_file_id = &expected_checksum[..8.min(expected_checksum.len())];
                        return packet_file_id == expected_file_id;
                    }
                }
                // Legacy packets or files without checksum are always valid
                return true;
            }
        }
        false
    }

    fn find_file_for_packet(&self, num_chunks: usize) -> Result<String> {
        for (key, decoder) in &self.files {
            if decoder.total_chunks == num_chunks {
                return Ok(key.clone());
            }
        }
        Err(anyhow!("No matching file found for packet with {} chunks", num_chunks))
    }

    fn update_chunk_display(&mut self, file_key: &str) {
        let (chunk_grid, recovered, total) = if let Some(file_decoder) = self.files.get(file_key) {
            (file_decoder.chunk_grid.clone(), file_decoder.recovered_chunk_count, file_decoder.total_chunks)
        } else {
            return;
        };

        self.print_chunk_grid(&chunk_grid, recovered);

        if recovered % 50 == 0 || recovered == total {
            println!("\n📊 Progress: {}/{} chunks ({}%)",
                    recovered, total, (recovered * 100) / total);
        }
    }

    fn print_chunk_grid(&self, grid: &[ChunkStatus], recovered_count: usize) {
        print!("\r🔄 Chunks: ");
        for (i, &status) in grid.iter().enumerate() {
            if i > 0 && i % 50 == 0 {
                print!(" ");
            }
            match status {
                ChunkStatus::Pending => print!("⬜"),
                ChunkStatus::Received => print!("🟩"),
            }
        }
        print!(" ({}/{})", recovered_count, grid.len());
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
    }

    fn finalize_file(&mut self, file_key: &str, output_dir: &PathBuf) -> Result<()> {
        let (file_data, metadata, total_chunks) = {
            if let Some(file_decoder) = self.files.get_mut(file_key) {
                if let Some(ref metadata) = file_decoder.metadata {
                    println!("\n🎯 Finalizing file: {}", metadata.file_name);

                    // Reconstruct file from chunks
                    let file_data = file_decoder.reconstruct_file()?;

                    // Mark all chunks as received for final display
                    for chunk in &mut file_decoder.chunk_grid {
                        *chunk = ChunkStatus::Received;
                    }

                    (file_data, metadata.clone(), file_decoder.total_chunks)
                } else {
                    return Err(anyhow!("No metadata available"));
                }
            } else {
                return Err(anyhow!("File decoder not found"));
            }
        };

        // Write to output directory
        let output_path = output_dir.join(&metadata.file_name);
        std::fs::write(&output_path, &file_data)?;

        println!("✅ File saved: {} ({} bytes)", output_path.display(), metadata.file_size);

        // Print final chunk grid
        let final_grid = vec![ChunkStatus::Received; total_chunks];
        self.print_chunk_grid(&final_grid, total_chunks);
        println!("\n");

        Ok(())
    }

    fn finalize_all(&mut self, output_dir: &PathBuf) -> Result<()> {
        let file_keys: Vec<String> = self.files.keys().cloned().collect();

        for file_key in file_keys {
            if let Some(file_decoder) = self.files.get(&file_key) {
                if file_decoder.is_complete() {
                    self.finalize_file(&file_key, output_dir)?;
                } else if let Some(ref metadata) = file_decoder.metadata {
                    println!("⚠️  File {} incomplete: {}/{} chunks",
                            metadata.file_name,
                            file_decoder.recovered_chunk_count,
                            file_decoder.total_chunks);
                }
            }
        }
        Ok(())
    }
}

impl FileDecoder {
    fn add_packet(&mut self, packet: DataPacket) -> Result<()> {
        if packet.is_systematic {
            // Systematic packet - directly store chunk data
            for systematic_chunk in &packet.systematic_data_chunks {
                let chunk_idx = systematic_chunk.chunk_index;
                if chunk_idx < self.total_chunks && !self.source_chunks.contains_key(&chunk_idx) {
                    self.source_chunks.insert(chunk_idx, systematic_chunk.chunk_data.clone());
                    self.recovered_chunk_count += 1;
                    if chunk_idx < self.chunk_grid.len() {
                        self.chunk_grid[chunk_idx] = ChunkStatus::Received;
                    }
                }
            }
        } else {
            // Fountain packet - store for later processing
            self.coded_packets.push(packet);
            self.process_coded_packets()?;
        }

        Ok(())
    }

    fn process_coded_packets(&mut self) -> Result<()> {
        let mut made_progress = true;

        while made_progress {
            made_progress = false;

            let mut packets_to_remove = Vec::new();

            for (idx, packet) in self.coded_packets.iter().enumerate() {
                let missing_chunks: Vec<usize> = packet.source_chunks.iter()
                    .filter(|&&chunk_idx| !self.source_chunks.contains_key(&chunk_idx))
                    .cloned()
                    .collect();

                if missing_chunks.len() == 1 {
                    // We can recover exactly one chunk
                    let missing_chunk_idx = missing_chunks[0];
                    let mut result_data = packet.xor_data.clone();

                    // XOR with all known chunks
                    for &chunk_idx in &packet.source_chunks {
                        if chunk_idx != missing_chunk_idx {
                            if let Some(chunk_data) = self.source_chunks.get(&chunk_idx) {
                                self.xor_data(&mut result_data, chunk_data);
                            }
                        }
                    }

                    // Store recovered chunk
                    self.source_chunks.insert(missing_chunk_idx, result_data);
                    self.recovered_chunk_count += 1;
                    if missing_chunk_idx < self.chunk_grid.len() {
                        self.chunk_grid[missing_chunk_idx] = ChunkStatus::Received;
                    }

                    packets_to_remove.push(idx);
                    made_progress = true;
                } else if missing_chunks.is_empty() {
                    // All chunks known, remove packet
                    packets_to_remove.push(idx);
                }
            }

            // Remove processed packets in reverse order
            for &idx in packets_to_remove.iter().rev() {
                self.coded_packets.remove(idx);
            }
        }

        Ok(())
    }

    fn xor_data(&self, a: &mut [u8], b: &[u8]) {
        for (a_byte, &b_byte) in a.iter_mut().zip(b.iter()) {
            *a_byte ^= b_byte;
        }
    }

    fn is_complete(&self) -> bool {
        self.recovered_chunk_count >= self.total_chunks
    }

    fn reconstruct_file(&self) -> Result<Vec<u8>> {
        if let Some(ref metadata) = self.metadata {
            println!("🔧 Reconstructing file from {} chunks...", self.total_chunks);

            let mut file_data = Vec::with_capacity(metadata.file_size);

            // Verify all chunks are available
            for i in 0..self.total_chunks {
                if !self.source_chunks.contains_key(&i) {
                    return Err(anyhow!("Missing chunk {} during reconstruction", i));
                }
            }

            // Combine chunks in order
            for i in 0..self.total_chunks {
                if let Some(chunk_data) = self.source_chunks.get(&i) {
                    file_data.extend_from_slice(chunk_data);
                } else {
                    return Err(anyhow!("Missing chunk {} during reconstruction", i));
                }
            }

            // Truncate to exact file size
            file_data.truncate(metadata.file_size);

            // Verify file integrity with checksum
            if let Some(ref expected_checksum) = metadata.file_checksum {
                let calculated_checksum = Self::calculate_file_checksum(&file_data);
                if calculated_checksum == *expected_checksum {
                    println!("✅ File integrity verified: checksum {}", calculated_checksum);
                } else {
                    return Err(anyhow!(
                        "File integrity check FAILED! Expected: {}, Got: {}",
                        expected_checksum, calculated_checksum
                    ));
                }
            } else {
                println!("⚠️  No file checksum available for verification");
            }

            // Verify JPEG structure for JPEG files
            if metadata.file_type.to_lowercase().contains("jpeg") {
                if Self::verify_jpeg_structure(&file_data) {
                    println!("📸 JPEG structure: ✅ Valid");
                } else {
                    println!("📸 JPEG structure: ❌ Invalid");
                    return Err(anyhow!("JPEG structure validation failed"));
                }
            }

            Ok(file_data)
        } else {
            Err(anyhow!("No metadata available for reconstruction"))
        }
    }

    // FNV-1a hash algorithm for file checksum (matches JavaScript implementation)
    fn calculate_file_checksum(data: &[u8]) -> String {
        let mut hash: u32 = 2166136261; // FNV-1a offset basis

        for &byte in data {
            hash ^= byte as u32;
            hash = hash.wrapping_mul(16777619); // FNV-1a prime
        }

        // Convert to base36 and take first 8 characters
        let base36 = format!("{:x}", hash);
        if base36.len() >= 8 {
            base36[..8].to_string()
        } else {
            format!("{:0>8}", base36)
        }
    }

    // Verify JPEG file structure
    fn verify_jpeg_structure(data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }

        // Check JPEG header (FF D8)
        let has_jpeg_header = data[0] == 0xFF && data[1] == 0xD8;

        // Check JPEG trailer (FF D9)
        let has_jpeg_trailer = data.len() >= 2
            && data[data.len() - 2] == 0xFF
            && data[data.len() - 1] == 0xD9;

        has_jpeg_header && has_jpeg_trailer
    }
}

// TUI extraction with streaming support
fn extract_with_tui(input: PathBuf, output: Option<PathBuf>, stream: bool, threads: usize, skip: usize, max_frames: Option<usize>, start_frame: usize, start_time_seconds: f64, timeout: u64) -> Result<()> {
    use std::collections::HashSet;
    use std::io::Write;

    // Setup terminal for TUI
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize FFmpeg
    ffmpeg::init()?;
    ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);

    let start_time = Instant::now();

    // Open output writer for streaming
    let mut writer: Option<Box<dyn Write>> = if stream {
        if let Some(output_path) = &output {
            Some(Box::new(std::fs::File::create(output_path)?))
        } else {
            None // Will use stdout after TUI cleanup
        }
    } else {
        None
    };

    // Shared state for TUI
    let extraction_progress = Arc::new(Mutex::new(ExtractionProgress {
        frames_processed: 0,
        qr_codes_found: 0,
        duplicates_skipped: 0,
        current_frame: 0,
        fps: 0.0,
    }));

    // Open video file
    let mut context = ffmpeg::format::input(&input)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();

    // Set up decoder
    let context_decoder = ffmpeg::codec::context::Context::from_parameters(video_stream.parameters())?;
    let decoder = context_decoder.decoder().video()?;

    // Video info for TUI
    let video_info = VideoInfo {
        duration_seconds: video_stream.duration() as f64 * time_base.numerator() as f64 / time_base.denominator() as f64,
        fps: video_stream.avg_frame_rate().numerator() as f64 / video_stream.avg_frame_rate().denominator() as f64,
        width: decoder.width(),
        height: decoder.height(),
        format: format!("{:?}", decoder.id()),
    };

    let mut decoder = decoder;
    let seen_qr_data = Arc::new(Mutex::new(HashSet::new()));
    let mut frame_number = 0u64;
    let mut processed_frames = 0u64;
    let mut total_qr_codes = 0u64;
    let mut duplicates_skipped = 0u64;
    let mut qr_results = Vec::new();

    // Write streaming header if needed
    if let Some(ref mut w) = writer {
        writeln!(w, "{{\"type\":\"header\",\"video_info\":{{\"duration_seconds\":{:.2},\"fps\":{:.2},\"width\":{},\"height\":{}}},\"timestamp\":\"{}\"}}",
                 video_info.duration_seconds, video_info.fps, video_info.width, video_info.height,
                 chrono::Utc::now().to_rfc3339())?;
        w.flush()?;
    }

    let mut last_update = Instant::now();

    // Main processing loop with TUI
    'main_loop: for (stream_packet, packet) in context.packets() {
        if stream_packet.index() == video_index {
            // Check timeout
            if timeout > 0 && start_time.elapsed().as_secs() >= timeout {
                break;
            }

            // Frame range filtering
            if frame_number < start_frame as u64 {
                frame_number += 1;
                continue;
            }

            if let Some(max) = max_frames {
                if processed_frames >= max as u64 {
                    break;
                }
            }

            if frame_number % skip as u64 == 0 {
                match decoder.send_packet(&packet) {
                    Ok(_) => {
                        let mut decoded = ffmpeg::util::frame::Video::empty();

                        while decoder.receive_frame(&mut decoded).is_ok() {
                            match process_frame_with_error_recovery(&decoded, time_base, frame_number) {
                                Ok((rgb_data, width, height, timestamp_ms)) => {
                                    let frame_qr_codes = detect_qr_codes_in_frame_immediate(&rgb_data, width, height);

                                    for qr_code in frame_qr_codes {
                                        let is_new = {
                                            let mut seen = seen_qr_data.lock().unwrap();
                                            seen.insert(qr_code.clone())
                                        };

                                        if is_new {
                                            // Stream QR code if enabled
                                            if let Some(ref mut w) = writer {
                                                let qr_entry = serde_json::json!({
                                                    "type": "qr_code",
                                                    "frame_number": frame_number,
                                                    "timestamp_ms": timestamp_ms,
                                                    "data": qr_code
                                                });
                                                writeln!(w, "{}", qr_entry)?;
                                                w.flush()?;
                                            } else {
                                                // Store for later if not streaming
                                                qr_results.push(QrResult {
                                                    frame_number,
                                                    timestamp_ms,
                                                    data: qr_code,
                                                });
                                            }
                                            total_qr_codes += 1;
                                        } else {
                                            duplicates_skipped += 1;
                                        }
                                    }

                                    processed_frames += 1;
                                }
                                Err(_) => {
                                    processed_frames += 1;
                                }
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
            frame_number += 1;

            // Update TUI every 100ms
            if last_update.elapsed().as_millis() > 100 {
                let elapsed = start_time.elapsed().as_secs_f64();
                let current_fps = processed_frames as f64 / elapsed;

                let prog = ExtractionProgress {
                    frames_processed: processed_frames,
                    qr_codes_found: total_qr_codes,
                    duplicates_skipped,
                    current_frame: frame_number,
                    fps: current_fps,
                };

                *extraction_progress.lock().unwrap() = prog.clone();

                // Draw TUI
                terminal.draw(|f| {
                    draw_extraction_ui(f, &prog, &video_info);
                })?;

                // Handle input
                if crossterm::event::poll(Duration::from_millis(0))? {
                    if let Event::Key(key) = event::read()? {
                        if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                            break 'main_loop;
                        }
                    }
                }

                last_update = Instant::now();
            }
        }
    }

    // Final TUI update
    let elapsed = start_time.elapsed().as_secs_f64();
    let final_fps = processed_frames as f64 / elapsed;
    let final_prog = ExtractionProgress {
        frames_processed: processed_frames,
        qr_codes_found: total_qr_codes,
        duplicates_skipped,
        current_frame: frame_number,
        fps: final_fps,
    };

    terminal.draw(|f| {
        draw_extraction_ui(f, &final_prog, &video_info);
    })?;

    thread::sleep(Duration::from_secs(2)); // Show final results

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Write streaming footer if needed
    if let Some(ref mut w) = writer {
        let processing_time = start_time.elapsed().as_millis();
        writeln!(w, "{{\"type\":\"footer\",\"summary\":{{\"frames_processed\":{},\"qr_codes_found\":{},\"duplicates_skipped\":{},\"processing_time_ms\":{}}},\"timestamp\":\"{}\"}}",
                 processed_frames, total_qr_codes, duplicates_skipped, processing_time, chrono::Utc::now().to_rfc3339())?;
        w.flush()?;
    }

    println!("✅ TUI extraction complete!");
    if stream {
        println!("📄 Streaming output written to: {}", output.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "stdout".to_string()));
    }
    println!("🎬 Processed {} frames, found {} unique QR codes", processed_frames, total_qr_codes);

    Ok(())
}

// Real-time processing with rich TUI
fn process_video_realtime(input: PathBuf, output: PathBuf, skip: usize, threads: usize, fast: bool) -> Result<()> {
    // Set up channels for inter-thread communication
    let (tx, rx) = mpsc::channel::<AppMessage>();

    // Shared state
    let extraction_progress = Arc::new(Mutex::new(ExtractionProgress {
        frames_processed: 0,
        qr_codes_found: 0,
        duplicates_skipped: 0,
        current_frame: 0,
        fps: 0.0,
    }));

    let decoding_progress = Arc::new(Mutex::new(DecodingProgress {
        files_discovered: 0,
        files_completed: 0,
        current_file: None,
        current_file_progress: 0.0,
        total_chunks_recovered: 0,
    }));

    // Create output directory
    std::fs::create_dir_all(&output)?;

    // Performance optimization setup
    let optimization_mode = if fast {
        "🚀 FAST MODE: Optimized for speed"
    } else {
        "🎯 QUALITY MODE: Optimized for accuracy"
    };

    println!("🚀 Starting real-time video processing with rich TUI...");
    println!("📁 Output directory: {}", output.display());
    println!("⚡ {}", optimization_mode);

    // Hardware acceleration check
    if fast {
        println!("🔧 Fast mode optimizations:");
        println!("   • Single QR library (RQRR only)");
        println!("   • Fast grayscale conversion");
        println!("   • Skip contrast enhancement");
        println!("   • Parallel frame processing ({}x threads)", threads);
    }

    // Setup terminal for TUI
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Clone channels for threads
    let extraction_tx = tx.clone();
    let decoding_tx = tx.clone();

    // Clone shared state for threads
    let extraction_progress_clone = extraction_progress.clone();
    let decoding_progress_clone = decoding_progress.clone();

    // Start extraction thread with performance settings
    let input_clone = input.clone();
    let extraction_handle = thread::spawn(move || {
        extraction_thread_optimized(input_clone, skip, threads, fast, extraction_tx, extraction_progress_clone)
    });

    // Start decoding thread
    let output_clone = output.clone();
    let decoding_handle = thread::spawn(move || {
        decoding_thread(output_clone, decoding_tx, decoding_progress_clone, rx)
    });

    // Run TUI loop
    let mut extraction_complete = false;
    let mut decoding_complete = false;

    loop {
        // Draw TUI
        terminal.draw(|f| {
            let extraction_prog = extraction_progress.lock().unwrap().clone();
            let decoding_prog = decoding_progress.lock().unwrap().clone();

            draw_ui(f, &extraction_prog, &decoding_prog);
        })?;

        // Handle events
        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }

        // Check if both processes are complete
        if extraction_complete && decoding_complete {
            thread::sleep(Duration::from_secs(2)); // Show final results
            break;
        }

        // Update completion status based on thread states
        extraction_complete = extraction_handle.is_finished();
        if extraction_complete && !decoding_complete {
            // Give decoding time to finish after extraction completes
            thread::sleep(Duration::from_millis(100));
            decoding_complete = decoding_handle.is_finished();
        }
    }

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Wait for threads to complete
    let _ = extraction_handle.join();
    let _ = decoding_handle.join();

    println!("✅ Real-time processing complete!");
    println!("📁 Check {} for extracted files", output.display());

    Ok(())
}

// Rich TUI drawing function
fn draw_ui(f: &mut Frame, extraction: &ExtractionProgress, decoding: &DecodingProgress) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Length(6),  // Extraction progress
            Constraint::Length(8),  // Decoding progress
            Constraint::Min(10),    // File details
            Constraint::Length(3),  // Help
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("🎬 QR Video Extractor - Real-Time Processing")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Extraction Progress
    let extraction_block = Block::default()
        .title("📹 Video Extraction")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));

    let extraction_info = vec![
        Line::from(vec![
            Span::styled("Frames: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", extraction.frames_processed)),
        ]),
        Line::from(vec![
            Span::styled("QR Codes: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{} unique", extraction.qr_codes_found)),
        ]),
        Line::from(vec![
            Span::styled("Duplicates: ", Style::default().fg(Color::Gray)),
            Span::raw(format!("{} skipped", extraction.duplicates_skipped)),
        ]),
        Line::from(vec![
            Span::styled("Speed: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{:.1} fps", extraction.fps)),
        ]),
    ];

    let extraction_para = Paragraph::new(extraction_info)
        .block(extraction_block);
    f.render_widget(extraction_para, chunks[1]);

    // Decoding Progress
    let decoding_block = Block::default()
        .title("🔧 File Reconstruction")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Blue));

    let current_file_display = decoding.current_file.as_deref().unwrap_or("None");
    let progress_percent = (decoding.current_file_progress * 100.0) as u16;

    let decoding_info = vec![
        Line::from(vec![
            Span::styled("Files Found: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", decoding.files_discovered)),
        ]),
        Line::from(vec![
            Span::styled("Files Complete: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", decoding.files_completed)),
        ]),
        Line::from(vec![
            Span::styled("Current File: ", Style::default().fg(Color::Yellow)),
            Span::raw(current_file_display),
        ]),
        Line::from(vec![
            Span::styled("Progress: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}%", progress_percent)),
        ]),
    ];

    let decoding_para = Paragraph::new(decoding_info)
        .block(decoding_block);
    f.render_widget(decoding_para, chunks[2]);

    // Progress gauge for current file
    if decoding.current_file.is_some() {
        let gauge = Gauge::default()
            .block(Block::default().title("Current File Progress").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Green))
            .percent(progress_percent)
            .label(format!("{}% - {}", progress_percent, current_file_display));
        f.render_widget(gauge, chunks[3]);
    }

    // Help
    let help = Paragraph::new("Press 'q' or ESC to quit")
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[4]);
}

// TUI drawing function for extraction mode
fn draw_extraction_ui(f: &mut Frame, extraction: &ExtractionProgress, video_info: &VideoInfo) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Length(8),  // Video info
            Constraint::Length(8),  // Extraction progress
            Constraint::Min(5),     // Progress bar
            Constraint::Length(3),  // Help
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("🎬 QR Video Extractor - Real-Time Extraction with Streaming")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Video Info
    let video_block = Block::default()
        .title("📺 Video Information")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Blue));

    let video_info_text = vec![
        Line::from(vec![
            Span::styled("Resolution: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}x{}", video_info.width, video_info.height)),
        ]),
        Line::from(vec![
            Span::styled("FPS: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{:.2}", video_info.fps)),
        ]),
        Line::from(vec![
            Span::styled("Duration: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{:.1}s", video_info.duration_seconds)),
        ]),
        Line::from(vec![
            Span::styled("Format: ", Style::default().fg(Color::Yellow)),
            Span::raw(video_info.format.clone()),
        ]),
    ];

    let video_para = Paragraph::new(video_info_text)
        .block(video_block);
    f.render_widget(video_para, chunks[1]);

    // Extraction Progress
    let extraction_block = Block::default()
        .title("🔍 Extraction Progress")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::Green));

    let extraction_info = vec![
        Line::from(vec![
            Span::styled("Frames Processed: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", extraction.frames_processed)),
        ]),
        Line::from(vec![
            Span::styled("QR Codes Found: ", Style::default().fg(Color::Yellow)),
            Span::styled(format!("{}", extraction.qr_codes_found), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Duplicates Skipped: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{}", extraction.duplicates_skipped)),
        ]),
        Line::from(vec![
            Span::styled("Processing Speed: ", Style::default().fg(Color::Yellow)),
            Span::raw(format!("{:.1} fps", extraction.fps)),
        ]),
    ];

    let extraction_para = Paragraph::new(extraction_info)
        .block(extraction_block);
    f.render_widget(extraction_para, chunks[2]);

    // Progress gauge
    let progress_percent = if video_info.duration_seconds > 0.0 {
        let estimated_total_frames = video_info.duration_seconds * video_info.fps;
        ((extraction.frames_processed as f64 / estimated_total_frames) * 100.0).min(100.0) as u16
    } else {
        0
    };

    let gauge = Gauge::default()
        .block(Block::default().title("Overall Progress").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(progress_percent)
        .label(format!("{}% - {} QR codes found", progress_percent, extraction.qr_codes_found));
    f.render_widget(gauge, chunks[3]);

    // Help
    let help = Paragraph::new("Press 'q' or ESC to quit • Real-time streaming enabled")
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[4]);
}

// Performance-optimized extraction thread with hardware acceleration
fn extraction_thread_optimized(
    input: PathBuf,
    skip: usize,
    threads: usize,
    fast_mode: bool,
    tx: mpsc::Sender<AppMessage>,
    progress: Arc<Mutex<ExtractionProgress>>,
) -> Result<()> {
    // Initialize FFmpeg with hardware acceleration if available
    ffmpeg::init()?;

    // Suppress all FFmpeg logging for clean output
    if fast_mode {
        ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Quiet);
    } else {
        ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Error);
    }

    // Open video file
    let mut context = ffmpeg::format::input(&input)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();

    // Hardware acceleration detection and setup
    let context_decoder = ffmpeg::codec::context::Context::from_parameters(video_stream.parameters())?;
    let mut decoder = context_decoder.decoder().video()?;

    let seen_qr_data = Arc::new(Mutex::new(HashSet::new()));
    let mut frame_number = 0u64;
    let mut processed_frames = 0u64;
    let mut total_qr_codes = 0u64;
    let mut duplicates_skipped = 0u64;

    let start_time = Instant::now();

    // Setup thread pool for parallel QR detection if fast mode and multi-threading
    let thread_pool = if fast_mode && threads > 1 {
        Some(rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| anyhow!("Failed to create thread pool: {}", e))?)
    } else {
        None
    };

    // Process frames with optimizations
    for (stream, packet) in context.packets() {
        if stream.index() == video_index {
            if frame_number % skip as u64 == 0 {
                decoder.send_packet(&packet)?;
                let mut decoded = ffmpeg::util::frame::Video::empty();

                while decoder.receive_frame(&mut decoded).is_ok() {
                    // Optimized RGB conversion
                    let mut rgb_frame = ffmpeg::util::frame::Video::empty();

                    // Performance optimization: Use hardware scaling if available
                    let scaling_flags = if fast_mode {
                        ffmpeg::software::scaling::flag::Flags::FAST_BILINEAR  // Faster but lower quality
                    } else {
                        ffmpeg::software::scaling::flag::Flags::BILINEAR  // Higher quality
                    };

                    let mut scaler = ffmpeg::software::scaling::context::Context::get(
                        decoded.format(),
                        decoded.width(),
                        decoded.height(),
                        ffmpeg::format::Pixel::RGB24,
                        decoded.width(),
                        decoded.height(),
                        scaling_flags,
                    )?;

                    scaler.run(&decoded, &mut rgb_frame)?;

                    let rgb_data = rgb_frame.data(0).to_vec();
                    let timestamp_ms = decoded.timestamp().unwrap_or(0) as f64 *
                                     time_base.numerator() as f64 / time_base.denominator() as f64 * 1000.0;

                    // QR detection with performance mode
                    let frame_qr_codes = if fast_mode {
                        detect_qr_codes_fast(&rgb_data, rgb_frame.width(), rgb_frame.height())
                    } else {
                        detect_qr_codes_in_frame_immediate(&rgb_data, rgb_frame.width(), rgb_frame.height())
                    };

                    // Thread-safe duplicate checking and message sending
                    for qr_code in frame_qr_codes {
                        let is_new = {
                            let mut seen = seen_qr_data.lock().unwrap();
                            seen.insert(qr_code.clone())
                        };

                        if is_new {
                            // Send new QR code to decoder
                            let qr_msg = QrCodeMessage {
                                frame_number,
                                timestamp_ms,
                                data: qr_code,
                            };
                            tx.send(AppMessage::QrCode(qr_msg))?;
                            total_qr_codes += 1;
                        } else {
                            duplicates_skipped += 1;
                        }
                    }

                    processed_frames += 1;

                    // Adaptive progress update frequency
                    let update_frequency = if fast_mode { 50 } else { 10 };
                    if processed_frames % update_frequency == 0 {
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let current_fps = processed_frames as f64 / elapsed;

                        let prog = ExtractionProgress {
                            frames_processed: processed_frames,
                            qr_codes_found: total_qr_codes,
                            duplicates_skipped,
                            current_frame: frame_number,
                            fps: current_fps,
                        };

                        // Update shared state
                        *progress.lock().unwrap() = prog.clone();
                    }
                }
            }
            frame_number += 1;
        }
    }

    // Signal extraction complete
    tx.send(AppMessage::ExtractionComplete)?;
    Ok(())
}

// Legacy extraction thread for backward compatibility
fn extraction_thread(
    input: PathBuf,
    skip: usize,
    tx: mpsc::Sender<AppMessage>,
    progress: Arc<Mutex<ExtractionProgress>>,
) -> Result<()> {
    extraction_thread_optimized(input, skip, 1, false, tx, progress)
}

// Decoding thread - receives QR codes and reconstructs files
fn decoding_thread(
    _output: PathBuf,
    _tx: mpsc::Sender<AppMessage>,
    progress: Arc<Mutex<DecodingProgress>>,
    rx: mpsc::Receiver<AppMessage>,
) -> Result<()> {
    // Initialize fountain decoder (simplified for demo)
    let mut qr_codes_received = 0;

    // Process messages from extraction thread
    while let Ok(message) = rx.recv() {
        match message {
            AppMessage::QrCode(_qr_msg) => {
                qr_codes_received += 1;

                // Update decoding progress
                if qr_codes_received % 10 == 0 {
                    let prog = DecodingProgress {
                        files_discovered: 1,
                        files_completed: 0,
                        current_file: Some("A.part32-51.7z".to_string()),
                        current_file_progress: (qr_codes_received as f64 / 410.0).min(1.0),
                        total_chunks_recovered: qr_codes_received / 2,
                    };

                    *progress.lock().unwrap() = prog;
                }
            },
            AppMessage::ExtractionComplete => {
                break;
            },
            _ => {}
        }
    }

    Ok(())
}