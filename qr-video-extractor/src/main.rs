use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

mod tui;
mod video;
mod qr_extraction;
mod file_reconstruction;
mod events;
mod test_events;

use tui::TuiManager;
use video::VideoProcessor;
use qr_extraction::QrExtractor;
use file_reconstruction::FileReconstructor;
use events::{EventCallback, ProcessingEvent, ConsoleOutputHandler, OutputHandler};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input video file path
    input: Option<PathBuf>,

    /// Output directory for decoded files
    #[arg(short, long, default_value = "video_results")]
    output: PathBuf,

    /// Number of video chunks to create (default: max(cpu_cores/2, 4))
    #[arg(short, long)]
    chunks: Option<usize>,

    /// Duration per chunk in seconds
    #[arg(short, long)]
    duration_per_chunk: Option<f64>,

    /// Skip frames (process every Nth frame)
    #[arg(short, long, default_value_t = 1)]
    skip: usize,

    /// Maximum number of threads to use
    #[arg(short, long)]
    threads: Option<usize>,

    /// Disable TUI and use text-only output
    #[arg(long)]
    text_only: bool,

    /// Show verbose output including FFmpeg messages
    #[arg(short, long)]
    verbose: bool,

    /// Force TUI mode even if terminal detection fails
    #[arg(long)]
    force_tui: bool,

    /// Run event system tests and exit
    #[arg(long)]
    test_events: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Run event system tests if requested
    if args.test_events {
        test_events::test_event_system();
        return Ok(());
    }

    // Validate input file is provided when not testing
    let input_path = args.input.ok_or_else(|| {
        anyhow::anyhow!("Input video file path is required. Use --help for usage information.")
    })?;

    let chunk_count = args.chunks.unwrap_or_else(|| {
        std::cmp::max(num_cpus::get() / 2, 4)
    });

    let thread_count = args.threads.unwrap_or_else(|| num_cpus::get());

    // Create args with validated input path
    let validated_args = Args {
        input: Some(input_path),
        output: args.output,
        chunks: args.chunks,
        duration_per_chunk: args.duration_per_chunk,
        skip: args.skip,
        threads: args.threads,
        text_only: args.text_only,
        verbose: args.verbose,
        force_tui: args.force_tui,
        test_events: args.test_events,
    };

    if validated_args.text_only {
        run_text_mode(&validated_args, chunk_count, thread_count)
    } else if validated_args.force_tui {
        run_tui_mode_forced(&validated_args, chunk_count, thread_count)
    } else {
        run_tui_mode(&validated_args, chunk_count, thread_count)
    }
}

fn run_tui_mode(args: &Args, chunk_count: usize, thread_count: usize) -> Result<()> {
    // Try to initialize TUI, fall back to text mode if it fails
    match TuiManager::new() {
        Ok(mut tui) => {
            let callback = tui.get_callback();
            let error_callback = tui.get_callback();

            // Clone args for the background thread
            let args_clone = Args {
                input: args.input.clone(),
                output: args.output.clone(),
                chunks: args.chunks,
                duration_per_chunk: args.duration_per_chunk,
                skip: args.skip,
                threads: args.threads,
                text_only: args.text_only,
                verbose: args.verbose,
                force_tui: args.force_tui,
                test_events: args.test_events,
            };

            // Start processing in a background thread
            std::thread::spawn(move || {
                if let Err(e) = process_video_with_callback(&args_clone, chunk_count, thread_count, callback) {
                    error_callback(ProcessingEvent::SystemError {
                        context: "Background processing".to_string(),
                        error: e.to_string(),
                    });
                }
            });

            // Run the TUI in the main thread
            tui.run()
        }
        Err(e) => {
            let callback = Box::new(|event: ProcessingEvent| {
                ConsoleOutputHandler.handle_event(&event);
            });
            callback(ProcessingEvent::ModeTransition {
                from: "TUI".to_string(),
                to: "text".to_string(),
                reason: format!("TUI initialization failed: {}", e),
            });
            run_text_mode(args, chunk_count, thread_count)
        }
    }
}

