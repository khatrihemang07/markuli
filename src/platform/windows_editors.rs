//! Windows Palette editors, opened for `View::edit`. Both run their own
//! modal loop and return when closed, so the caller needs no state:
//!
//! - Color: the system `ChooseColor` dialog, with a "Reset" button of ours
//!   added under it by a hook (`windows_width` has the other editor).
//! - Width: a small popup with a trackbar (0.25 pt steps), an edit box and Reset.
//!
//! Re-entrancy: both editors pump messages (`ChooseColorW` and the popup's
//! loop) on the thread that is inside winit's event handler. That is safe
//! because winit's Windows backend buffers every event that arrives while a
//! handler is running (`EventLoopRunner::send_event`) and delivers it after
//! the handler returns; nothing re-enters `App`, whose only access during the
//! loop is the `send` closure this module was given. The consequence is that
//! the click that dismisses an editor reaches the core after `EditEnd`.
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
use super::windows_width;
use core::ffi::c_void;
use core::mem::{size_of, zeroed};
use core::ptr::null;
use markuli_core::{EditRequest, Event, Palette, Side, SlotKind, SlotValue};
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    ClientToScreen, GetMonitorInfoW, GetStockObject, MonitorFromWindow, DEFAULT_GUI_FONT,
    MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    ChooseColorW, CC_ENABLEHOOK, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW,
};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, EndDialog, GetClientRect, GetWindowRect, IsWindow, PostMessageW, SendMessageW,
    SetForegroundWindow, SetWindowPos, BS_PUSHBUTTON, SWP_NOMOVE, SWP_NOZORDER, WM_APP, WM_COMMAND,
    WM_INITDIALOG, WM_SETFONT, WS_CHILD, WS_TABSTOP, WS_VISIBLE,
};
use winit::raw_window_handle::RawWindowHandle;

const ID_RESET_COLOR: usize = 201;
/// Set by the Choose Color hook when its Reset button ended the dialog.
static RESET_COLOR: AtomicBool = AtomicBool::new(false);

/// Where the editors send what the user does. Edits are dropped once the
/// Overlay is gone (it can be destroyed while a modal loop runs), so the app
/// never hears of an edit for a window it no longer has.
pub(super) struct Out<'a> {
    pub(super) owner: HWND,
    send: &'a mut dyn FnMut(Event),
}

impl Out<'_> {
    pub(super) fn edit(&mut self, event: Event) {
        // SAFETY: IsWindow accepts any handle value, destroyed ones included.
        if unsafe { IsWindow(self.owner) } != 0 {
            (self.send)(event);
        }
    }
}

/// Opens the editor for `request` on top of the Overlay `owner` and returns
/// when it is closed. Edits go out as they happen; `EditEnd` comes last.
pub fn open_editor(request: EditRequest, owner: RawWindowHandle, send: &mut dyn FnMut(Event)) {
    let RawWindowHandle::Win32(handle) = owner else {
        return;
    };
    let owner = handle.hwnd.get() as HWND;
    let mut out = Out { owner, send };
    // SAFETY: IsWindow accepts any handle value; the rest runs only for a
    // live window, on the thread that owns it.
    unsafe {
        if IsWindow(owner) == 0 {
            return;
        }
        match request.value {
            SlotValue::Color(rgb) => choose_color(&mut out, request.index, rgb),
            SlotValue::Width(width) => {
                let dpi = GetDpiForWindow(owner).max(96);
                let size = windows_width::size(dpi);
                let at = place(owner, &request, size);
                windows_width::popup(&mut out, at, (request.index, width), dpi);
            }
        }
    }
    (out.send)(Event::EditEnd);
    // SAFETY: SetForegroundWindow only asks the system to switch; a window
    // destroyed meanwhile makes it fail harmlessly, but check anyway so the
    // Overlay gets the keyboard back only if it is still there.
    unsafe {
        if IsWindow(owner) != 0 {
            SetForegroundWindow(owner);
        }
    }
}

/// Nothing is left open between calls: the Windows editors are modal.
pub fn close_editor(_send: &mut dyn FnMut(Event)) {}

/// The Windows editors never report closing later.
pub fn editor_closed(_generation: u64, _send: &mut dyn FnMut(Event)) {}

/// The screen position of a `size` editor next to the button, on the side
/// the core chose, moved to lie inside the work area of the nearest monitor.
///
/// # Safety
/// `owner` is the live Overlay window.
unsafe fn place(owner: HWND, request: &EditRequest, (width, height): (i32, i32)) -> POINT {
    let anchor = request.anchor;
    let mut corner = POINT {
        x: anchor.x as i32,
        y: anchor.y as i32,
    };
    // SAFETY: `owner` is live (caller); ClientToScreen writes the POINT.
    unsafe { ClientToScreen(owner, &mut corner) };
    let (aw, ah) = (anchor.w as i32, anchor.h as i32);
    let (left, top) = match request.side {
        Side::Below => (corner.x, corner.y + ah),
        Side::Above => (corner.x, corner.y - height),
        Side::Right => (corner.x + aw, corner.y),
        Side::Left => (corner.x - width, corner.y),
    };
    // SAFETY: all-zero is a valid MONITORINFO once `cbSize` is set.
    let work = unsafe {
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        let monitor = MonitorFromWindow(owner, MONITOR_DEFAULTTONEAREST);
        (GetMonitorInfoW(monitor, &mut info) != 0).then_some(info.rcWork)
    };
    match work {
        // `max` last: a work area smaller than the editor keeps its top-left.
        Some(area) => POINT {
            x: left.min(area.right - width).max(area.left),
            y: top.min(area.bottom - height).max(area.top),
        },
        None => POINT { x: left, y: top },
    }
}

