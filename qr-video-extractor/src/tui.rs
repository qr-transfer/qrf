use anyhow::{anyhow, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use std::io::{self, IsTerminal};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::events::{EventCallback, ProcessingEvent};

pub struct TuiState {
    pub phases: Vec<PhaseInfo>,
    pub current_phase: usize,
    pub messages: Vec<String>,
    pub should_quit: bool,
    pub chunks: Vec<ChunkInfo>,
    pub total_frames: u64,
    pub frames_processed: u64,
    pub start_time: Option<std::time::Instant>,
}

#[derive(Clone)]
pub struct ChunkInfo {
    pub id: usize,
    pub name: String,
    pub status: ChunkStatus,
    pub frames_processed: usize,
    pub qr_codes_found: usize,
    pub jsonl_file: Option<String>,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ChunkStatus {
    Pending,
    Processing,
    Completed,
    Error,
}

#[derive(Clone)]
pub struct PhaseInfo {
    pub name: String,
    pub status: PhaseStatus,
    pub progress: f64,
    pub message: String,
    pub duration_ms: Option<u64>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum PhaseStatus {
    Pending,
    InProgress,
    Completed,
    Error,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            phases: vec![
                PhaseInfo {
                    name: "Video Analysis & Intelligent Splitting".to_string(),
                    status: PhaseStatus::Pending,
                    progress: 0.0,
                    message: "Waiting to start...".to_string(),
                    duration_ms: None,
                },
                PhaseInfo {
                    name: "Parallel Chunk Processing".to_string(),
                    status: PhaseStatus::Pending,
                    progress: 0.0,
                    message: "Waiting to start...".to_string(),
                    duration_ms: None,
                },
                PhaseInfo {
                    name: "QR Code Processing & File Reconstruction".to_string(),
                    status: PhaseStatus::Pending,
                    progress: 0.0,
                    message: "Waiting to start...".to_string(),
                    duration_ms: None,
                },
            ],
            current_phase: 0,
            messages: Vec::new(),
            should_quit: false,
            chunks: Vec::new(),
            total_frames: 0,
            frames_processed: 0,
            start_time: None,
        }
    }

    pub fn handle_event(&mut self, event: ProcessingEvent) {
        match event {
            ProcessingEvent::PhaseStarted { phase, description } => {
                // Start timing on first phase
                if phase == 1 && self.start_time.is_none() {
                    self.start_time = Some(std::time::Instant::now());
                }

                let phase_idx = (phase as usize).saturating_sub(1);
                if phase_idx < self.phases.len() {
                    self.phases[phase_idx].status = PhaseStatus::InProgress;
                    self.phases[phase_idx].message = "Starting...".to_string();
                    self.current_phase = phase_idx;
                }
                self.messages.push(format!("Started: {}", description));
            }
            ProcessingEvent::Progress { phase, current, total, message } => {
                let phase_idx = (phase as usize).saturating_sub(1);
                if phase_idx < self.phases.len() {
                    self.phases[phase_idx].progress = if total > 0 {
                        current as f64 / total as f64 * 100.0
                    } else {
                        0.0
                    };
                    self.phases[phase_idx].message = message.clone();
                }

                // Extract total frame count from video analysis message
                if phase == 1 && message.contains("frames") {
                    if let Some(frames_str) = message.split("frames").next() {
                        if let Some(last_part) = frames_str.split_whitespace().last() {
                            if let Ok(frames) = last_part.replace(",", "").parse::<u64>() {
                                self.total_frames = frames;
                            }
                        }
                    }
                }

                self.messages.push(format!("Progress [{}]: {}", phase, message));
                if self.messages.len() > 100 {
                    self.messages.remove(0);
                }
            }
            ProcessingEvent::PhaseCompleted { phase, duration_ms } => {
                let phase_idx = (phase as usize).saturating_sub(1);
                if phase_idx < self.phases.len() {
                    self.phases[phase_idx].status = PhaseStatus::Completed;
                    self.phases[phase_idx].progress = 100.0;
                    self.phases[phase_idx].duration_ms = Some(duration_ms);
                    self.phases[phase_idx].message = format!("Completed in {}ms", duration_ms);
                }
                self.messages.push(format!("Completed: Phase {} ({}ms)", phase, duration_ms));
            }
            ProcessingEvent::Error { phase, error } => {
                let phase_idx = (phase as usize).saturating_sub(1);
                if phase_idx < self.phases.len() {
                    self.phases[phase_idx].status = PhaseStatus::Error;
                    self.phases[phase_idx].message = format!("Error: {}", error);
                }
                self.messages.push(format!("Error in Phase {}: {}", phase, error));
            }
            ProcessingEvent::AllCompleted { total_duration_ms, files_extracted } => {
                self.messages.push(format!("🎉 All processing completed! Extracted {} files in {}ms", files_extracted, total_duration_ms));
                self.messages.push("Press 'q' to quit".to_string());
            }
            ProcessingEvent::ChunkStarted { chunk_id, chunk_name } => {
                let chunk_info = ChunkInfo {
                    id: chunk_id,
                    name: chunk_name.clone(),
                    status: ChunkStatus::Processing,
                    frames_processed: 0,
                    qr_codes_found: 0,
                    jsonl_file: None,
                    duration_ms: None,
                };

                // Find existing chunk or add new one
                if let Some(existing) = self.chunks.iter_mut().find(|c| c.id == chunk_id) {
                    existing.status = ChunkStatus::Processing;
                } else {
                    self.chunks.push(chunk_info);
                }

                self.messages.push(format!("Started processing chunk {}: {}", chunk_id + 1, chunk_name));
            }
            ProcessingEvent::ChunkProgress { chunk_id, frames_processed, qr_codes_found, status } => {
                if let Some(chunk) = self.chunks.iter_mut().find(|c| c.id == chunk_id) {
                    chunk.frames_processed = frames_processed;
                    chunk.qr_codes_found = qr_codes_found;
                    chunk.status = ChunkStatus::Processing;
                }
                self.messages.push(format!("Chunk {}: {} - {} frames, {} QR codes", chunk_id + 1, status, frames_processed, qr_codes_found));
            }
            ProcessingEvent::ChunkCompleted { chunk_id, qr_codes_found, jsonl_file, duration_ms } => {
                if let Some(chunk) = self.chunks.iter_mut().find(|c| c.id == chunk_id) {
                    chunk.status = ChunkStatus::Completed;
                    chunk.qr_codes_found = qr_codes_found;
                    chunk.jsonl_file = Some(jsonl_file.clone());
                    chunk.duration_ms = Some(duration_ms);
                }
                self.messages.push(format!("✅ Chunk {} completed: {} QR codes → {} ({}ms)", chunk_id + 1, qr_codes_found, jsonl_file, duration_ms));
            }
            ProcessingEvent::FileReconstructed { file_name, file_size, checksum_valid, output_path } => {
                let status = if checksum_valid { "✅" } else { "⚠️" };
                self.messages.push(format!("{} File reconstructed: {} ({} bytes) → {}", status, file_name, file_size, output_path));
            }
            ProcessingEvent::ChecksumValidation { file_name, checksum_type, expected, actual, valid } => {
                let status = if valid { "✅" } else { "❌" };
                self.messages.push(format!("{} {}: {} (expected: {}, actual: {})", status, checksum_type, file_name, expected, actual));
            }
            ProcessingEvent::SystemError { context, error } => {
                self.messages.push(format!("🚨 System Error in {}: {}", context, error));
                if self.messages.len() > 100 {
                    self.messages.remove(0);
                }
            }
            ProcessingEvent::InitializationProgress { stage, message } => {
                self.messages.push(format!("🔧 {}: {}", stage, message));
                if self.messages.len() > 100 {
                    self.messages.remove(0);
                }
            }
            ProcessingEvent::FinalSummary { files_count, output_dir, total_duration_ms } => {
                self.messages.push(format!("📊 Final Summary:"));
                self.messages.push(format!("   Files extracted: {}", files_count));
                self.messages.push(format!("   Output directory: {}", output_dir));
                self.messages.push(format!("   Total duration: {}ms", total_duration_ms));
                self.messages.push("Press 'q' to quit".to_string());
            }
            ProcessingEvent::ModeTransition { from, to, reason } => {
                self.messages.push(format!("🔄 Mode transition: {} → {} ({})", from, to, reason));
                if self.messages.len() > 100 {
                    self.messages.remove(0);
                }
            }
            ProcessingEvent::FrameProgress { chunk_id, frames_processed, total_frames, qr_codes_found } => {
                // Update chunk-specific frame count
                if let Some(chunk) = self.chunks.iter_mut().find(|c| c.id == chunk_id) {
                    chunk.frames_processed = frames_processed as usize;
                    chunk.qr_codes_found = qr_codes_found;
                }

                // Update total frame progress
                self.frames_processed = self.chunks.iter().map(|c| c.frames_processed as u64).sum();

                // Only log significant progress updates to avoid spam
                if frames_processed % 500 == 0 || frames_processed == total_frames {
                    let progress = (frames_processed as f64 / total_frames as f64 * 100.0).min(100.0);
                    self.messages.push(format!("📊 Chunk {}: {}/{} frames ({:.1}%) - {} QR codes",
                                              chunk_id + 1, frames_processed, total_frames, progress, qr_codes_found));
                    if self.messages.len() > 100 {
                        self.messages.remove(0);
                    }
                }
            }
        }
    }
}

pub struct TuiManager {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    state: Arc<Mutex<TuiState>>,
}

impl TuiManager {
    pub fn new() -> Result<Self> {
        // Check if we're in a proper terminal
        if !io::stdout().is_terminal() {
            return Err(anyhow!("Not running in a TTY"));
        }

        // Try to enable raw mode
        enable_raw_mode().map_err(|e| anyhow!("Failed to enable raw mode: {}", e))?;

        let mut stdout = io::stdout();

        // Try to setup terminal
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| {
                let _ = disable_raw_mode();
                anyhow!("Failed to setup terminal: {}", e)
            })?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)
            .map_err(|e| {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
                anyhow!("Failed to create terminal: {}", e)
            })?;

        let state = Arc::new(Mutex::new(TuiState::new()));

        Ok(Self { terminal, state })
    }

    pub fn new_forced() -> Result<Self> {
        // Skip terminal checks and try to force initialization
        enable_raw_mode().map_err(|e| anyhow!("Failed to enable raw mode: {}", e))?;

        let mut stdout = io::stdout();

        // Try to setup terminal
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .map_err(|e| {
                let _ = disable_raw_mode();
                anyhow!("Failed to setup terminal: {}", e)
            })?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)
            .map_err(|e| {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
                anyhow!("Failed to create terminal: {}", e)
            })?;

        let state = Arc::new(Mutex::new(TuiState::new()));

        Ok(Self { terminal, state })
    }

    pub fn get_callback(&self) -> EventCallback {
        let state = Arc::clone(&self.state);
        Box::new(move |event| {
            if let Ok(mut state) = state.lock() {
                state.handle_event(event);
            }
        })
    }

    pub fn run(&mut self) -> Result<()> {
        let tick_rate = Duration::from_millis(250);
        let mut last_tick = Instant::now();

        loop {
            let state_clone = Arc::clone(&self.state);
            self.terminal.draw(|f| {
                let state = state_clone.lock().unwrap();
                Self::ui_static(&state, f);
            })?;

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if crossterm::event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            if let Ok(mut state) = self.state.lock() {
                                state.should_quit = true;
                            }
                            break;
                        }
                        _ => {}
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                last_tick = Instant::now();
            }

            if let Ok(state) = self.state.lock() {
                if state.should_quit {
                    break;
                }
            }
        }

        Ok(())
    }

    fn ui_static(state: &TuiState, f: &mut Frame) {

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),          // Title
                Constraint::Length(6),          // Phases
                Constraint::Min(10),            // Chunk tracking (more space)
                Constraint::Length(4),          // Messages (compact)
                Constraint::Length(3),          // Status bar
            ])
            .split(f.size());

        let title = Paragraph::new("QR Video Files Processor")
            .block(Block::default().borders(Borders::ALL).title("Status"))
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(title, chunks[0]);

        let phases: Vec<ListItem> = state
            .phases
            .iter()
            .enumerate()
            .map(|(i, phase)| {
                let status_char = match phase.status {
                    PhaseStatus::Pending => "⏸",
                    PhaseStatus::InProgress => "⏳",
                    PhaseStatus::Completed => "✅",
                    PhaseStatus::Error => "❌",
                };

                let style = match phase.status {
                    PhaseStatus::Pending => Style::default().fg(Color::Gray),
                    PhaseStatus::InProgress => Style::default().fg(Color::Yellow),
                    PhaseStatus::Completed => Style::default().fg(Color::Green),
                    PhaseStatus::Error => Style::default().fg(Color::Red),
                };

                let progress_bar = if phase.status == PhaseStatus::InProgress || phase.status == PhaseStatus::Completed {
                    format!(" [{:5.1}%]", phase.progress)
                } else {
                    "".to_string()
                };

                let duration_info = if let Some(duration) = phase.duration_ms {
                    format!(" ({}ms)", duration)
                } else {
                    "".to_string()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", status_char), style),
                    Span::styled(format!("Phase {}: {}", i + 1, phase.name), style),
                    Span::styled(progress_bar, style),
                    Span::styled(duration_info, Style::default().fg(Color::Cyan)),
                ]))
            })
            .collect();

        let phases_list = List::new(phases)
            .block(Block::default().borders(Borders::ALL).title("Processing Phases"));
        f.render_widget(phases_list, chunks[1]);

        // Render chunk tracking section
        let chunk_items: Vec<ListItem> = state
            .chunks
            .iter()
            .map(|chunk| {
                let status_char = match chunk.status {
                    ChunkStatus::Pending => "⏸",
                    ChunkStatus::Processing => "⏳",
                    ChunkStatus::Completed => "✅",
                    ChunkStatus::Error => "❌",
                };

                let style = match chunk.status {
                    ChunkStatus::Pending => Style::default().fg(Color::Gray),
                    ChunkStatus::Processing => Style::default().fg(Color::Yellow),
                    ChunkStatus::Completed => Style::default().fg(Color::Green),
                    ChunkStatus::Error => Style::default().fg(Color::Red),
                };

                // Compact duration display (convert ms to minutes:seconds)
                let duration_info = chunk.duration_ms
                    .map(|d| {
                        let secs = d / 1000;
                        format!(" ({:02}:{:02})", secs / 60, secs % 60)
                    })
                    .unwrap_or_default();

                // Compact display: no redundant JSONL filename
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} Chunk {}: {} QR codes{}",
                                       status_char, chunk.id + 1, chunk.qr_codes_found, duration_info), style),
                ]))
            })
            .collect();

        let chunks_list = List::new(chunk_items)
            .block(Block::default().borders(Borders::ALL).title("Chunk Processing"));
        f.render_widget(chunks_list, chunks[2]);

        let messages: Vec<ListItem> = state
            .messages
            .iter()
            .rev()
            .take(3)
            .map(|m| ListItem::new(m.as_str()))
            .collect();

        let messages_list = List::new(messages)
            .block(Block::default().borders(Borders::ALL).title("Recent Messages"));
        f.render_widget(messages_list, chunks[3]);

        // Status bar with controls and frame progress
        let total_qr_codes: usize = state.chunks.iter().map(|c| c.qr_codes_found).sum();
        let completed_chunks = state.chunks.iter().filter(|c| c.status == ChunkStatus::Completed).count();
        let total_chunks = state.chunks.len();

        let status_text = if let Some(start_time) = state.start_time {
            let elapsed = start_time.elapsed();
            let elapsed_str = format!("{:02}:{:02}", elapsed.as_secs() / 60, elapsed.as_secs() % 60);

            if state.total_frames > 0 && state.frames_processed > 0 {
                let frame_progress = (state.frames_processed as f64 / state.total_frames as f64 * 100.0).min(100.0);
                let remaining_frames = state.total_frames.saturating_sub(state.frames_processed);
                let frames_per_sec = state.frames_processed as f64 / elapsed.as_secs_f64();
                let remaining_secs = if frames_per_sec > 0.0 {
                    (remaining_frames as f64 / frames_per_sec) as u64
                } else {
                    0
                };
                let remaining_str = format!("{:02}:{:02}", remaining_secs / 60, remaining_secs % 60);

                format!("Frames: {}/{} ({:.1}%) | Chunks: {}/{} | QR: {} | Time: {}/-{} | 'q' to quit",
                       state.frames_processed, state.total_frames, frame_progress,
                       completed_chunks, total_chunks, total_qr_codes, elapsed_str, remaining_str)
            } else if total_chunks > 0 {
                format!("Chunks: {}/{} | QR: {} | Time: {} | 'q' to quit",
                       completed_chunks, total_chunks, total_qr_codes, elapsed_str)
            } else {
                format!("Time: {} | 'q' to quit", elapsed_str)
            }
        } else {
            "Press 'q' or 'Esc' to quit | Processing will begin shortly...".to_string()
        };

        let status_bar = Paragraph::new(status_text)
            .block(Block::default().borders(Borders::ALL).title("Status & Controls"))
            .style(Style::default().fg(Color::Green));
        f.render_widget(status_bar, chunks[4]);
    }
}

impl Drop for TuiManager {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}