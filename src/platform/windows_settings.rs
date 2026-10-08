//! Windows Settings window: a plain Win32 window with `BUTTON` and `STATIC`
//! controls, created on demand and destroyed on close. The hotkey recorder is
//! the window itself: after a recorder button is pressed, keyboard focus moves
//! to the window so its `WM_KEYDOWN` sees the combo.
//!
//! Compile-checked on CI; not yet run on real hardware.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::borrow_as_ptr,
    reason = "Win32 FFI: struct fields and flags have fixed C integer types, sizes are bounded by a screen"
)]

use super::windows::wide;
use crate::hotkeys::{self, Binding, Mods};
use crate::settings::{emit, SettingsEvent};
use core::ffi::c_void;
use core::ptr::{null, null_mut};
use markuli_core::Config;
use std::cell::{Cell, RefCell};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{GetStockObject, COLOR_BTNFACE, DEFAULT_GUI_FONT, HBRUSH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SetFocus, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics,
    GetWindowLongPtrW, IsWindow, LoadCursorW, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowTextW, ShowWindow, BM_GETCHECK, BM_SETCHECK, BN_CLICKED,
    BS_AUTOCHECKBOX, BS_PUSHBUTTON, CREATESTRUCTW, GWLP_USERDATA, IDC_ARROW, SM_CXSCREEN,
    SM_CYSCREEN, SW_SHOW, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN, WM_NCCREATE, WM_NCDESTROY,
    WM_SETFONT, WM_SYSKEYDOWN, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE,
};

const CLASS: &str = "MarkuliSettings";
const ID_TOGGLE: usize = 101;
const ID_CLEAR: usize = 102;
const ID_LOGIN: usize = 103;
/// `BM_GETCHECK` results (defined in the controls header; not worth a feature flag).
const BST_UNCHECKED: u32 = 0;
const BST_CHECKED: u32 = 1;
const CLIENT: (i32, i32) = (400, 190);

/// Per-window state, owned by the window (freed in `WM_NCDESTROY`).
struct State {
    recording: Cell<Option<Binding>>,
    /// Combo text per hotkey (Toggle, Clear), restored if recording is cancelled.
    shown: RefCell<[String; 2]>,
    toggle: Cell<HWND>,
    clear: Cell<HWND>,
    login: Cell<HWND>,
    message: Cell<HWND>,
    initial: Config,
}

impl State {
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

    fn set_message(&self, text: &str) {
        let text = wide(text);
        // SAFETY: as above.
        unsafe { SetWindowTextW(self.message.get(), text.as_ptr()) };
    }
}

/// The state of a live Settings window, or `None` while it is being created
/// or destroyed.
fn state<'a>(hwnd: HWND) -> Option<&'a State> {
    // SAFETY: the pointer was stored by `WM_NCCREATE` from a leaked `Box` and
    // is cleared in `WM_NCDESTROY`; everything runs on the one UI thread.
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref() }
}

fn modifier_down(vk: u16) -> bool {
    // SAFETY: plain query.
    unsafe { GetKeyState(i32::from(vk)) < 0 }
}

fn key_down(hwnd: HWND, state: &State, vk: u16) {
    let Some(binding) = state.recording.get() else {
        return;
    };
    if [VK_SHIFT, VK_CONTROL, VK_MENU, VK_LWIN, VK_RWIN].contains(&vk) {
        return; // wait for the real key
    }
    if vk == VK_ESCAPE {
        state.recording.set(None);
        state.show_binding(binding);
        state.set_message("");
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
            state.recording.set(None);
            state.show_binding(binding);
            emit(SettingsEvent::Record(binding, text));
            emit(SettingsEvent::Listening(false));
        }
        None => state.set_message("Hold Ctrl, Alt, Shift or Win with a key (Esc cancels)."),
    }
    let _ = hwnd;
}

fn start_recording(hwnd: HWND, state: &State, binding: Binding) {
    if let Some(previous) = state.recording.replace(Some(binding)) {
        state.show_binding(previous);
    }
    // A combo another app owns never reaches this window (the OS hands it to
    // that app), so say what silence means.
    state.set_message("No reaction? Another app owns it. Esc cancels.");
    let title = wide("Press the new shortcut...");
    // SAFETY: live controls; moving focus off the button lets the window get keys.
    unsafe {
        SetWindowTextW(state.button(binding), title.as_ptr());
        SetFocus(hwnd);
    }
    emit(SettingsEvent::Listening(true));
}

fn control(parent: HWND, class: &str, text: &str, style: u32, id: usize, rect: [i32; 4]) -> HWND {
    let (class, text) = (wide(class), wide(text));
    // SAFETY: `parent` is the window being created on this thread.
    unsafe {
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            rect[0],
            rect[1],
            rect[2],
            rect[3],
            parent,
            id as *mut c_void,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(
            hwnd,
            WM_SETFONT,
            GetStockObject(DEFAULT_GUI_FONT) as WPARAM,
            1,
        );
        hwnd
    }
}

