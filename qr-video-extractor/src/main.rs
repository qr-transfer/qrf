use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

mod tui;
mod video;
mod qr_extraction;
mod file_reconstruction;
mod events;
mod error_logger;
mod resume_state;
mod resume_controller;
mod error_handler;
mod completion_detector;

use tui::TuiManager;
use video::VideoProcessor;
use qr_extraction::QrExtractor;
use file_reconstruction::FileReconstructor;
use events::{EventCallback, ProcessingEvent, ConsoleOutputHandler, OutputHandler};
use resume_controller::{ResumeController, ResumePoint};
use error_handler::ErrorHandler;
use completion_detector::CompletionDetector;

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

    /// Skip frames (process every Nth frame) - 0 for maximum quality
    #[arg(short, long, default_value_t = 0)]
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

    /// Run TUI demo with simulated 8-chunk processing
    #[arg(long)]
    demo_tui: bool,

    /// Resume processing from previous interrupted session
    #[arg(long)]
    resume: bool,

    /// Check completion status of existing JSONL files
    #[arg(long)]
    check_status: bool,

    /// Run only Phase 3 (file reconstruction) using existing JSONL files
    #[arg(long)]
    phase3_only: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Run event system tests if requested
    if args.test_events {
        println!("Event system tests temporarily disabled");
        return Ok(());
    }

    // Run TUI demo if requested
    if args.demo_tui {
        return run_tui_demo();
    }

    // Check completion status if requested
    if args.check_status {
        return check_completion_status(&args);
    }

    // Run Phase 3 only if requested
    if args.phase3_only {
        return run_phase3_only(&args);
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
        demo_tui: args.demo_tui,
        resume: args.resume,
        check_status: args.check_status,
        phase3_only: args.phase3_only,
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
                demo_tui: args.demo_tui,
                resume: args.resume,
                check_status: args.check_status,
        phase3_only: args.phase3_only,
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
                demo_tui: args.demo_tui,
                resume: args.resume,
                check_status: args.check_status,
        phase3_only: args.phase3_only,
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
    // Initialize logging for the entire process
    let log_path = args.output.join("processing.log");
    let process_logger = crate::error_logger::ErrorLogger::new(&log_path.to_string_lossy())
        .unwrap_or_else(|_| crate::error_logger::ErrorLogger::new("/tmp/processing.log").unwrap());

    process_logger.log_info(&format!("=== PROCESSING STARTED === Version: 0.1.0"));
    process_logger.log_info(&format!("Input: {}", args.input.as_ref().unwrap().display()));
    process_logger.log_info(&format!("Output: {}", args.output.display()));
    process_logger.log_info(&format!("Chunks: {}, Threads: {}", chunk_count, thread_count));

    callback(ProcessingEvent::PhaseStarted {
        phase: 1,
        description: "Video Analysis & Intelligent Splitting".to_string(),
    });

    process_logger.log_processing_phase("PHASE_1", "Started video analysis and splitting");

    // CRITICAL: Preserve files when resuming, ask confirmation when cleaning
    if !args.resume {
        // Count existing files before potential cleaning
        let mut existing_jsonl_count = 0;
        let mut existing_chunk_count = 0;
        for i in 1..=50 {
            if args.output.join(format!("chunk_{:03}.jsonl", i)).exists() {
                existing_jsonl_count += 1;
            }
            if args.output.join(format!("chunk_{:03}.mp4", i)).exists() {
                existing_chunk_count += 1;
            }
        }

        if existing_jsonl_count > 0 || existing_chunk_count > 0 {
            // ASK FOR CONFIRMATION BEFORE CLEANING
            eprintln!("⚠️  EXISTING FILES DETECTED:");
            eprintln!("   {} JSONL files with QR code data", existing_jsonl_count);
            eprintln!("   {} video chunk files", existing_chunk_count);
            eprintln!();
            eprintln!("⚠️  This will DELETE all existing processing data!");
            eprintln!("   To preserve and continue from where you left off, use: --resume");
            eprintln!();
            eprint!("Continue and DELETE existing files? [y/N]: ");

            use std::io::{self, Write};
            io::stdout().flush().ok();

            let mut input = String::new();
            io::stdin().read_line(&mut input).ok();
            let response = input.trim().to_lowercase();

            if response != "y" && response != "yes" {
                eprintln!("❌ Operation cancelled. Use --resume to preserve existing files.");
                eprintln!("💡 Example: ./target/release/qr-video-files --resume {} --chunks {} --threads {}",
                         args.input.as_ref().unwrap().display(), chunk_count, thread_count);
                return Ok(());
            }

            process_logger.log_warning("USER_CONFIRMED", &format!("User confirmed deletion of {} JSONL files and {} chunk files", existing_jsonl_count, existing_chunk_count));
        }

        process_logger.log_info("FRESH START: Cleaning target folder for new processing");

        // Remove existing files from previous runs
        for i in 1..=50 { // Clean up to 50 possible chunks
            let chunk_file = args.output.join(format!("chunk_{:03}.mp4", i));
            if chunk_file.exists() {
                std::fs::remove_file(&chunk_file).ok();
            }
            let jsonl_file = args.output.join(format!("chunk_{:03}.jsonl", i));
            if jsonl_file.exists() {
                std::fs::remove_file(&jsonl_file).ok();
            }
        }

        let old_report = args.output.join("integrity_report.json");
        if old_report.exists() {
            std::fs::remove_file(&old_report).ok();
        }
    } else {
        // RESUME MODE - ABSOLUTELY NO CLEANING
        process_logger.log_info("🔄 RESUME MODE: Preserving ALL existing files for incremental processing");

        // Count preserved files
        let mut preserved_jsonl = 0;
        let mut preserved_chunks = 0;
        for i in 1..=50 {
            if args.output.join(format!("chunk_{:03}.jsonl", i)).exists() {
                preserved_jsonl += 1;
            }
            if args.output.join(format!("chunk_{:03}.mp4", i)).exists() {
                preserved_chunks += 1;
            }
        }

        process_logger.log_info(&format!("PRESERVED: {} JSONL files, {} chunk files for resume processing", preserved_jsonl, preserved_chunks));

        if preserved_jsonl == 0 && preserved_chunks == 0 {
            process_logger.log_info("No existing files found - will start fresh processing");
        }
    }

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
        video_processor.split_by_duration(duration, &args.output, &callback)?
    } else {
        video_processor.split_by_count(chunk_count, &args.output, &callback)?
    };

    process_logger.log_processing_phase("PHASE_1", &format!("Created {} video chunks", chunks.len()));

    callback(ProcessingEvent::PhaseCompleted {
        phase: 1,
        duration_ms: 0,
    });

    callback(ProcessingEvent::PhaseStarted {
        phase: 2,
        description: "Parallel Chunk Processing & JSONL Creation".to_string(),
    });

    process_logger.log_processing_phase("PHASE_2", "Started parallel chunk processing");

    // Create output directory for JSONL files
    std::fs::create_dir_all(&args.output)?;

    // Phase 2: Extract QR codes and create individual chunk JSONL files
    let qr_extractor = QrExtractor::new(thread_count, args.skip);
    process_logger.log_info(&format!("Starting QR extraction with {} threads, skip_frames: {}", thread_count, args.skip));

    let qr_results = qr_extractor.extract_from_chunks(&chunks, &args.output, &callback)?;

    process_logger.log_processing_phase("PHASE_2", &format!("COMPLETED - {} QR codes extracted", qr_results.qr_codes.len()));

    callback(ProcessingEvent::PhaseCompleted {
        phase: 2,
        duration_ms: 0,
    });

    callback(ProcessingEvent::PhaseStarted {
        phase: 3,
        description: "JSONL Combination & File Reconstruction".to_string(),
    });

    process_logger.log_processing_phase("PHASE_3", "Started JSONL combination and file reconstruction");

    // Phase 3: Combine all JSONLs, split by metadata, then reconstruct files
    let reconstructor = FileReconstructor::new(&args.output);
    let final_report = reconstructor.process_combined_jsonl_files(&args.output, &callback)?;

    process_logger.log_processing_phase("PHASE_3", &format!("COMPLETED - {} files reconstructed", final_report.files.len()));

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

