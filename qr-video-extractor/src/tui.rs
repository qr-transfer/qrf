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
        }
    }

    pub fn handle_event(&mut self, event: ProcessingEvent) {
        match event {
            ProcessingEvent::PhaseStarted { phase, description } => {
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
                Constraint::Min(8),             // Chunk tracking
                Constraint::Length(8),          // Messages
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

                let jsonl_info = chunk.jsonl_file.as_ref()
                    .map(|f| format!(" → {}", f))
                    .unwrap_or_default();

                let duration_info = chunk.duration_ms
                    .map(|d| format!(" ({}ms)", d))
                    .unwrap_or_default();

                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", status_char), style),
                    Span::styled(format!("Chunk {}: {} QR codes", chunk.id + 1, chunk.qr_codes_found), style),
                    Span::styled(jsonl_info, Style::default().fg(Color::Cyan)),
                    Span::styled(duration_info, Style::default().fg(Color::Gray)),
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
            .take(8)
            .map(|m| ListItem::new(m.as_str()))
            .collect();

        let messages_list = List::new(messages)
            .block(Block::default().borders(Borders::ALL).title("Recent Messages"));
        f.render_widget(messages_list, chunks[3]);
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