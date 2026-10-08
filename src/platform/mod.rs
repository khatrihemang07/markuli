//! Thin per-OS layer. Each OS module exposes the same function set:
//!
//! - `window_attributes` tunes the Overlay window before it is created.
//! - `monitor_under_cursor` picks the display for Draw Mode.
//! - `config_dir`, `set_launch_at_login` and `SettingsWindow` serve Settings.
//! - `open_editor` runs the Palette editor for a `View::edit` request until
//!   it closes, sending edits to the core as they happen. On macOS the editor
//!   is not modal: it returns at once and later edits arrive as `editors`
//!   events. `close_editor` ends one still open.
//! - `Presenter` owns the pixel buffer and puts it on screen with per-pixel
//!   alpha (softbuffer cannot, see the PR notes).
//!
//! `cfg(target_os)` appears only here.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_editors;
#[cfg(target_os = "macos")]
mod macos_settings;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(target_os = "macos")]
pub use macos_editors::{close_editor, open_editor};
#[cfg(target_os = "macos")]
pub use macos_settings::SettingsWindow;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
mod windows_editors;
#[cfg(target_os = "windows")]
mod windows_recorder;
#[cfg(target_os = "windows")]
mod windows_settings;
#[cfg(target_os = "windows")]
use windows as imp;
#[cfg(target_os = "windows")]
pub use windows_editors::open_editor;
/// The Windows editor is modal: nothing is left open to close.
#[cfg(target_os = "windows")]
pub fn close_editor(_: &mut dyn FnMut(markuli_core::Event)) {}
#[cfg(target_os = "windows")]
pub use windows_settings::SettingsWindow;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("Markuli supports macOS and Windows only");

pub use imp::{
    command_held, config_dir, configure_event_loop, frontmost_other, insets, monitor_under_cursor,
    pen_pressure, release_memory, secondary_click_modifier, set_clipboard_text,
    set_launch_at_login, tray_icon, window_attributes, Presenter, Previous, Shape,
};

/// A hotkey as the Settings window shows it.
pub fn label(text: &str) -> String {
    crate::hotkeys::label(text, imp::LOGO_KEY_NAME)
}

/// Channel order of the buffer `Presenter::buffer` hands to the core.
pub const FORMAT: markuli_core::Format = imp::FORMAT;