fn run_tui_demo() -> Result<()> {
    use std::thread;

    println!("🎬 Starting TUI Demo with 8-chunk processing simulation...");
    println!("This demonstrates the TUI interface with parallel chunk processing.");
    println!("Press Ctrl+C to stop or wait for completion.\n");

    match TuiManager::new_forced() {
        Ok(mut tui) => {
            let callback = tui.get_callback();

            // Start demo simulation in background thread
            thread::spawn(move || {
                simulate_8_chunk_processing(callback);
            });

            // Run the TUI
            tui.run()
        }
        Err(e) => {
            println!("TUI Demo failed to start: {}", e);
            println!("Try running this in a real terminal for full TUI experience.");

            // Fall back to console demo
            println!("\nRunning console simulation instead...");
            let callback = Box::new(|event: ProcessingEvent| {
                ConsoleOutputHandler.handle_event(&event);
            });
            simulate_8_chunk_processing(callback);
            Ok(())
        }
    }
}

fn simulate_8_chunk_processing(callback: EventCallback) {
    use std::thread;
    use std::time::Duration;

    // Phase 1: Video Analysis
    callback(ProcessingEvent::PhaseStarted {
        phase: 1,
        description: "Video Analysis & Intelligent Splitting".to_string(),
    });

    callback(ProcessingEvent::Progress {
        phase: 1,
        current: 1,
        total: 4,
        message: "Opening demo video file...".to_string(),
    });
    thread::sleep(Duration::from_millis(500));

    callback(ProcessingEvent::Progress {
        phase: 1,
        current: 2,
        total: 4,
        message: "Video: 1920x1080, 30.0fps, 120.0s".to_string(),
    });
    thread::sleep(Duration::from_millis(300));

    callback(ProcessingEvent::Progress {
        phase: 1,
        current: 3,
        total: 4,
        message: "Splitting video into 8 chunks...".to_string(),
    });
    thread::sleep(Duration::from_millis(800));

    for i in 0..8 {
        let start_time = i as f64 * 15.0;
        let end_time = (i + 1) as f64 * 15.0;
        callback(ProcessingEvent::Progress {
            phase: 1,
            current: 3,
            total: 4,
            message: format!("Created chunk {} of 8 ({:.1}s-{:.1}s)", i + 1, start_time, end_time),
        });
        thread::sleep(Duration::from_millis(200));
    }

    callback(ProcessingEvent::Progress {
        phase: 1,
        current: 4,
        total: 4,
        message: "Created 8 video chunks".to_string(),
    });
    thread::sleep(Duration::from_millis(300));

    callback(ProcessingEvent::PhaseCompleted {
        phase: 1,
        duration_ms: 2500,
    });

    // Phase 2: Parallel Chunk Processing
    callback(ProcessingEvent::PhaseStarted {
        phase: 2,
        description: "Parallel Chunk Processing (8 threads)".to_string(),
    });

    // Start all 8 chunks
    for i in 0..8 {
        callback(ProcessingEvent::ChunkStarted {
            chunk_id: i,
            chunk_name: format!("chunk_{:03}.mp4", i + 1),
        });
        thread::sleep(Duration::from_millis(150));
    }

    // Simulate parallel processing with random completion times
    let chunk_processing_times = vec![1200, 1500, 1100, 1800, 1300, 1400, 1600, 1000];
    let chunk_qr_counts = vec![150, 143, 167, 89, 134, 156, 121, 178];

    // Simulate progress updates
    for step in 0..15 {
        for i in 0..8 {
            let progress = (step + 1) as f64 / 15.0;
            let frames = (progress * 450.0) as usize;
            let qrs = (progress * chunk_qr_counts[i] as f64) as usize;

            if step * 100 < chunk_processing_times[i] {
                callback(ProcessingEvent::ChunkProgress {
                    chunk_id: i,
                    frames_processed: frames,
                    qr_codes_found: qrs,
                    status: format!("Processing frame {}", frames),
                });
            }
        }
        thread::sleep(Duration::from_millis(200));
    }

    // Complete chunks in staggered fashion
    let mut completion_order = vec![7, 2, 0, 4, 1, 5, 6, 3]; // Realistic completion order
    for &chunk_id in &completion_order {
        thread::sleep(Duration::from_millis(300));
        callback(ProcessingEvent::ChunkCompleted {
            chunk_id,
            qr_codes_found: chunk_qr_counts[chunk_id],
            jsonl_file: format!("chunk_{:03}.jsonl", chunk_id + 1),
            duration_ms: chunk_processing_times[chunk_id] as u64,
        });
    }

    callback(ProcessingEvent::PhaseCompleted {
        phase: 2,
        duration_ms: 8500,
    });

    // Phase 3: File Reconstruction
    callback(ProcessingEvent::PhaseStarted {
        phase: 3,
        description: "QR Code Processing & File Reconstruction".to_string(),
    });

    thread::sleep(Duration::from_millis(500));

    // Simulate file reconstruction
    let files = vec!["document.pdf", "image.jpg", "data.json"];
    for (i, file) in files.iter().enumerate() {
        thread::sleep(Duration::from_millis(400));
        callback(ProcessingEvent::FileReconstructed {
            file_name: file.to_string(),
            file_size: ((i + 1) * 1024 * 1024) as u64,
            checksum_valid: true,
            output_path: format!("output/{}", file),
        });
    }

    callback(ProcessingEvent::PhaseCompleted {
        phase: 3,
        duration_ms: 1500,
    });

    // Final completion
    callback(ProcessingEvent::AllCompleted {
        total_duration_ms: 12500,
        files_extracted: 3,
    });

    callback(ProcessingEvent::FinalSummary {
        files_count: 3,
        output_dir: "output/".to_string(),
        total_duration_ms: 12500,
    });

    // Keep demo running for a bit to see final state
    thread::sleep(Duration::from_secs(2));
}

