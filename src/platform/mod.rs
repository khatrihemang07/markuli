//! Thin per-OS layer. Each OS module exposes the same function set:
//!
//! - `window_attributes` tunes the Overlay window before it is created.
//! - `monitor_under_cursor` picks the display for Draw Mode.
//! - `Presenter` owns the pixel buffer and puts it on screen with per-pixel
//!   alpha (softbuffer cannot, see the PR notes), and toggles click-through.
//!
//! `cfg(target_os)` appears only here.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
compile_error!("Markuli supports macOS and Windows only");

pub use imp::{
    command_held, configure_event_loop, monitor_under_cursor, release_memory, tray_icon,
    window_attributes, Presenter,
};

/// Channel order of the buffer `Presenter::buffer` hands to the core.
pub const FORMAT: markuli_core::Format = imp::FORMAT;
