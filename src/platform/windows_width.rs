//! The Windows width editor: a small popup with a trackbar (0.25 pt steps), an
//! edit box and Reset. Modal: `popup` returns when it is closed. The
//! re-entrancy argument is in `windows_editors`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::borrow_as_ptr,
    reason = "Win32 FFI: struct fields and flags have fixed C integer types; trackbar positions are 1..=40"
)]

use super::windows::wide;
use super::windows_editors::{child, scaled, Out};
use core::ffi::c_void;
use core::mem::{size_of, zeroed};
use core::ptr::{null, null_mut};
use markuli_core::{Event, Palette, SlotKind, SlotValue};
use std::cell::{Cell, RefCell};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{COLOR_BTNFACE, HBRUSH};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, TBM_SETPAGESIZE, TBM_SETPOS,
    TBM_SETRANGE, TBS_AUTOTICKS, TRACKBAR_CLASSW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, GetWindowTextW, IsDialogMessageW, LoadCursorW, PostQuitMessage,
    RegisterClassW, SendMessageW, SetWindowLongPtrW, SetWindowTextW, TranslateMessage,
    BS_PUSHBUTTON, CREATESTRUCTW, EN_CHANGE, ES_AUTOHSCROLL, GWLP_USERDATA, IDC_ARROW, MSG,
    WA_INACTIVE, WM_ACTIVATE, WM_COMMAND, WM_DESTROY, WM_HSCROLL, WM_NCCREATE, WNDCLASSW,
    WS_BORDER, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const CLASS: &str = "MarkuliWidth";
const ID_OK: usize = 1; // Enter, via IsDialogMessage
const ID_CANCEL: usize = 2; // Esc, via IsDialogMessage
/// `WM_USER`; windows-sys has no constant for it.
const TBM_GETPOS: u32 = 0x0400;
const ID_EDIT: usize = 101;
const ID_RESET: usize = 102;
/// Popup client size at 96 dpi: trackbar on top, edit box and Reset below.
const SIZE: (i32, i32) = (230, 74);
/// Trackbar positions are quarter points: position 2 is width 0.5, 120 is 30.
const MIN_POS: i32 = 2;
const MAX_POS: i32 = 120;

/// The popup's outer size in pixels at `dpi`.
pub(super) fn size(dpi: u32) -> (i32, i32) {
    (scaled(SIZE.0, dpi), scaled(SIZE.1, dpi))
}

/// The trackbar position for a slot width (snapped and clamped first).
fn pos_from_width(width: f32) -> i32 {
    Palette::snap_width(width).map_or(MIN_POS, |w| {
        ((w * 4.0).round() as i32).clamp(MIN_POS, MAX_POS)
    })
}

/// The slot width a trackbar position stands for.
fn width_from_pos(pos: i32) -> f32 {
    pos.clamp(MIN_POS, MAX_POS) as f32 / 4.0
}

/// What the popup's window procedure reaches through `GWLP_USERDATA`. It
/// lives on the stack of `popup`, which outlives the window.
struct State<'a, 'b> {
    slot: usize,
    out: RefCell<&'a mut Out<'b>>,
    track: Cell<HWND>,
    edit: Cell<HWND>,
    /// Set while the code, not the user, changes the edit box.
    syncing: Cell<bool>,
    closed: Cell<bool>,
}

impl State<'_, '_> {
    /// Shows `width` in the trackbar, and in the edit box unless the user is
    /// typing there (`from_edit`), then reports it.
    fn apply(&self, width: f32, from_edit: bool) {
        // SAFETY: both controls are children of the popup, which is live
        // while its window procedure runs.
        unsafe {
            SendMessageW(
                self.track.get(),
                TBM_SETPOS,
                1,
                pos_from_width(width) as LPARAM,
            );
            if !from_edit {
                self.syncing.set(true);
                SetWindowTextW(self.edit.get(), wide(&Palette::width_text(width)).as_ptr());
                self.syncing.set(false);
            }
        }
        self.out.borrow_mut().edit(Event::EditWidth {
            slot: self.slot,
            width,
        });
    }

    fn typed(&self) {
        if self.syncing.get() {
            return;
        }
        let mut text = [0_u16; 16];
        // SAFETY: the buffer holds the 16 units passed.
        let len = unsafe { GetWindowTextW(self.edit.get(), text.as_mut_ptr(), 16) };
        // Half-typed text ("", "2.") is not a width yet: leave it alone.
        let typed = String::from_utf16_lossy(&text[..len.max(0) as usize]);
        if let Some(width) = Palette::parse_width(&typed) {
            self.apply(width, true);
        }
    }