fn check_completion_status(args: &Args) -> Result<()> {
    println!("🔍 Checking Completion Status...");
    println!("================================");

    let output_dir = &args.output;

    // Get video info for frame calculations
    if let Some(input_path) = &args.input {
        let mut video_processor = VideoProcessor::new(input_path)?;
        let dummy_callback: EventCallback = Box::new(|_| {});
        let video_info = video_processor.get_video_info(&dummy_callback)?;

        let chunk_count = args.chunks.unwrap_or_else(|| std::cmp::max(num_cpus::get() / 2, 4));
        let detector = CompletionDetector::new(
            video_info.total_frames,
            video_info.duration,
            video_info.fps,
            chunk_count,
            args.skip,
            output_dir
        )?;

        println!("Video: {}x{}, {:.1}fps, {:.1}s, {} frames",
                video_info.width, video_info.height, video_info.fps, video_info.duration, video_info.total_frames);
        println!("Expected: {} chunks × ~{} frames each\n", chunk_count, video_info.total_frames / chunk_count as u64);

        // Check overall completion
        let (is_complete, summary) = detector.verify_processing_completeness(output_dir)?;
        println!("Overall Status: {}", summary);
        println!("Ready for Phase 3: {}\n", if is_complete { "✅ YES" } else { "❌ NO" });

        // Get detailed resume points
        let resume_points = detector.get_all_resume_points(output_dir)?;

        println!("Detailed Chunk Analysis:");
        println!("========================");
        for point in &resume_points {
            let status_icon = if point.should_resume { "⏳" } else { "✅" };
            println!("{} Chunk {}: {} (from frame {}, {} QR codes found)",
                    status_icon, point.chunk_id + 1, point.completion_status,
                    point.resume_from_frame, point.qr_codes_already_found);
        }

        let incomplete_chunks: Vec<usize> = resume_points.iter()
            .filter(|p| p.should_resume)
            .map(|p| p.chunk_id + 1)
            .collect();

        if !incomplete_chunks.is_empty() {
            println!("\n🔄 Resume Commands:");
            println!("==================");
            println!("Resume incomplete chunks: ./target/release/qr-video-files --resume {} --chunks {} --threads {}",
                    input_path.display(), chunk_count, args.threads.unwrap_or(num_cpus::get()));
            println!("Incomplete chunks will continue from their last processed frame automatically.");
        } else {
            println!("\n✅ All chunks complete! Ready for file reconstruction:");
            println!("====================================================");
            println!("Process existing JSONLs: ./target/release/qr-video-files --resume {} --chunks {}",
                    input_path.display(), chunk_count);
        }

        let total_qr_codes: usize = resume_points.iter().map(|p| p.qr_codes_already_found).sum();
        println!("\n📊 Summary: {} total QR codes found across all chunks", total_qr_codes);

    } else {
        println!("❌ No input file specified. Use: --check-status <video-file>");
    }

    Ok(())
}

