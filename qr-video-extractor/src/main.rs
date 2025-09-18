use anyhow::{anyhow, Result};
use clap::Parser;
use ffmpeg_next as ffmpeg;
use rqrr;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input video file path
    input: PathBuf,

    /// Output JSON file path (optional)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Maximum number of threads to use
    #[arg(short, long, default_value_t = num_cpus::get())]
    threads: usize,

    /// Skip frames (process every Nth frame)
    #[arg(short, long, default_value_t = 1)]
    skip: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct QrResult {
    frame_number: u64,
    timestamp_ms: f64,
    data: String,
    corners: [(i32, i32); 4],
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

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize FFmpeg
    ffmpeg::init()?;

    let start_time = Instant::now();
    println!("Starting QR code extraction from: {}", args.input.display());

    // Extract frames and process QR codes
    let results = extract_qr_codes_from_video(&args.input, args.threads, args.skip)?;

    let processing_time = start_time.elapsed().as_millis();

    // Print summary
    println!("\n=== Extraction Complete ===");
    println!("Total frames processed: {}", results.total_frames_processed);
    println!("QR codes found: {}", results.qr_codes_found);
    println!("Processing time: {:.2}s", processing_time as f64 / 1000.0);
    println!("Processing speed: {:.1}x realtime",
             results.video_info.duration_seconds / (processing_time as f64 / 1000.0));

    // Output results
    if let Some(output_path) = args.output {
        let json = serde_json::to_string_pretty(&results)?;
        std::fs::write(&output_path, json)?;
        println!("Results saved to: {}", output_path.display());
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

fn extract_qr_codes_from_video(input_path: &PathBuf, max_threads: usize, skip_frames: usize) -> Result<ExtractionResults> {
    // Set up thread pool
    rayon::ThreadPoolBuilder::new()
        .num_threads(max_threads)
        .build_global()
        .map_err(|e| anyhow!("Failed to initialize thread pool: {}", e))?;

    // Open video file
    let mut context = ffmpeg::format::input(&input_path)?;
    let video_stream = context.streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("No video stream found"))?;

    let video_index = video_stream.index();
    let time_base = video_stream.time_base();
    let fps = video_stream.avg_frame_rate();
    let fps_value = fps.numerator() as f64 / fps.denominator() as f64;

    // Set up decoder to get codec info
    let context_decoder = ffmpeg::codec::context::Context::from_parameters(video_stream.parameters())?;
    let decoder = context_decoder.decoder().video()?;

    // Get video info
    let video_info = VideoInfo {
        duration_seconds: video_stream.duration() as f64 * time_base.numerator() as f64 / time_base.denominator() as f64,
        fps: fps_value,
        width: decoder.width(),
        height: decoder.height(),
        format: format!("{:?}", decoder.id()),
    };

    println!("Video info: {}x{} @ {:.2} fps, {:.2}s duration",
             video_info.width, video_info.height, video_info.fps, video_info.duration_seconds);

    // Use the decoder we already created
    let mut decoder = decoder;

    // Collect frames for parallel processing
    let frames = Arc::new(Mutex::new(Vec::<FrameData>::new()));
    let mut frame_number = 0u64;
    let mut processed_frames = 0u64;

    // Frame extraction
    println!("Extracting frames...");
    for (stream, packet) in context.packets() {
        if stream.index() == video_index {
            if frame_number % skip_frames as u64 == 0 {
                decoder.send_packet(&packet)?;
                let mut decoded = ffmpeg::util::frame::Video::empty();

                while decoder.receive_frame(&mut decoded).is_ok() {
                    // Convert to RGB
                    let mut rgb_frame = ffmpeg::util::frame::Video::empty();
                    let mut scaler = ffmpeg::software::scaling::context::Context::get(
                        decoded.format(),
                        decoded.width(),
                        decoded.height(),
                        ffmpeg::format::Pixel::RGB24,
                        decoded.width(),
                        decoded.height(),
                        ffmpeg::software::scaling::flag::Flags::BILINEAR,
                    )?;

                    scaler.run(&decoded, &mut rgb_frame)?;

                    // Extract RGB data
                    let rgb_data = rgb_frame.data(0).to_vec();
                    let timestamp_ms = decoded.timestamp().unwrap_or(0) as f64 *
                                     time_base.numerator() as f64 / time_base.denominator() as f64 * 1000.0;

                    let frame_data = FrameData {
                        frame_number,
                        timestamp_ms,
                        rgb_data,
                        width: rgb_frame.width(),
                        height: rgb_frame.height(),
                    };

                    frames.lock().unwrap().push(frame_data);
                    processed_frames += 1;

                    if processed_frames % 100 == 0 {
                        print!("\rFrames extracted: {}", processed_frames);
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                }
            }
            frame_number += 1;
        }
    }

    println!("\nProcessing {} frames for QR codes...", processed_frames);

    // Process frames in parallel for QR codes
    let frames_vec = Arc::try_unwrap(frames).unwrap().into_inner().unwrap();
    let qr_results: Vec<Vec<QrResult>> = frames_vec
        .par_iter()
        .map(|frame| detect_qr_codes_in_frame(frame))
        .collect();

    // Flatten and sort results by frame number
    let mut all_results: Vec<QrResult> = qr_results.into_iter().flatten().collect();
    all_results.sort_by_key(|r| r.frame_number);

    Ok(ExtractionResults {
        video_info,
        total_frames_processed: processed_frames,
        qr_codes_found: all_results.len(),
        processing_time_ms: start_time.elapsed().as_millis(),
        results: all_results,
    })
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
                corners,
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