    fn reset(&self) {
        if let Some(SlotValue::Width(width)) = Palette::default_slot(SlotKind::Width, self.slot) {
            self.apply(width, false);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: for this message `lparam` points to the CREATESTRUCTW.
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        // SAFETY: stores the stack state pointer passed to CreateWindowExW.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
    }
    // SAFETY: set by `WM_NCCREATE` above to the `State` that outlives the
    // window; the loop in `popup` ends before it is dropped. The lifetimes
    // are erased: they only matter to `popup`, which owns the State.
    let state = unsafe {
        (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State<'static, 'static>).as_ref()
    };
    if let Some(state) = state {
        let (id, code) = (wparam & 0xFFFF, (wparam >> 16) as u32);
        match message {
            WM_HSCROLL => {
                // SAFETY: the trackbar is a live child of this popup.
                let pos = unsafe { SendMessageW(state.track.get(), TBM_GETPOS, 0, 0) };
                state.apply(width_from_pos(pos as i32), false);
            }
            WM_COMMAND if id == ID_EDIT && code == EN_CHANGE => state.typed(),
            WM_COMMAND if id == ID_RESET => state.reset(),
            // Enter or Esc (IsDialogMessage), or a click elsewhere (the popup
            // is no longer active), closes the popup like a menu.
            WM_COMMAND if id == ID_OK || id == ID_CANCEL => {
                // SAFETY: destroys this popup; WM_DESTROY ends the loop.
                unsafe { DestroyWindow(hwnd) };
            }
            WM_ACTIVATE if id as u32 == WA_INACTIVE => {
                // SAFETY: as above.
                unsafe { DestroyWindow(hwnd) };
            }
            WM_DESTROY => state.closed.set(true),
            _ => {}
        }
    }
    // SAFETY: default handling for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// Shows the popup at `at` and runs it until it closes.
///
/// # Safety
/// `out.owner` is the live Overlay window of this thread.
pub(super) unsafe fn popup(out: &mut Out, at: POINT, (slot, width): (usize, f32), dpi: u32) {
    let name = wide(CLASS);
    let state = State {
        slot,
        out: RefCell::new(out),
        track: Cell::new(null_mut()),
        edit: Cell::new(null_mut()),
        syncing: Cell::new(false),
        closed: Cell::new(false),
    };
    let (w, h) = size(dpi);
    // SAFETY: plain Win32 calls on the UI thread. `state` and `name` outlive
    // the window, and the loop below runs until the window is destroyed.
    unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        });
        // A repeated registration fails, which is fine.
        RegisterClassW(&WNDCLASSW {
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
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            name.as_ptr(),
            wide("").as_ptr(),
            WS_POPUP | WS_BORDER | WS_VISIBLE,
            at.x,
            at.y,
            w,
            h,
            state.out.borrow().owner,
            null_mut(),
            GetModuleHandleW(null()),
            (&raw const state).cast::<c_void>(),
        );
        if hwnd.is_null() {
            return;
        }
        add_controls(hwnd, &state, width, dpi);
        run_loop(hwnd, &state);
    }
}

/// The trackbar, edit box and Reset button.
///
/// # Safety
/// `hwnd` is the live popup of this thread.
unsafe fn add_controls(hwnd: HWND, state: &State, width: f32, dpi: u32) {
    let s = |v| scaled(v, dpi);
    // SAFETY: `hwnd` is the popup (caller) and the class names are nul
    // terminated and outlive the calls.
    unsafe {
        let track = child(
            hwnd,
            TRACKBAR_CLASSW,
            "",
            TBS_AUTOTICKS,
            0,
            [s(6), s(4), s(218), s(30)],
        );
        let edit_class = wide("EDIT");
        let edit = child(
            hwnd,
            edit_class.as_ptr(),
            &Palette::width_text(width),
            ES_AUTOHSCROLL as u32 | WS_BORDER,
            ID_EDIT,
            [s(10), s(42), s(60), s(22)],
        );
        let button_class = wide("BUTTON");
        child(
            hwnd,
            button_class.as_ptr(),
            "Reset",
            BS_PUSHBUTTON as u32,
            ID_RESET,
            [s(80), s(40), s(70), s(26)],
        );
        state.track.set(track);
        state.edit.set(edit);
        SendMessageW(track, TBM_SETRANGE, 1, (MIN_POS | MAX_POS << 16) as LPARAM);
        SendMessageW(track, TBM_SETPAGESIZE, 0, 4);
        SendMessageW(track, TBM_SETPOS, 1, pos_from_width(width) as LPARAM);
        SetFocus(track);
    }
}

/// Runs messages until the popup is destroyed. `IsDialogMessageW` gives Tab,
/// Enter and Esc to the controls.
///
/// # Safety
/// `hwnd` is the live popup of this thread.
unsafe fn run_loop(hwnd: HWND, state: &State) {
    // SAFETY: standard message loop on the thread that owns the window.
    unsafe {
        let mut msg: MSG = zeroed();
        while !state.closed.get() {
            match GetMessageW(&mut msg, null_mut(), 0, 0) {
                0 => {
                    // The app is quitting: pass WM_QUIT on, close the popup.
                    PostQuitMessage(msg.wParam as i32);
                    DestroyWindow(hwnd);
                    break;
                }
                -1 => break,
                _ => {}
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
