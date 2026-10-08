//! Windows Settings window: a plain Win32 window with `BUTTON` and `STATIC`
//! controls, created on demand and destroyed on close. The hotkey recorder
//! lives in `windows_recorder`.
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
use super::windows_recorder::Recorder;
use crate::hotkeys::Binding;
use crate::settings::{emit, SettingsEvent};
use core::ffi::c_void;
use core::ptr::{null, null_mut};
use markuli_core::{Config, ToolbarPosition};
use std::cell::Cell;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{GetStockObject, COLOR_BTNFACE, DEFAULT_GUI_FONT, HBRUSH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics,
    GetWindowLongPtrW, IsWindow, LoadCursorW, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, ShowWindow, BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_AUTOCHECKBOX,
    BS_AUTORADIOBUTTON, BS_PUSHBUTTON, CREATESTRUCTW, GWLP_USERDATA, IDC_ARROW, SM_CXSCREEN,
    SM_CYSCREEN, SW_SHOW, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN, WM_NCCREATE, WM_NCDESTROY,
    WM_SETFONT, WM_SYSKEYDOWN, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_GROUP, WS_OVERLAPPED,
    WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

const CLASS: &str = "MarkuliSettings";
const ID_TOGGLE: usize = 101;
const ID_CLEAR: usize = 102;
const ID_LOGIN: usize = 103;
/// The Toolbar radio buttons, in order; a button's id is `ID_POSITION + index`.
const ID_POSITION: usize = 104;
const ID_LAST_POSITION: usize = ID_POSITION + 3;
const POSITIONS: [(&str, ToolbarPosition); 4] = [
    ("Top", ToolbarPosition::Top),
    ("Bottom", ToolbarPosition::Bottom),
    ("Left", ToolbarPosition::Left),
    ("Right", ToolbarPosition::Right),
];
/// `BM_GETCHECK` results (defined in the controls header; not worth a feature flag).
const BST_UNCHECKED: u32 = 0;
const BST_CHECKED: u32 = 1;
const CLIENT: (i32, i32) = (400, 230);

/// Per-window state, owned by the window (freed in `WM_NCDESTROY`).
struct State {
    recorder: Recorder,
    login: Cell<HWND>,
    initial: Config,
}

/// The state of a live Settings window, or `None` while it is being created
/// or destroyed.
fn state<'a>(hwnd: HWND) -> Option<&'a State> {
    // SAFETY: the pointer was stored by `WM_NCCREATE` from a leaked `Box` and
    // is cleared in `WM_NCDESTROY`; everything runs on the one UI thread.
    unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref() }
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

/// The "Toolbar" row: a label and four radio buttons, `current` one on.
/// `WS_GROUP` on the first makes the four exclude each other.
fn add_toolbar_row(hwnd: HWND, current: ToolbarPosition) {
    control(hwnd, "STATIC", "Toolbar", 0, 0, [20, 144, 80, 20]);
    for (index, (title, position)) in POSITIONS.into_iter().enumerate() {
        let group = if index == 0 { WS_GROUP } else { 0 };
        let radio = control(
            hwnd,
            "BUTTON",
            title,
            BS_AUTORADIOBUTTON as u32 | WS_TABSTOP | group,
            ID_POSITION + index,
            [110 + 66 * index as i32, 140, 64, 24],
        );
        let check = if position == current {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        };
        // SAFETY: `radio` was just created.
        unsafe { SendMessageW(radio, BM_SETCHECK, check as WPARAM, 0) };
    }
}

fn create_controls(hwnd: HWND, state: &State) {
    let config = &state.initial;
    control(hwnd, "STATIC", "Toggle Draw Mode", 0, 0, [20, 24, 160, 20]);
    state.recorder.toggle.set(control(
        hwnd,
        "BUTTON",
        &super::label(&config.toggle),
        BS_PUSHBUTTON as u32 | WS_TABSTOP,
        ID_TOGGLE,
        [190, 20, 190, 28],
    ));
    control(hwnd, "STATIC", "Clear", 0, 0, [20, 64, 160, 20]);
    state.recorder.clear.set(control(
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
    add_toolbar_row(hwnd, config.toolbar);
    // WS_GROUP ends the radio group before it.
    state
        .recorder
        .message
        .set(control(hwnd, "STATIC", "", WS_GROUP, 0, [20, 180, 360, 40]));
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
                    ID_TOGGLE => state.recorder.start(hwnd, Binding::Toggle),
                    ID_CLEAR => state.recorder.start(hwnd, Binding::Clear),
                    ID_LOGIN => {
                        // SAFETY: live checkbox.
                        let checked = unsafe { SendMessageW(state.login.get(), BM_GETCHECK, 0, 0) }
                            == BST_CHECKED as LRESULT;
                        emit(SettingsEvent::LaunchAtLogin(checked));
                    }
                    ID_POSITION..=ID_LAST_POSITION => {
                        emit(SettingsEvent::Toolbar(POSITIONS[id - ID_POSITION].1));
                    }
                    _ => {}
                }
            }
            return 0;
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if let Some(state) = state(hwnd) {
                if state.recorder.is_recording() {
                    state.recorder.key_down(wparam as u16);
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
            recorder: Recorder::new(&config.toggle, &config.clear),
            login: Cell::new(null_mut()),
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
            state.recorder.set_hotkey(binding, text);
        }
    }

    pub fn set_message(&self, text: &str) {
        if let Some(state) = state(self.hwnd) {
            state.recorder.set_message(text);
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
