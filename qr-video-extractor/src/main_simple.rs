use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input video file path
    input: PathBuf,

    /// Output directory for decoded files
    #[arg(short, long, default_value = "decoded_files")]
    output: PathBuf,

    /// Number of video chunks to create (default: max(cpu_cores/2, 4))
    #[arg(short, long)]
    chunks: Option<usize>,

    /// Skip frames (process every Nth frame)
    #[arg(short, long, default_value_t = 1)]
    skip: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let chunk_count = args.chunks.unwrap_or_else(|| {
        std::cmp::max(num_cpus::get() / 2, 4)
    });

    println!("QR Video Files Processor");
    println!("========================");
    println!("Input: {}", args.input.display());
    println!("Output: {}", args.output.display());
    println!("Chunks: {}", chunk_count);
    println!("Skip frames: {}", args.skip);
    println!();

    // Phase 1: Video Analysis & Splitting
    println!("Phase 1: Video Analysis & Intelligent Splitting");
    let video_info = get_video_info(&args.input)?;
    println!("Video: {}x{}, {:.1}fps, {:.1}s",
             video_info.width, video_info.height, video_info.fps, video_info.duration);

    let chunk_duration = video_info.duration / chunk_count as f64;
    println!("Creating {} chunks of {:.1}s each...", chunk_count, chunk_duration);

    let chunks = create_video_chunks(&args.input, chunk_count, video_info.duration)?;
    println!("Created {} video chunks", chunks.len());
    println!();

    // Phase 2: Parallel Chunk Processing
    println!("Phase 2: Parallel Chunk Processing");
    println!("Extracting QR codes from {} chunks...", chunks.len());

    // For now, just show that we would process the chunks
    for (i, chunk) in chunks.iter().enumerate() {
        println!("Would process chunk {}: {} ({:.1}s-{:.1}s)",
                 i + 1, chunk.display(), i as f64 * chunk_duration, (i + 1) as f64 * chunk_duration);
    }
    println!();

    // Phase 3: File Reconstruction
    println!("Phase 3: QR Code Processing & File Reconstruction");
    std::fs::create_dir_all(&args.output)?;
    println!("Created output directory: {}", args.output.display());
    println!();

    println!("Processing completed successfully!");
    println!("Note: This is a simplified version. Full QR extraction and file reconstruction");
    println!("would be implemented in the complete version.");

    Ok(())
}

#[derive(Debug)]
struct VideoInfo {
    width: u32,
    height: u32,
    fps: f64,
    duration: f64,
}

fn get_video_info(input_path: &PathBuf) -> Result<VideoInfo> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(input_path)
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("ffprobe failed"));
    }

    let json_str = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&json_str)?;

    let streams = json["streams"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No streams found"))?;

    let video_stream = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or_else(|| anyhow::anyhow!("No video stream found"))?;

    let width = video_stream["width"].as_u64().unwrap_or(0) as u32;
    let height = video_stream["height"].as_u64().unwrap_or(0) as u32;

    let fps_str = video_stream["r_frame_rate"].as_str().unwrap_or("30/1");
    let fps_parts: Vec<&str> = fps_str.split('/').collect();
    let fps = if fps_parts.len() == 2 {
        let num: f64 = fps_parts[0].parse().unwrap_or(30.0);
        let den: f64 = fps_parts[1].parse().unwrap_or(1.0);
        num / den
    } else {
        30.0
    };

    let duration_str = json["format"]["duration"].as_str().unwrap_or("0");
    let duration: f64 = duration_str.parse().unwrap_or(0.0);

    Ok(VideoInfo {
        width,
        height,
        fps,
        duration,
    })
}

fn create_video_chunks(input_path: &PathBuf, chunk_count: usize, total_duration: f64) -> Result<Vec<PathBuf>> {
    let chunk_duration = total_duration / chunk_count as f64;
    let mut chunks = Vec::new();

    for i in 0..chunk_count {
        let start_time = i as f64 * chunk_duration;
        let duration = if i == chunk_count - 1 {
            total_duration - start_time
        } else {
            chunk_duration
        };

        let chunk_path = PathBuf::from(format!("chunk_{:03}.mp4", i + 1));

        println!("Creating chunk {}: {:.1}s-{:.1}s", i + 1, start_time, start_time + duration);

        let output = Command::new("ffmpeg")
            .args([
                "-i", &input_path.to_string_lossy(),
                "-ss", &format!("{:.3}", start_time),
                "-t", &format!("{:.3}", duration),
                "-c", "copy",
                "-avoid_negative_ts", "make_zero",
                "-y",
                &chunk_path.to_string_lossy(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("FFmpeg failed: {}", stderr));
        }

        chunks.push(chunk_path);
    }

    Ok(chunks)
}