fn run_phase3_only(args: &Args) -> Result<()> {
    println!("🔧 Running Phase 3 Only - File Reconstruction");
    println!("==============================================");

    let output_dir = &args.output;

    // Check if JSONL files exist
    let mut jsonl_count = 0;
    for i in 1..=20 {
        let jsonl_file = output_dir.join(format!("chunk_{:03}.jsonl", i));
        if jsonl_file.exists() {
            jsonl_count += 1;
        }
    }

    if jsonl_count == 0 {
        println!("❌ No JSONL files found in {}", output_dir.display());
        println!("💡 Run QR extraction first or use --resume to process video");
        return Ok(());
    }

    println!("✅ Found {} JSONL files in {}", jsonl_count, output_dir.display());

    // Create a minimal callback for console output
    let callback: EventCallback = Box::new(|event| {
        ConsoleOutputHandler.handle_event(&event);
    });

    // Initialize logging
    let log_path = output_dir.join("processing.log");
    let process_logger = crate::error_logger::ErrorLogger::new(&log_path.to_string_lossy())
        .unwrap_or_else(|_| crate::error_logger::ErrorLogger::new("/tmp/processing.log").unwrap());

    process_logger.log_info("=== PHASE 3 ONLY MODE ===");
    process_logger.log_info(&format!("Processing {} JSONL files for file reconstruction", jsonl_count));

    callback(ProcessingEvent::PhaseStarted {
        phase: 3,
        description: "JSONL Combination & File Reconstruction (Phase 3 Only)".to_string(),
    });

    // Run Phase 3 file reconstruction
    let reconstructor = FileReconstructor::new(output_dir);
    let final_report = reconstructor.process_combined_jsonl_files(output_dir, &callback)?;

    process_logger.log_info(&format!("Phase 3 completed: {} files reconstructed", final_report.files.len()));

    callback(ProcessingEvent::PhaseCompleted {
        phase: 3,
        duration_ms: 0,
    });

    callback(ProcessingEvent::FinalSummary {
        files_count: final_report.files.len(),
        output_dir: output_dir.display().to_string(),
        total_duration_ms: 0,
    });

    println!("\n✅ Phase 3 completed successfully!");
    println!("📊 Files reconstructed: {}", final_report.files.len());
    println!("📁 Output directory: {}", output_dir.display());

    Ok(())
}