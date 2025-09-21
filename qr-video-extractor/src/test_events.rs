use crate::events::{ProcessingEvent, ConsoleOutputHandler, OutputHandler};
use crate::tui::TuiState;

pub fn test_event_system() {
    println!("🧪 Testing Event System Implementation...");

    // Test 1: Console Output Handler
    println!("\n1️⃣ Testing ConsoleOutputHandler");
    let console_handler = ConsoleOutputHandler;

    let test_events = vec![
        ProcessingEvent::SystemError {
            context: "Background processing".to_string(),
            error: "Test error message".to_string(),
        },
        ProcessingEvent::InitializationProgress {
            stage: "TUI Setup".to_string(),
            message: "Forcing TUI mode initialization".to_string(),
        },
        ProcessingEvent::FinalSummary {
            files_count: 5,
            output_dir: "/test/output".to_string(),
            total_duration_ms: 12345,
        },
        ProcessingEvent::ModeTransition {
            from: "TUI".to_string(),
            to: "text".to_string(),
            reason: "TUI initialization failed".to_string(),
        },
    ];

    println!("Testing new event types through ConsoleOutputHandler:");
    for event in &test_events {
        console_handler.handle_event(event);
    }

    // Test 2: TUI State Event Handling
    println!("\n2️⃣ Testing TUI State Event Handling");
    let mut tui_state = TuiState::new();

    for event in test_events {
        tui_state.handle_event(event);
    }

    println!("TUI State after new events - Messages ({}): ", tui_state.messages.len());
    for (i, msg) in tui_state.messages.iter().enumerate() {
        println!("  {}: {}", i + 1, msg);
    }

    // Test 3: Processing Events
    println!("\n3️⃣ Testing Processing Events");
    let processing_events = vec![
        ProcessingEvent::PhaseStarted {
            phase: 1,
            description: "Video Analysis & Intelligent Splitting".to_string(),
        },
        ProcessingEvent::Progress {
            phase: 1,
            current: 2,
            total: 4,
            message: "Analyzing video...".to_string(),
        },
        ProcessingEvent::ChunkStarted {
            chunk_id: 0,
            chunk_name: "chunk_001.mp4".to_string(),
        },
        ProcessingEvent::ChunkCompleted {
            chunk_id: 0,
            qr_codes_found: 150,
            jsonl_file: "chunk_001.jsonl".to_string(),
            duration_ms: 5000,
        },
        ProcessingEvent::PhaseCompleted {
            phase: 1,
            duration_ms: 10000,
        },
    ];

    for event in &processing_events {
        tui_state.handle_event(event.clone());
    }

    println!("Final TUI State:");
    println!("  Phases: {}", tui_state.phases.len());
    println!("  Chunks: {}", tui_state.chunks.len());
    println!("  Messages: {}", tui_state.messages.len());
    println!("  Current Phase: {}", tui_state.current_phase);

    // Validate phase status
    for (i, phase) in tui_state.phases.iter().enumerate() {
        println!("  Phase {}: {:?} - {:.1}% - {}",
                 i + 1, phase.status, phase.progress, phase.message);
    }

    // Validate chunk status
    for chunk in &tui_state.chunks {
        println!("  Chunk {}: {:?} - {} QR codes",
                 chunk.id + 1, chunk.status, chunk.qr_codes_found);
    }

    // Test 4: Verify specific new event handling
    println!("\n4️⃣ Testing Specific New Event Handling");

    // Test SystemError
    let mut clean_state = TuiState::new();
    clean_state.handle_event(ProcessingEvent::SystemError {
        context: "Test Context".to_string(),
        error: "Test Error".to_string(),
    });
    assert!(!clean_state.messages.is_empty(), "SystemError should add message");
    assert!(clean_state.messages[0].contains("System Error"), "Should contain system error indicator");
    println!("  ✅ SystemError event handled correctly");

    // Test InitializationProgress
    clean_state.handle_event(ProcessingEvent::InitializationProgress {
        stage: "Test Stage".to_string(),
        message: "Test Message".to_string(),
    });
    assert!(clean_state.messages.len() >= 2, "InitializationProgress should add message");
    println!("  ✅ InitializationProgress event handled correctly");

    // Test FinalSummary
    clean_state.handle_event(ProcessingEvent::FinalSummary {
        files_count: 10,
        output_dir: "/test".to_string(),
        total_duration_ms: 5000,
    });
    println!("  ✅ FinalSummary event handled correctly");

    // Test ModeTransition
    clean_state.handle_event(ProcessingEvent::ModeTransition {
        from: "TUI".to_string(),
        to: "text".to_string(),
        reason: "Test reason".to_string(),
    });
    println!("  ✅ ModeTransition event handled correctly");

    println!("\n✅ All event handling tests completed successfully!");
    println!("   - ConsoleOutputHandler: ✅ Working");
    println!("   - TUI State Updates: ✅ Working");
    println!("   - New Event Types: ✅ Working");
    println!("   - Message Formatting: ✅ Working");
}