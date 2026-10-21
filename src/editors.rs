//! What the system color panel tells the app. The Color Picker is drawn by the
//! core (ADR-0004); the panel behind its "..." is per OS in `platform`
//! (`macos_editors`, `windows_editors`).

use std::sync::OnceLock;

/// Sent by the open panel; handled on the event loop. Only the macOS panel
/// reports this way (it is not modal and calls back later); the Windows dialog
/// is modal and sends its color to the core inline.
#[allow(dead_code, reason = "constructed only by the macOS panel")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditorEvent {
    Color {
        slot: usize,
        rgb: [u8; 3],
    },
    /// The panel closed on its own (the user dismissed it). `generation` says
    /// which opened panel: a notice from one that a newer panel replaced is
    /// stale and must be ignored.
    Closed {
        generation: u64,
    },
}

type Sink = Box<dyn Fn(EditorEvent) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Routes panel events to the event loop (call once, at startup).
pub fn set_sink(sink: impl Fn(EditorEvent) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(sink));
}

/// Called by the macOS panel from its callbacks.
#[allow(dead_code, reason = "called only by the macOS panel")]
pub fn emit(event: EditorEvent) {
    if let Some(sink) = SINK.get() {
        sink(event);
    }
}