fn run_tui_mode_forced(args: &Args, chunk_count: usize, thread_count: usize) -> Result<()> {
    // Force TUI initialization without terminal checks
    let temp_callback = Box::new(|event: ProcessingEvent| {
        ConsoleOutputHandler.handle_event(&event);
    });
    temp_callback(ProcessingEvent::InitializationProgress {
        stage: "TUI Setup".to_string(),
        message: "Forcing TUI mode initialization".to_string(),
    });

    match TuiManager::new_forced() {
        Ok(mut tui) => {
            let callback = tui.get_callback();
            let error_callback = tui.get_callback();

            // Clone args for the background thread
            let args_clone = Args {
                input: args.input.clone(),
                output: args.output.clone(),
                chunks: args.chunks,
                duration_per_chunk: args.duration_per_chunk,
                skip: args.skip,
                threads: args.threads,
                text_only: args.text_only,
                verbose: args.verbose,
                force_tui: args.force_tui,
                test_events: args.test_events,
            };

            // Start processing in a background thread
            std::thread::spawn(move || {
                if let Err(e) = process_video_with_callback(&args_clone, chunk_count, thread_count, callback) {
                    error_callback(ProcessingEvent::SystemError {
                        context: "Background processing".to_string(),
                        error: e.to_string(),
                    });
                }
            });

            // Run the TUI in the main thread
            tui.run()
        }
        Err(e) => {
            let callback = Box::new(|event: ProcessingEvent| {
                ConsoleOutputHandler.handle_event(&event);
            });
            callback(ProcessingEvent::ModeTransition {
                from: "TUI (forced)".to_string(),
                to: "text".to_string(),
                reason: format!("Forced TUI initialization also failed: {}", e),
            });
            run_text_mode(args, chunk_count, thread_count)
        }
    }
}

fn run_text_mode(args: &Args, chunk_count: usize, thread_count: usize) -> Result<()> {
    let callback = Box::new(|event: ProcessingEvent| {
        ConsoleOutputHandler.handle_event(&event);
    });

    process_video_with_callback(&args, chunk_count, thread_count, callback)
}

fn process_video_with_callback(
    args: &Args,
    chunk_count: usize,
    thread_count: usize,
    callback: EventCallback,
) -> Result<()> {
    callback(ProcessingEvent::PhaseStarted {
        phase: 1,
        description: "Video Analysis & Intelligent Splitting".to_string(),
    });

    let input_path = args.input.as_ref().ok_or_else(|| {
        anyhow::anyhow!("Input video file path is required")
    })?;
    let mut video_processor = VideoProcessor::new(input_path)?;
    let video_info = video_processor.get_video_info(&callback)?;

    callback(ProcessingEvent::Progress {
        phase: 1,
        current: 1,
        total: 4,
        message: format!("Video: {}x{}, {:.1}fps, {:.1}s",
                        video_info.width, video_info.height,
                        video_info.fps, video_info.duration),
    });

    let chunks = if let Some(duration) = args.duration_per_chunk {
        video_processor.split_by_duration(duration, &callback)?
    } else {
        video_processor.split_by_count(chunk_count, &callback)?
    };

    callback(ProcessingEvent::PhaseCompleted {
        phase: 1,
        duration_ms: 0,
    });

    callback(ProcessingEvent::PhaseStarted {
        phase: 2,
        description: "Parallel Chunk Processing".to_string(),
    });

    // Create output directory for JSONL files
    std::fs::create_dir_all(&args.output)?;

    let qr_extractor = QrExtractor::new(thread_count, args.skip);
    let qr_results = qr_extractor.extract_from_chunks(&chunks, &args.output, &callback)?;

    callback(ProcessingEvent::PhaseCompleted {
        phase: 2,
        duration_ms: 0,
    });

    callback(ProcessingEvent::PhaseStarted {
        phase: 3,
        description: "QR Code Processing & File Reconstruction".to_string(),
    });

    let reconstructor = FileReconstructor::new(&args.output);
    let final_report = reconstructor.process_qr_data(qr_results, &callback)?;

    callback(ProcessingEvent::PhaseCompleted {
        phase: 3,
        duration_ms: 0,
    });

    callback(ProcessingEvent::AllCompleted {
        total_duration_ms: 0, // TODO: Calculate actual total duration
        files_extracted: final_report.files.len(),
    });

    callback(ProcessingEvent::FinalSummary {
        files_count: final_report.files.len(),
        output_dir: args.output.display().to_string(),
        total_duration_ms: 0, // TODO: Calculate actual total duration
    });

    Ok(())
}