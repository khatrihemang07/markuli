//! The hotkey recorder of the Windows Settings window. After a recorder
//! button is pressed, keyboard focus moves to the window so its `WM_KEYDOWN`
//! sees the combo.

use super::windows::wide;
use crate::hotkeys::{self, Binding, Mods};
use crate::settings::{emit, SettingsEvent};
use core::ptr::null_mut;
use std::cell::{Cell, RefCell};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;

/// What the window needs to record a combo; owned by the window's `State`.
pub struct Recorder {
    recording: Cell<Option<Binding>>,
    /// Combo text per hotkey (Toggle, Clear), restored if recording is cancelled.
    shown: RefCell<[String; 2]>,
    pub toggle: Cell<HWND>,
    pub clear: Cell<HWND>,
    pub message: Cell<HWND>,
}

impl Recorder {
    pub fn new(toggle: &str, clear: &str) -> Self {
        Self {
            recording: Cell::new(None),
            shown: RefCell::new([toggle.to_owned(), clear.to_owned()]),
            toggle: Cell::new(null_mut()),
            clear: Cell::new(null_mut()),
            message: Cell::new(null_mut()),
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording.get().is_some()
    }

    fn button(&self, binding: Binding) -> HWND {
        match binding {
            Binding::Toggle => self.toggle.get(),
            Binding::Clear => self.clear.get(),
        }
    }

    fn show_binding(&self, binding: Binding) {
        let title = wide(&super::label(&self.shown.borrow()[binding as usize]));
        // SAFETY: the control belongs to a live window of this thread.
        unsafe { SetWindowTextW(self.button(binding), title.as_ptr()) };
    }

    /// Shows `text` as the combo for `binding` (the app changed it).
    pub fn set_hotkey(&self, binding: Binding, text: &str) {
        text.clone_into(&mut self.shown.borrow_mut()[binding as usize]);
        self.show_binding(binding);
    }

    pub fn set_message(&self, text: &str) {
        let text = wide(text);
        // SAFETY: as above.
        unsafe { SetWindowTextW(self.message.get(), text.as_ptr()) };
    }

    pub fn key_down(&self, vk: u16) {
        let Some(binding) = self.recording.get() else {
            return;
        };
        if [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN].contains(&vk) {
            return; // wait for the real key
        }
        if vk == VK_ESCAPE {
            self.recording.set(None);
            self.show_binding(binding);
            self.set_message("");
            emit(SettingsEvent::Listening(false));
            return;
        }
        let mods = Mods {
            ctrl: modifier_down(VK_CONTROL),
            alt: modifier_down(VK_MENU),
            shift: modifier_down(VK_SHIFT),
            logo: modifier_down(VK_LWIN) || modifier_down(VK_RWIN),
        };
        match hotkeys::key_from_vk(vk).and_then(|key| hotkeys::recorded(mods, key)) {
            Some(text) => {
                self.recording.set(None);
                self.show_binding(binding);
                emit(SettingsEvent::Record(binding, text));
                emit(SettingsEvent::Listening(false));
            }
            None => self.set_message("Hold Ctrl, Alt, Shift or Win with a key (Esc cancels)."),
        }
    }

    pub fn start(&self, hwnd: HWND, binding: Binding) {
        if let Some(previous) = self.recording.replace(Some(binding)) {
            self.show_binding(previous);
        }
        // A combo another app owns never reaches this window (the OS hands it to
        // that app), so say what silence means.
        self.set_message("No reaction? Another app owns it. Esc cancels.");
        let title = wide("Press the new shortcut...");
        // SAFETY: live controls; moving focus off the button lets the window get keys.
        unsafe {
            SetWindowTextW(self.button(binding), title.as_ptr());
            SetFocus(hwnd);
        }
        emit(SettingsEvent::Listening(true));
    }
}

fn modifier_down(vk: u16) -> bool {
    // SAFETY: plain query.
    unsafe { GetKeyState(i32::from(vk)) < 0 }
}