fn create_controls(hwnd: HWND, state: &State) {
    let config = &state.initial;
    control(hwnd, "STATIC", "Toggle Draw Mode", 0, 0, [20, 24, 160, 20]);
    state.toggle.set(control(
        hwnd,
        "BUTTON",
        &super::label(&config.toggle),
        BS_PUSHBUTTON as u32 | WS_TABSTOP,
        ID_TOGGLE,
        [190, 20, 190, 28],
    ));
    control(hwnd, "STATIC", "Clear", 0, 0, [20, 64, 160, 20]);
    state.clear.set(control(
        hwnd,
        "BUTTON",
        &super::label(&config.clear),
        BS_PUSHBUTTON as u32 | WS_TABSTOP,
        ID_CLEAR,
        [190, 60, 190, 28],
    ));
    let login = control(
        hwnd,
        "BUTTON",
        "Launch at login",
        BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
        ID_LOGIN,
        [20, 104, 360, 24],
    );
    let check = if config.launch_at_login {
        BST_CHECKED
    } else {
        BST_UNCHECKED
    };
    // SAFETY: `login` was just created.
    unsafe { SendMessageW(login, BM_SETCHECK, check as WPARAM, 0) };
    state.login.set(login);
    state
        .message
        .set(control(hwnd, "STATIC", "", 0, 0, [20, 140, 360, 40]));
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE => {
            // SAFETY: for this message `lparam` points to the CREATESTRUCTW.
            let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
            // SAFETY: stores the leaked state pointer passed to CreateWindowExW.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
        }
        WM_CREATE => {
            if let Some(state) = state(hwnd) {
                create_controls(hwnd, state);
            }
        }
        WM_COMMAND => {
            let (id, code) = (wparam & 0xFFFF, (wparam >> 16) as u32);
            let Some(state) = state(hwnd) else { return 0 };
            if code == BN_CLICKED {
                match id {
                    ID_TOGGLE => start_recording(hwnd, state, Binding::Toggle),
                    ID_CLEAR => start_recording(hwnd, state, Binding::Clear),
                    ID_LOGIN => {
                        // SAFETY: live checkbox.
                        let checked = unsafe { SendMessageW(state.login.get(), BM_GETCHECK, 0, 0) }
                            == BST_CHECKED as LRESULT;
                        emit(SettingsEvent::LaunchAtLogin(checked));
                    }
                    _ => {}
                }
            }
            return 0;
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if let Some(state) = state(hwnd) {
                if state.recording.get().is_some() {
                    key_down(hwnd, state, wparam as u16);
                    return 0; // swallow, so Alt+key does not open the system menu
                }
            }
        }
        WM_DESTROY => emit(SettingsEvent::Closed),
        WM_NCDESTROY => {
            // SAFETY: reclaims the state leaked in `open`; cleared first so
            // nothing can reach it again.
            unsafe {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                if !ptr.is_null() {
                    drop(Box::from_raw(ptr));
                }
            }
        }
        _ => {}
    }
    // SAFETY: default handling for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn register_class() {
    let name = wide(CLASS);
    // SAFETY: the class name outlives the call (the system copies it); a
    // second registration simply fails, which is fine.
    unsafe {
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(null()),
            hIcon: null_mut(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as usize as HBRUSH,
            lpszMenuName: null(),
            lpszClassName: name.as_ptr(),
        };
        RegisterClassW(&class);
    }
}

/// Handle to the open window. Dropping it destroys the window if still open.
pub struct SettingsWindow {
    hwnd: HWND,
}

impl SettingsWindow {
    pub fn open(config: &Config) -> Option<Self> {
        register_class();
        let state = Box::new(State {
            recording: Cell::new(None),
            shown: RefCell::new([config.toggle.clone(), config.clear.clone()]),
            toggle: Cell::new(null_mut()),
            clear: Cell::new(null_mut()),
            login: Cell::new(null_mut()),
            message: Cell::new(null_mut()),
            initial: config.clone(),
        });
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU;
        let (class, title) = (wide(CLASS), wide("Markuli Settings"));
        // SAFETY: plain Win32 calls on the UI thread; ownership of `state`
        // passes to the window and is reclaimed in `WM_NCDESTROY`.
        unsafe {
            let mut frame = RECT {
                left: 0,
                top: 0,
                right: CLIENT.0,
                bottom: CLIENT.1,
            };
            AdjustWindowRect(&mut frame, style, 0);
            let (width, height) = (frame.right - frame.left, frame.bottom - frame.top);
            let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
            let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;
            let raw = Box::into_raw(state);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                style,
                x,
                y,
                width,
                height,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                raw.cast::<c_void>(),
            );
            if hwnd.is_null() {
                drop(Box::from_raw(raw));
                return None;
            }
            let this = Self { hwnd };
            this.raise();
            Some(this)
        }
    }

    pub fn raise(&self) {
        // SAFETY: `hwnd` is live while this handle exists (checked in `Drop`).
        unsafe {
            ShowWindow(self.hwnd, SW_SHOW);
            SetForegroundWindow(self.hwnd);
        }
    }

    pub fn set_hotkey(&self, binding: Binding, text: &str) {
        if let Some(state) = state(self.hwnd) {
            text.clone_into(&mut state.shown.borrow_mut()[binding as usize]);
            state.show_binding(binding);
        }
    }

    pub fn set_message(&self, text: &str) {
        if let Some(state) = state(self.hwnd) {
            state.set_message(text);
        }
    }

    pub fn set_launch_at_login(&self, on: bool) {
        if let Some(state) = state(self.hwnd) {
            let check = if on { BST_CHECKED } else { BST_UNCHECKED };
            // SAFETY: live checkbox.
            unsafe { SendMessageW(state.login.get(), BM_SETCHECK, check as WPARAM, 0) };
        }
    }
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        // SAFETY: the window may already be gone (user closed it).
        unsafe {
            if IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
