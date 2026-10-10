//! Windows: layered window fed by a BGRA DIB through `UpdateLayeredWindow`.
//!
//! Compile-checked on CI (`cargo check --target x86_64-pc-windows-msvc`);
//! not yet run on real hardware.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::borrow_as_ptr,
    reason = "Win32 FFI: struct fields and flags have fixed C integer types, sizes are bounded by a display"
)]

use core::ffi::c_void;
use core::mem::{size_of, zeroed};
use core::ptr::{null, null_mut};
use markuli_core::{Damage, Format, Insets};
use std::path::PathBuf;
use tiny_skia::PixmapMut;
use tray_icon::{Icon, TrayIconBuilder};
use windows_sys::Win32::Foundation::{GlobalFree, ERROR_FILE_NOT_FOUND, HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC, HMONITOR, MONITORINFO,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, REG_SZ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW, UpdateLayeredWindowIndirect, GWL_EXSTYLE,
    ULW_ALPHA, UPDATELAYEREDWINDOWINFO, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
};
use winit::event_loop::{ActiveEventLoop, EventLoopBuilder};
use winit::keyboard::ModifiersState;
use winit::monitor::MonitorHandle;
use winit::platform::windows::{MonitorHandleExtWindows, WindowAttributesExtWindows};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowAttributes, WindowLevel};

/// DIB sections are BGRA.
pub const FORMAT: Format = Format::Bgra;

/// How the Super modifier reads in the UI.
pub const LOGO_KEY_NAME: &str = "Win";

/// A mouse cursor for the Overlay: the system arrow, or a picture (winit
/// builds the `HCURSOR`; cursor pixels are device pixels on Windows).
pub struct Shape(winit::window::Cursor);

impl Shape {
    pub fn new(event_loop: &ActiveEventLoop, image: Option<&crate::cursor::Image>, _: f64) -> Self {
        let custom = image.and_then(|image| {
            // winit wants straight alpha; the picture is premultiplied.
            let mut rgba = image.rgba.clone();
            for px in rgba.as_chunks_mut::<4>().0 {
                let a = u16::from(px[3]);
                if a != 0 && a != 255 {
                    for c in &mut px[..3] {
                        *c = (u16::from(*c) * 255 / a).min(255) as u8;
                    }
                }
            }
            let (size, hot_x, hot_y) = (image.size as u16, image.hot_x as u16, image.hot_y as u16);
            winit::window::CustomCursor::from_rgba(rgba, size, size, hot_x, hot_y).ok()
        });
        Self(match custom {
            Some(source) => event_loop.create_custom_cursor(source).into(),
            None => winit::window::CursorIcon::Default.into(),
        })
    }

    /// The system open or closed hand.
    pub fn system(_: &ActiveEventLoop, grabbing: bool) -> Self {
        Self(
            if grabbing {
                winit::window::CursorIcon::Grabbing
            } else {
                winit::window::CursorIcon::Grab
            }
            .into(),
        )
    }

    pub fn set(&self, window: &Window) {
        window.set_cursor(self.0.clone());
    }

    /// Windows keeps the cursor on its own.
    #[allow(clippy::unused_self, reason = "same interface as macOS")]
    pub fn keep(&self, _: &Window) {}
}

/// Windows gives the focus back to the previous window by itself when the
/// Overlay (never active, see `window_attributes`) goes away.
pub struct Previous;

pub fn frontmost_other() -> Option<Previous> {
    None
}

impl Previous {
    #[allow(
        clippy::unused_self,
        reason = "same interface as macOS, where it activates the app"
    )]
    pub fn restore(self) {}
}

pub fn configure_event_loop<T>(_builder: &mut EventLoopBuilder<T>) {}

pub fn window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    attributes
        // Per-pixel alpha comes from UpdateLayeredWindow, not DWM blur-behind.
        .with_transparent(false)
        .with_skip_taskbar(true)
        .with_active(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
}

/// The monitor containing the cursor; both are physical pixels.
pub fn monitor_under_cursor(event_loop: &ActiveEventLoop) -> Option<MonitorHandle> {
    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: `at` is a valid out pointer.
    if unsafe { GetCursorPos(&mut at) } == 0 {
        return None;
    }
    event_loop.available_monitors().find(|monitor| {
        let (position, size) = (monitor.position(), monitor.size());
        let right = position.x.saturating_add_unsigned(size.width);
        let bottom = position.y.saturating_add_unsigned(size.height);
        at.x >= position.x && at.y >= position.y && at.x < right && at.y < bottom
    })
}

/// Physical pixels at each edge of `monitor` outside its work area (a taskbar
/// docked on any side). The toolbar is laid out clear of them.
pub fn insets(monitor: &MonitorHandle) -> Insets {
    // SAFETY: an all-zero MONITORINFO is valid; `cbSize` is set before the call
    // and the handle comes from a live winit monitor.
    unsafe {
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor.hmonitor() as HMONITOR, &mut info) == 0 {
            return Insets::default();
        }
        let (work, full) = (info.rcWork, info.rcMonitor);
        let side = |v: i32| u32::try_from(v).unwrap_or(0);
        Insets {
            top: side(work.top - full.top),
            bottom: side(full.bottom - work.bottom),
            left: side(work.left - full.left),
            right: side(full.right - work.right),
        }
    }
}

/// Windows pens arrive as touch events with a force (see `main.rs`), so
/// mouse events carry no pressure.
pub fn pen_pressure() -> Option<f32> {
    None
}

pub struct Presenter {
    hwnd: HWND,
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
    width: u32,
    height: u32,
}

