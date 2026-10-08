//! What the native Palette editors tell the app. The editors themselves are
//! per OS in `platform`.

use std::sync::OnceLock;

/// Sent by an open editor; handled on the event loop.
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
    /// The editor closed on its own (the user dismissed it).
    Closed,
}

type Sink = Box<dyn Fn(EditorEvent) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Routes editor events to the event loop (call once, at startup).
pub fn set_sink(sink: impl Fn(EditorEvent) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(sink));
}

/// Called by the platform editors from their callbacks.
// Unused on Windows, whose editors do not exist yet.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn emit(event: EditorEvent) {
    if let Some(sink) = SINK.get() {
        sink(event);
    }
}
