//! Windows Palette editors, opened for `View::edit`. Both run their own
//! modal loop and return when closed, so the caller needs no state:
//!
//! - Color: the system `ChooseColor` dialog. It has no Reset, so the slot's
//!   default is put in the first custom swatch; one click on it, then OK.
//!   That costs no code and no control of our own.
//! - Width: a small popup with a trackbar (0.5 steps), an edit box and Reset.
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
use crate::editor::{
    colorref, parse_width, pos_from_width, rgb_from_colorref, width_from_pos, width_text, MAX_POS,
    MIN_POS,
};
use core::ffi::c_void;
use core::mem::{size_of, zeroed};
use core::ptr::{null, null_mut};
use markuli_core::{EditRequest, Event, Palette, SlotValue};
use std::cell::{Cell, RefCell};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, GetStockObject, COLOR_BTNFACE, DEFAULT_GUI_FONT, HBRUSH,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW,
};
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, TBM_SETPAGESIZE, TBM_SETPOS,
    TBM_SETRANGE, TBS_AUTOTICKS, TRACKBAR_CLASSW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, GetWindowTextW, IsDialogMessageW, LoadCursorW, PostQuitMessage,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW,
    TranslateMessage, BS_PUSHBUTTON, CREATESTRUCTW, EN_CHANGE, ES_AUTOHSCROLL, GWLP_USERDATA,
    IDC_ARROW, MSG, WA_INACTIVE, WM_ACTIVATE, WM_COMMAND, WM_DESTROY, WM_HSCROLL, WM_NCCREATE,
    WM_SETFONT, WNDCLASSW, WS_BORDER, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    WS_TABSTOP, WS_VISIBLE,
};
use winit::raw_window_handle::RawWindowHandle;

const CLASS: &str = "MarkuliWidth";
const ID_OK: usize = 1; // Enter, via IsDialogMessage
const ID_CANCEL: usize = 2; // Esc, via IsDialogMessage
/// `WM_USER`; windows-sys has no constant for it.
const TBM_GETPOS: u32 = 0x0400;
const ID_EDIT: usize = 101;
const ID_RESET: usize = 102;
/// Popup client size: trackbar on top, edit box and Reset below.
const SIZE: (i32, i32) = (230, 74);

/// Opens the editor for `request` on top of `window` and returns when it is
/// closed. Edits go out through `send` as they happen; `EditEnd` comes last.
pub fn open_editor(request: EditRequest, owner: RawWindowHandle, send: &mut dyn FnMut(Event)) {
    let RawWindowHandle::Win32(handle) = owner else {
        return;
    };
    let owner = handle.hwnd.get() as HWND;
    // The anchor is in Overlay pixels; the editor sits just under the button.
    let mut at = POINT {
        x: request.anchor.x as i32,
        y: (request.anchor.y + request.anchor.h) as i32,
    };
    // SAFETY: `owner` is the live Overlay window of this thread.
    unsafe { ClientToScreen(owner, &mut at) };
    match request.value {
        SlotValue::Color(rgb) => choose_color(owner, request.index, rgb, send),
        SlotValue::Width(width) => width_popup(owner, at, request.index, width, send),
    }
    // SAFETY: the Overlay is live; the closed editor left it without focus.
    unsafe { SetForegroundWindow(owner) };
}

fn choose_color(owner: HWND, slot: usize, rgb: [u8; 3], send: &mut dyn FnMut(Event)) {
    let default = Palette::DEFAULT.colors.get(slot).copied().unwrap_or(rgb);
    let mut custom = [0x00FF_FFFF_u32; 16];
    custom[0] = colorref(default);
    // SAFETY: all-zero is a valid CHOOSECOLORW (null pointers, no flags); the
    // fields the dialog needs are set below, and `custom` outlives the call.
    let ok = unsafe {
        let mut choose: CHOOSECOLORW = zeroed();
        choose.lStructSize = size_of::<CHOOSECOLORW>() as u32;
        choose.hwndOwner = owner;
        choose.rgbResult = colorref(rgb);
        choose.lpCustColors = custom.as_mut_ptr();
        choose.Flags = CC_RGBINIT | CC_FULLOPEN;
        ChooseColorW(&mut choose) != 0 && {
            send(Event::EditColor {
                slot,
                rgb: rgb_from_colorref(choose.rgbResult),
            });
            true
        }
    };
    if ok {
        send(Event::EditEnd);
    }
}

