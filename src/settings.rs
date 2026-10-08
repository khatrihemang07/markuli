//! What the native Settings window tells the app, and where the app's own
//! Settings file lives. The window code itself is per OS in `platform`.

use crate::hotkeys::Binding;
use markuli_core::{Config, ToolbarPosition};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Sent by the Settings window; handled on the event loop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsEvent {
    /// The user pressed a new combo for a hotkey (text like `alt+Backquote`).
    Record(Binding, String),
    LaunchAtLogin(bool),
    /// A radio button in Settings chose where the Toolbar sits.
    #[allow(dead_code, reason = "only the macOS window sends it; Windows follows")]
    Toolbar(ToolbarPosition),
    /// The recorder started (`true`) or stopped (`false`) listening for a
    /// combo; Markuli's own hotkeys are released meanwhile.
    Listening(bool),
    /// The window is gone; drop the handle so its memory is freed.
    Closed,
}

type Sink = Box<dyn Fn(SettingsEvent) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();

/// Routes window events to the event loop (call once, at startup).
pub fn set_sink(sink: impl Fn(SettingsEvent) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(sink));
}

/// Called by the platform windows from their callbacks.
pub fn emit(event: SettingsEvent) {
    if let Some(sink) = SINK.get() {
        sink(event);
    }
}

/// With the `dev-hooks` feature, `MARKULI_CONFIG_DIR` overrides the OS
/// directory (manual tests); release builds ignore it.
fn config_path() -> Option<PathBuf> {
    #[cfg(feature = "dev-hooks")]
    let over = std::env::var_os("MARKULI_CONFIG_DIR").map(PathBuf::from);
    #[cfg(not(feature = "dev-hooks"))]
    let over = None;
    let dir = over.or_else(crate::platform::config_dir)?;
    Some(dir.join("config"))
}

/// Settings from disk; defaults when the file is missing or unreadable.
pub fn load() -> Config {
    config_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| Config::parse(&text))
        .unwrap_or_default()
}

pub fn save(config: &Config) {
    let Some(path) = config_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(error) = std::fs::write(&path, config.to_text()) {
        eprintln!("markuli: could not save settings: {error}");
    }
}
