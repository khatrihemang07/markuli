//! The Windows system color dialog, opened for `View::edit` (the Color
//! Picker's "..." for exact values, ADR-0004). It runs its own modal loop and
//! returns when closed, so the caller needs no state.
//!
//! Re-entrancy: `ChooseColorW` pumps messages on the thread that is inside
//! winit's event handler. That is safe because winit's Windows backend buffers
//! every event that arrives while a handler is running
//! (`EventLoopRunner::send_event`) and delivers it after the handler returns;
//! nothing re-enters `App`, whose only access during the loop is the `send`
//! closure this module was given.
//!
//! Compile-checked on CI; not yet run on real hardware.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::borrow_as_ptr,
    reason = "Win32 FFI: struct fields and flags have fixed C integer types"
)]

use core::mem::{size_of, zeroed};
use markuli_core::{EditRequest, Event, SlotValue};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Controls::Dialogs::{
    ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow};
use winit::raw_window_handle::RawWindowHandle;

/// Opens the color dialog for `request` (a color slot) on top of the Overlay
/// `owner` and returns when it is closed. OK sends the color; `EditEnd` comes
/// last. Nothing is sent once the Overlay is gone (it can be destroyed while
/// the modal loop runs), so the app never hears of an edit for a window it no
/// longer has.
pub fn open_editor(request: EditRequest, owner: RawWindowHandle, send: &mut dyn FnMut(Event)) {
    let (RawWindowHandle::Win32(handle), SlotValue::Color(rgb)) = (owner, request.value) else {
        return;
    };
    let owner = handle.hwnd.get() as HWND;
    // SAFETY: IsWindow accepts any handle value; the rest runs only for a
    // live window, on the thread that owns it.
    unsafe {
        if IsWindow(owner) == 0 {
            return;
        }
        if let Some(rgb) = choose_color(owner, rgb) {
            if IsWindow(owner) != 0 {
                send(Event::EditColor {
                    slot: request.index,
                    rgb,
                });
            }
        }
    }
    send(Event::EditEnd);
    // SAFETY: SetForegroundWindow only asks the system to switch; a window
    // destroyed meanwhile makes it fail harmlessly, but check anyway so the
    // Overlay gets the keyboard back only if it is still there.
    unsafe {
        if IsWindow(owner) != 0 {
            SetForegroundWindow(owner);
        }
    }
}

/// Nothing is left open between calls: the Windows dialog is modal.
pub fn close_editor(_send: &mut dyn FnMut(Event)) {}

/// The Windows dialog never reports closing later.
pub fn editor_closed(_generation: u64, _send: &mut dyn FnMut(Event)) {}

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

/// The Choose Color dialog, full-open, starting at `rgb`; the color picked,
/// or `None` on Cancel.
///
/// # Safety
/// `owner` is the live Overlay window of this thread.
unsafe fn choose_color(owner: HWND, rgb: [u8; 3]) -> Option<[u8; 3]> {
    let mut custom = [0x00FF_FFFF_u32; 16];
    // SAFETY: all-zero is a valid CHOOSECOLORW (null pointers, no flags); the
    // fields the dialog needs are set below, and `custom` outlives the call.
    let (ok, chosen) = unsafe {
        let mut choose: CHOOSECOLORW = zeroed();
        choose.lStructSize = size_of::<CHOOSECOLORW>() as u32;
        choose.hwndOwner = owner;
        choose.rgbResult = colorref(rgb);
        choose.lpCustColors = custom.as_mut_ptr();
        choose.Flags = CC_RGBINIT | CC_FULLOPEN;
        (ChooseColorW(&mut choose) != 0, choose.rgbResult)
    };
    ok.then(|| rgb_from_colorref(chosen))
}