/// What the popup's window procedure reaches through `GWLP_USERDATA`. It
/// lives on the stack of `width_popup`, which outlives the window.
struct State<'a> {
    slot: usize,
    send: RefCell<&'a mut dyn FnMut(Event)>,
    track: Cell<HWND>,
    edit: Cell<HWND>,
    /// Set while the code, not the user, changes the edit box.
    syncing: Cell<bool>,
    closed: Cell<bool>,
}

impl State<'_> {
    /// Shows `width` in the trackbar, and in the edit box unless the user is
    /// typing there (`from_edit`), then reports it.
    fn apply(&self, width: f32, from_edit: bool) {
        // SAFETY: both controls are children of the live popup.
        unsafe {
            SendMessageW(
                self.track.get(),
                TBM_SETPOS,
                1,
                pos_from_width(width) as LPARAM,
            );
            if !from_edit {
                self.syncing.set(true);
                SetWindowTextW(self.edit.get(), wide(&width_text(width)).as_ptr());
                self.syncing.set(false);
            }
        }
        (self.send.borrow_mut())(Event::EditWidth {
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
        if let Some(width) = parse_width(&String::from_utf16_lossy(&text[..len.max(0) as usize])) {
            self.apply(width, true);
        }
    }
}

fn child(
    parent: HWND,
    class: *const u16,
    text: &str,
    style: u32,
    id: usize,
    rect: [i32; 4],
) -> HWND {
    // SAFETY: `parent` is the popup being created on this thread.
    unsafe {
        let hwnd = CreateWindowExW(
            0,
            class,
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
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
    // window; the loop in `width_popup` ends before it is dropped.
    let state = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref() };
    if let Some(state) = state {
        let (id, code) = (wparam & 0xFFFF, (wparam >> 16) as u32);
        match message {
            WM_HSCROLL => {
                // SAFETY: live trackbar.
                let pos = unsafe { SendMessageW(state.track.get(), TBM_GETPOS, 0, 0) };
                state.apply(width_from_pos(pos as i32), false);
            }
            WM_COMMAND if id == ID_EDIT && code == EN_CHANGE => state.typed(),
            WM_COMMAND if id == ID_RESET => {
                let default = Palette::DEFAULT.widths.get(state.slot).copied();
                state.apply(default.unwrap_or(2.0), false);
            }
            WM_COMMAND if id == ID_OK || id == ID_CANCEL => {
                // SAFETY: closes this popup; WM_DESTROY ends the loop.
                unsafe { DestroyWindow(hwnd) };
            }
            // Clicking elsewhere closes the popup, like a menu.
            WM_ACTIVATE if (wparam & 0xFFFF) as u32 == WA_INACTIVE => {
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

fn width_popup(owner: HWND, at: POINT, slot: usize, width: f32, send: &mut dyn FnMut(Event)) {
    let name = wide(CLASS);
    let state = State {
        slot,
        send: RefCell::new(send),
        track: Cell::new(null_mut()),
        edit: Cell::new(null_mut()),
        syncing: Cell::new(false),
        closed: Cell::new(false),
    };
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
            SIZE.0,
            SIZE.1,
            owner,
            null_mut(),
            GetModuleHandleW(null()),
            (&raw const state).cast::<c_void>(),
        );
        if hwnd.is_null() {
            return;
        }
        let track = child(hwnd, TRACKBAR_CLASSW, "", TBS_AUTOTICKS, 0, [6, 4, 218, 30]);
        let edit = child(
            hwnd,
            wide("EDIT").as_ptr(),
            &width_text(width),
            ES_AUTOHSCROLL as u32 | WS_BORDER,
            ID_EDIT,
            [10, 42, 60, 22],
        );
        let reset = wide("BUTTON");
        child(
            hwnd,
            reset.as_ptr(),
            "Reset",
            BS_PUSHBUTTON as u32,
            ID_RESET,
            [80, 40, 70, 26],
        );
        state.track.set(track);
        state.edit.set(edit);
        SendMessageW(track, TBM_SETRANGE, 1, (MIN_POS | MAX_POS << 16) as LPARAM);
        SendMessageW(track, TBM_SETPAGESIZE, 0, 2);
        SendMessageW(track, TBM_SETPOS, 1, pos_from_width(width) as LPARAM);
        SetFocus(track);
        run_loop(hwnd, &state);
    }
    (state.send.borrow_mut())(Event::EditEnd);
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
