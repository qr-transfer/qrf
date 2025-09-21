use anyhow::{anyhow, Result};
use ffmpeg_next as ffmpeg;
use std::path::PathBuf;

use crate::events::{EventCallback, ProcessingEvent};

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub total_frames: u64,
    pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct VideoChunk {
    pub id: usize,
    pub path: PathBuf,
    pub start_time: f64,
    pub duration: f64,
    pub end_time: f64,
}

pub struct VideoProcessor {
    input_path: PathBuf,
    video_info: Option<VideoInfo>,
}

impl VideoProcessor {
    pub fn new(input_path: &PathBuf) -> Result<Self> {
        ffmpeg::init().map_err(|e| anyhow!("Failed to initialize FFmpeg: {}", e))?;

        // Set log level to quiet to suppress warnings
        ffmpeg::log::set_level(ffmpeg::log::Level::Quiet);

        Ok(Self {
            input_path: input_path.clone(),
            video_info: None,
        })
    }

    pub fn get_video_info(&mut self, callback: &EventCallback) -> Result<VideoInfo> {
        callback(ProcessingEvent::Progress {
            phase: 1,
            current: 1,
            total: 4,
            message: "Opening video file...".to_string(),
        });

        let ictx = ffmpeg::format::input(&self.input_path)
            .map_err(|e| anyhow!("Failed to open video file: {}", e))?;

        let video_stream = ictx
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or_else(|| anyhow!("No video stream found"))?;

        let time_base = video_stream.time_base();
        let duration_frames = video_stream.duration();
        let duration_seconds = duration_frames as f64 * f64::from(time_base);

        let fps = video_stream.avg_frame_rate();
        let fps_value = fps.numerator() as f64 / fps.denominator() as f64;

        let total_frames = (duration_seconds * fps_value) as u64;

        let codec_params = video_stream.parameters();
        let (width, height) = match codec_params.medium() {
            ffmpeg::media::Type::Video => {
                match ffmpeg::codec::context::Context::from_parameters(codec_params)
                    .and_then(|ctx| ctx.decoder().video()) {
                    Ok(decoder) => (decoder.width(), decoder.height()),
                    Err(_) => (1920, 1080), // Default resolution
                }
            }
            _ => (1920, 1080)
        };

        let file_size = std::fs::metadata(&self.input_path)
            .map(|m| m.len())
            .unwrap_or(0);

        let video_info = VideoInfo {
            width,
            height,
            fps: fps_value,
            duration: duration_seconds,
            total_frames,
            file_size,
        };

        callback(ProcessingEvent::Progress {
            phase: 1,
            current: 2,
            total: 4,
            message: format!("Video analyzed: {}x{}, {:.1}fps, {:.1}s, {} frames",
                           width, height, fps_value, duration_seconds, total_frames),
        });

        self.video_info = Some(video_info.clone());
        Ok(video_info)
    }

    pub fn split_by_count(&self, chunk_count: usize, callback: &EventCallback) -> Result<Vec<VideoChunk>> {
        let video_info = self.video_info.as_ref()
            .ok_or_else(|| anyhow!("Video info not available. Call get_video_info first."))?;

        callback(ProcessingEvent::Progress {
            phase: 1,
            current: 3,
            total: 4,
            message: format!("Splitting video into {} chunks...", chunk_count),
        });

        let chunk_duration = video_info.duration / chunk_count as f64;
        let mut chunks = Vec::with_capacity(chunk_count);

        for i in 0..chunk_count {
            let start_time = i as f64 * chunk_duration;
            let end_time = if i == chunk_count - 1 {
                video_info.duration
            } else {
                (i + 1) as f64 * chunk_duration
            };

            let chunk_path = PathBuf::from(format!("chunk_{:03}.mp4", i + 1));

            chunks.push(VideoChunk {
                id: i,
                path: chunk_path,
                start_time,
                duration: end_time - start_time,
                end_time,
            });
        }

        self.create_chunk_files(&chunks, callback)?;

        callback(ProcessingEvent::Progress {
            phase: 1,
            current: 4,
            total: 4,
            message: format!("Created {} video chunks", chunks.len()),
        });

        Ok(chunks)
    }

    pub fn split_by_duration(&self, duration_per_chunk: f64, callback: &EventCallback) -> Result<Vec<VideoChunk>> {
        let video_info = self.video_info.as_ref()
            .ok_or_else(|| anyhow!("Video info not available. Call get_video_info first."))?;

        let chunk_count = (video_info.duration / duration_per_chunk).ceil() as usize;

        callback(ProcessingEvent::Progress {
            phase: 1,
            current: 3,
            total: 4,
            message: format!("Splitting video into chunks of {:.1}s each ({} chunks)...",
                           duration_per_chunk, chunk_count),
        });

        let mut chunks = Vec::with_capacity(chunk_count);

        for i in 0..chunk_count {
            let start_time = i as f64 * duration_per_chunk;
            let end_time = ((i + 1) as f64 * duration_per_chunk).min(video_info.duration);

            let chunk_path = PathBuf::from(format!("chunk_{:03}.mp4", i + 1));

            chunks.push(VideoChunk {
                id: i,
                path: chunk_path,
                start_time,
                duration: end_time - start_time,
                end_time,
            });
        }

        self.create_chunk_files(&chunks, callback)?;

        callback(ProcessingEvent::Progress {
            phase: 1,
            current: 4,
            total: 4,
            message: format!("Created {} video chunks", chunks.len()),
        });

        Ok(chunks)
    }

    fn create_chunk_files(&self, chunks: &[VideoChunk], callback: &EventCallback) -> Result<()> {
        for (idx, chunk) in chunks.iter().enumerate() {
            self.split_video_segment_embedded(chunk)?;

            callback(ProcessingEvent::Progress {
                phase: 1,
                current: 3,
                total: 4,
                message: format!("Created chunk {} of {} ({:.1}s-{:.1}s)",
                               idx + 1, chunks.len(), chunk.start_time, chunk.end_time),
            });
        }

        Ok(())
    }

    fn split_video_segment_embedded(&self, chunk: &VideoChunk) -> Result<()> {
        // Use external ffmpeg to avoid borrowing issues
        use std::process::Command;

        let start_time = format!("{:.3}", chunk.start_time);
        let duration = format!("{:.3}", chunk.duration);

        let output = Command::new("ffmpeg")
            .arg("-i")
            .arg(&self.input_path)
            .arg("-ss")
            .arg(&start_time)
            .arg("-t")
            .arg(&duration)
            .arg("-c")
            .arg("copy")
            .arg("-avoid_negative_ts")
            .arg("make_zero")
            .arg("-y")
            .arg(&chunk.path)
            .output()
            .map_err(|e| anyhow!("Failed to execute ffmpeg: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("FFmpeg failed: {}", stderr));
        }

        Ok(())
    }

}