impl Presenter {
    /// Makes the window layered and allocates the DIB it is fed from.
    /// `set_cursor_hittest(false)` is called once here, so winit's own flag
    /// set includes `WS_EX_LAYERED` and does not drop it on later changes.
    pub fn new(window: &Window) -> Self {
        let RawWindowHandle::Win32(handle) = window
            .window_handle()
            .expect("overlay window has a native handle")
            .as_raw()
        else {
            unreachable!("Windows windows have Win32 handles");
        };
        let _ = window.set_cursor_hittest(false);
        let hwnd = handle.hwnd.get() as HWND;
        let size = window.inner_size();
        let (width, height) = (size.width.max(1), size.height.max(1));
        // SAFETY: `hwnd` is a live window of this thread; the GDI objects
        // created here are released in `Drop`.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            // TOOLWINDOW keeps the Overlay out of Alt-Tab.
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                style | WS_EX_LAYERED as isize | WS_EX_TOOLWINDOW as isize,
            );
            let dc = CreateCompatibleDC(null_mut());
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = width as i32;
            info.bmiHeader.biHeight = -(height as i32); // top-down
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut c_void = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            assert!(
                !bitmap.is_null() && !bits.is_null(),
                "CreateDIBSection failed"
            );
            SelectObject(dc, bitmap);
            Self {
                hwnd,
                dc,
                bitmap,
                bits: bits.cast(),
                width,
                height,
            }
        }
    }

    pub fn buffer(&mut self) -> PixmapMut<'_> {
        let len = self.width as usize * self.height as usize * 4;
        // SAFETY: the DIB section owns `len` bytes at `bits` until `Drop`, and
        // `&mut self` guarantees exclusive access.
        let bytes = unsafe { core::slice::from_raw_parts_mut(self.bits, len) };
        PixmapMut::from_bytes(bytes, self.width, self.height)
            .expect("buffer length matches its size")
    }

    /// Pushes only the damaged rectangle to the screen.
    pub fn present(&mut self, damage: Damage) {
        let dirty = RECT {
            left: damage.x as i32,
            top: damage.y as i32,
            right: (damage.x + damage.width) as i32,
            bottom: (damage.y + damage.height) as i32,
        };
        let size = SIZE {
            cx: self.width as i32,
            cy: self.height as i32,
        };
        let source = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: all pointers refer to locals that outlive the call.
        unsafe {
            let mut info: UPDATELAYEREDWINDOWINFO = zeroed();
            info.cbSize = size_of::<UPDATELAYEREDWINDOWINFO>() as u32;
            info.hdcDst = null_mut();
            info.pptDst = null();
            info.psize = &size;
            info.hdcSrc = self.dc;
            info.pptSrc = &source;
            info.pblend = &blend;
            info.dwFlags = ULW_ALPHA;
            info.prcDirty = &dirty;
            UpdateLayeredWindowIndirect(self.hwnd, &info);
        }
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        // SAFETY: created in `new`, released exactly once.
        unsafe {
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

pub fn tray_icon(builder: TrayIconBuilder, icon: Icon) -> TrayIconBuilder {
    builder.with_icon(icon)
}

pub fn config_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Markuli"))
}

/// UTF-16 with a trailing nul, for Win32 wide-string parameters.
pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "Markuli";

/// Adds or removes `HKCU\...\Run\Markuli`, which Explorer starts at login.
pub fn set_launch_at_login(on: bool) -> Result<(), String> {
    let key = wide(RUN_KEY);
    let name = wide(RUN_VALUE);
    let mut hkey: HKEY = null_mut();
    // SAFETY: all pointers refer to live locals; the key is closed below.
    unsafe {
        let status = RegCreateKeyW(HKEY_CURRENT_USER, key.as_ptr(), &mut hkey);
        if status != 0 {
            return Err(format!("registry error {status}"));
        }
        let status = if on {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            // Quoted, so a path with spaces still launches.
            let value = wide(&format!("\"{}\"", exe.display()));
            RegSetValueExW(
                hkey,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        } else {
            match RegDeleteValueW(hkey, name.as_ptr()) {
                ERROR_FILE_NOT_FOUND => 0,
                other => other,
            }
        };
        RegCloseKey(hkey);
        if status == 0 {
            Ok(())
        } else {
            Err(format!("registry error {status}"))
        }
    }
}

/// The clipboard format for UTF-16 text (`CF_UNICODETEXT`).
const CF_UNICODETEXT: u32 = 13;

/// Replaces the clipboard with plain text (what Excalidraw pastes from).
/// Failures are ignored: there is nothing useful to tell the user mid-draw.
pub fn set_clipboard_text(text: &str) {
    let utf16 = wide(text);
    let bytes = utf16.len() * size_of::<u16>();
    // SAFETY: the memory block is sized for `utf16`, locked while written and
    // handed to the clipboard, which then owns it; on any failure before the
    // hand-over it is freed here. The clipboard is opened and closed in this
    // function with no window (allowed for a process without a clipboard
    // owner window).
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return;
        }
        EmptyClipboard();
        let block = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if !block.is_null() {
            let target = GlobalLock(block).cast::<u16>();
            if target.is_null() {
                GlobalFree(block);
            } else {
                core::ptr::copy_nonoverlapping(utf16.as_ptr(), target, utf16.len());
                GlobalUnlock(block);
                if SetClipboardData(CF_UNICODETEXT, block).is_null() {
                    GlobalFree(block);
                }
            }
        }
        CloseClipboard();
    }
}

/// Ctrl is the shortcut modifier on Windows.
pub fn command_held(modifiers: ModifiersState) -> bool {
    modifiers.control_key()
}

/// Windows has a right button: no modifier turns a left click into one.
pub fn secondary_click_modifier() -> bool {
    false
}

/// Nothing to hand back: the DIB section is freed with the window.
pub fn release_memory() {}
