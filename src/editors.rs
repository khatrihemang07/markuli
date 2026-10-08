//! What the native Palette editors tell the app. The editors themselves are
//! per OS in `platform` (`macos_editors`, `windows_editors`).

use std::sync::OnceLock;

/// Sent by an open editor; handled on the event loop. Only the macOS editors
/// report this way (they are not modal and call back later); the Windows
/// editors are modal and send their edits to the core inline.
#[allow(dead_code, reason = "constructed only by the macOS editors")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EditorEvent {
    Color {
        slot: usize,
        rgb: [u8; 3],
    },
    Width {
        slot: usize,
        width: f32,
    },
    /// The editor closed on its own (the user dismissed it). `generation` says
    /// which opened editor: a notice from one that a newer editor replaced is
    /// stale and must be ignored.
    Closed {
        generation: u64,
    },
}

type Sink = Box<dyn Fn(EditorEvent) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Routes editor events to the event loop (call once, at startup).
pub fn set_sink(sink: impl Fn(EditorEvent) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(sink));
}

/// Called by the macOS editors from their callbacks.
#[allow(dead_code, reason = "called only by the macOS editors")]
pub fn emit(event: EditorEvent) {
    if let Some(sink) = SINK.get() {
        sink(event);
    }
}