/// `value` (96 dpi pixels) at `dpi`.
pub(super) fn scaled(value: i32, dpi: u32) -> i32 {
    value * dpi as i32 / 96
}

/// A child control of `parent`, in the default GUI font.
///
/// # Safety
/// `parent` is a live window of this thread, and `class` is a nul-terminated
/// UTF-16 window class name that outlives the call.
pub(super) unsafe fn child(
    parent: HWND,
    class: *const u16,
    text: &str,
    style: u32,
    id: usize,
    rect: [i32; 4],
) -> HWND {
    // SAFETY: `parent` and `class` are valid (caller); the text buffer lives
    // until the call returns; the new window is ours, so WM_SETFONT is fine.
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

/// A Win32 `COLORREF` (0x00BBGGRR) for `rgb`.
fn colorref(rgb: [u8; 3]) -> u32 {
    u32::from(rgb[0]) | u32::from(rgb[1]) << 8 | u32::from(rgb[2]) << 16
}

/// The color a `COLORREF` holds; the high byte is ignored.
fn rgb_from_colorref(colorref: u32) -> [u8; 3] {
    [
        colorref as u8,
        (colorref >> 8) as u8,
        (colorref >> 16) as u8,
    ]
}

/// The Choose Color dialog, full-open, with Reset added by `color_hook`.
///
/// # Safety
/// `out.owner` is the live Overlay window of this thread.
unsafe fn choose_color(out: &mut Out, slot: usize, rgb: [u8; 3]) {
    let mut custom = [0x00FF_FFFF_u32; 16];
    RESET_COLOR.store(false, Ordering::Relaxed);
    // SAFETY: all-zero is a valid CHOOSECOLORW (null pointers, no flags); the
    // fields the dialog needs are set below, and `custom` outlives the call.
    let (ok, chosen) = unsafe {
        let mut choose: CHOOSECOLORW = zeroed();
        choose.lStructSize = size_of::<CHOOSECOLORW>() as u32;
        choose.hwndOwner = out.owner;
        choose.rgbResult = colorref(rgb);
        choose.lpCustColors = custom.as_mut_ptr();
        choose.Flags = CC_RGBINIT | CC_FULLOPEN | CC_ENABLEHOOK;
        choose.lpfnHook = Some(color_hook);
        (ChooseColorW(&mut choose) != 0, choose.rgbResult)
    };
    let picked = if RESET_COLOR.swap(false, Ordering::Relaxed) {
        match Palette::default_slot(SlotKind::Color, slot) {
            Some(SlotValue::Color(default)) => Some(default),
            _ => None,
        }
    } else {
        ok.then(|| rgb_from_colorref(chosen))
    };
    if let Some(rgb) = picked {
        out.edit(Event::EditColor { slot, rgb });
    }
}

/// The hook of the Choose Color dialog: adds a "Reset" button once the dialog
/// has its final shape, and ends the dialog when it is pressed (the slot's
/// default is then sent by `choose_color`).
unsafe extern "system" fn color_hook(
    dialog: HWND,
    message: u32,
    wparam: WPARAM,
    _lparam: LPARAM,
) -> usize {
    match message {
        // The dialog lays itself out (full-open) after this message; wait for
        // the posted one to see its final size.
        WM_INITDIALOG => {
            // SAFETY: `dialog` is the live dialog this hook belongs to.
            unsafe { PostMessageW(dialog, WM_APP, 0, 0) };
            0
        }
        // SAFETY: as above.
        WM_APP => unsafe { add_reset(dialog) },
        WM_COMMAND if wparam & 0xFFFF == ID_RESET_COLOR => {
            RESET_COLOR.store(true, Ordering::Relaxed);
            // SAFETY: ends the live dialog this hook belongs to.
            unsafe { EndDialog(dialog, 0) };
            1
        }
        _ => 0,
    }
}

/// Grows the dialog by a strip at the bottom and puts "Reset" in it, where no
/// control of the system dialog is.
///
/// # Safety
/// `dialog` is the live Choose Color dialog of this thread.
unsafe fn add_reset(dialog: HWND) -> usize {
    let dpi = unsafe { GetDpiForWindow(dialog) }.max(96);
    let strip = scaled(40, dpi);
    // SAFETY: all-zero RECTs are valid; both calls fill them in.
    unsafe {
        let (mut client, mut outer): (RECT, RECT) = (zeroed(), zeroed());
        GetClientRect(dialog, &mut client);
        GetWindowRect(dialog, &mut outer);
        SetWindowPos(
            dialog,
            core::ptr::null_mut(),
            0,
            0,
            outer.right - outer.left,
            outer.bottom - outer.top + strip,
            SWP_NOMOVE | SWP_NOZORDER,
        );
        let class = wide("BUTTON");
        child(
            dialog,
            class.as_ptr(),
            "Reset",
            BS_PUSHBUTTON as u32,
            ID_RESET_COLOR,
            [
                scaled(12, dpi),
                client.bottom + scaled(8, dpi),
                scaled(80, dpi),
                scaled(26, dpi),
            ],
        );
    }
    1
}
