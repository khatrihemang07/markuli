//! macOS: Accessory app, `CALayer` presentation, no permission prompts.

use markuli_core::{Damage, Format};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{msg_send, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSEvent, NSEventSubtype, NSEventType, NSPasteboard, NSPasteboardTypeString,
    NSScreen, NSScreenSaverWindowLevel, NSView, NSWindowCollectionBehavior,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{ns_string, NSDictionary, NSNumber, NSString};
use objc2_quartz_core::{CALayer, CATransaction};
use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr;
use tiny_skia::PixmapMut;
use tray_icon::{Icon, TrayIconBuilder};
use winit::event_loop::{ActiveEventLoop, EventLoopBuilder};
use winit::keyboard::ModifiersState;
use winit::monitor::MonitorHandle;
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowAttributes, WindowLevel};

/// The surface is BGRA, so the core paints BGRA and nothing is converted.
pub const FORMAT: Format = Format::Bgra;

/// How the Super modifier reads in the UI.
pub const LOGO_KEY_NAME: &str = "Cmd";

/// No Dock icon, no app menu bar, no activation prompts.
pub fn configure_event_loop<T>(builder: &mut EventLoopBuilder<T>) {
    builder
        .with_activation_policy(ActivationPolicy::Accessory)
        .with_default_menu(false);
}

pub fn window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    attributes
        .with_transparent(true)
        .with_window_level(WindowLevel::AlwaysOnTop)
}

/// The monitor containing the mouse. `NSEvent.mouseLocation` needs no
/// permission; it is in points, bottom-left origin of the primary screen.
pub fn monitor_under_cursor(event_loop: &ActiveEventLoop) -> Option<MonitorHandle> {
    let mtm = MainThreadMarker::new()?;
    let primary_height = NSScreen::screens(mtm).firstObject()?.frame().size.height;
    let location = NSEvent::mouseLocation();
    let (x, y) = (location.x, primary_height - location.y);
    event_loop.available_monitors().find(|monitor| {
        let scale = monitor.scale_factor();
        let (left, top) = (
            f64::from(monitor.position().x) / scale,
            f64::from(monitor.position().y) / scale,
        );
        let (width, height) = (
            f64::from(monitor.size().width) / scale,
            f64::from(monitor.size().height) / scale,
        );
        x >= left && y >= top && x < left + width && y < top + height
    })
}

/// Pressure (0..=1) of the stylus behind the mouse event being dispatched, or
/// `None` for a mouse or trackpad. winit does not expose this, so read the
/// current `NSEvent`. Only the subtype `TabletPoint` counts: a Force Touch
/// trackpad also reports `pressure`, and must keep simulating it. Own-app
/// events only: no permission needed. Unverified on real tablet hardware.
pub fn pen_pressure() -> Option<f32> {
    let event = NSApplication::sharedApplication(MainThreadMarker::new()?).currentEvent()?;
    // `subtype` raises on event types that have none, so check the type first.
    let is_mouse = matches!(
        event.r#type(),
        NSEventType::LeftMouseDown | NSEventType::LeftMouseDragged | NSEventType::LeftMouseUp
    );
    (is_mouse && event.subtype() == NSEventSubtype::TabletPoint).then(|| event.pressure())
}

/// `IOSurfaceRef` and the few C functions used on it. Core Animation shows an
/// `IOSurface` without copying it and the core draws straight into its memory,
/// so presenting a frame allocates nothing. (The first presenter built a new
/// `CGImage` per frame, which Core Animation copies on commit: 8 MB per frame
/// per 1080p display, and RSS spiked to ~220 MB while drawing.)
type IOSurfaceRef = *mut c_void;

#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    fn IOSurfaceCreate(properties: *const c_void) -> IOSurfaceRef;
    fn IOSurfaceLock(surface: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    fn IOSurfaceUnlock(surface: IOSurfaceRef, options: u32, seed: *mut u32) -> i32;
    fn IOSurfaceGetBaseAddress(surface: IOSurfaceRef) -> *mut c_void;
    fn IOSurfaceGetBytesPerRow(surface: IOSurfaceRef) -> usize;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(object: *const c_void);
}

/// 'BGRA': bytes B, G, R, A in memory, premultiplied, as the core's `Bgra`.
const PIXEL_FORMAT_BGRA: u64 = 0x4247_5241;

pub struct Presenter {
    layer: Retained<CALayer>,
    surface: IOSurfaceRef,
    /// Surface width in pixels: the window width rounded up to a whole
    /// 64-byte row, because a `Pixmap` needs tightly packed rows. The layer
    /// shows only the first columns (`contentsRect`).
    padded_width: u32,
    height: u32,
    locked: bool,
}

impl Presenter {
    /// Prepares the window as an Overlay and allocates its surface.
    /// Must run on the main thread right after the window is created.
    pub fn new(window: &Window) -> Self {
        let RawWindowHandle::AppKit(handle) = window
            .window_handle()
            .expect("overlay window has a native handle")
            .as_raw()
        else {
            unreachable!("macOS windows have AppKit handles");
        };
        // SAFETY: the handle comes from a live winit window on the main thread
        // and `ns_view` is its NSView; we retain it for our own lifetime.
        let view: Retained<NSView> = unsafe {
            Retained::retain(handle.ns_view.as_ptr().cast::<NSView>())
                .expect("window has an NSView")
        };
        let ns_window = view.window().expect("view is in a window");
        // No new Space, not in Cmd-Tab / Mission Control, visible over full-screen apps.
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns_window.setLevel(NSScreenSaverWindowLevel);
        ns_window.setOpaque(false);
        ns_window.setHasShadow(false);

        // A layer-hosting view: our own layer, set before `wantsLayer`. A
        // view-owned layer gets its contents reset by AppKit's first display
        // pass (after the window shows, and on appearance changes), which
        // wiped the first frame until the next present.
        let layer = CALayer::new();
        // SAFETY: plain property messages on a live NSView with a live layer.
        unsafe {
            let _: () = msg_send![&*view, setLayer: &*layer];
            let _: () = msg_send![&*view, setWantsLayer: Bool::YES];
        }
        layer.setContentsScale(window.scale_factor());

        let size = window.inner_size();
        let (width, height) = (size.width.max(1), size.height.max(1));
        let padded_width = width.div_ceil(16) * 16;
        let surface = create_surface(padded_width, height);
        let crop = f64::from(width) / f64::from(padded_width);
        layer.setContentsRect(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(crop, 1.0)));
        Self {
            layer,
            surface,
            padded_width,
            height,
            locked: false,
        }
    }

    /// The surface's pixels, locked for CPU writes until `present`.
    pub fn buffer(&mut self) -> PixmapMut<'_> {
        if !self.locked {
            // SAFETY: `surface` is the live surface created in `new`.
            let status = unsafe { IOSurfaceLock(self.surface, 0, ptr::null_mut()) };
            assert_eq!(status, 0, "IOSurfaceLock failed");
            self.locked = true;
        }
        let len = self.padded_width as usize * self.height as usize * 4;
        // SAFETY: the surface owns `len` bytes at its base address (rows are
        // tightly packed, checked in `create_surface`) while it is locked, and
        // `&mut self` guarantees exclusive access.
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(IOSurfaceGetBaseAddress(self.surface).cast::<u8>(), len)
        };
        PixmapMut::from_bytes(bytes, self.padded_width, self.height)
            .expect("buffer length matches its size")
    }

    /// Unlocks the surface and tells Core Animation its contents changed.
    /// There is no partial update, so the damage rectangle is not needed.
    pub fn present(&mut self, _damage: Damage) {
        if self.locked {
            // SAFETY: locked by `buffer`, same surface.
            unsafe { IOSurfaceUnlock(self.surface, 0, ptr::null_mut()) };
            self.locked = false;
        }
        CATransaction::begin();
        // Without this, every contents change cross-fades for 0.25 s.
        CATransaction::setDisableActions(true);
        // SAFETY: an `IOSurfaceRef` is an Objective-C object that `contents`
        // accepts; assigning it again after a CPU write publishes the new seed.
        unsafe {
            let object: &AnyObject = &*self.surface.cast::<AnyObject>();
            self.layer.setContents(Some(object));
        }
        CATransaction::commit();
    }

    #[allow(
        clippy::unused_self,
        reason = "same signature as the Windows presenter"
    )]
    pub fn set_click_through(&self, window: &Window, click_through: bool) {
        // Winit maps this to `setIgnoresMouseEvents`.
        let _ = window.set_cursor_hittest(!click_through);
    }
}

impl Drop for Presenter {
    fn drop(&mut self) {
        // SAFETY: the surface was created (+1) in `new` and is released once;
        // an unpresented lock is dropped first.
        unsafe {
            if self.locked {
                IOSurfaceUnlock(self.surface, 0, ptr::null_mut());
            }
            self.layer.setContents(None);
            CFRelease(self.surface);
        }
    }
}

/// Creates a tightly packed BGRA surface.
fn create_surface(width: u32, height: u32) -> IOSurfaceRef {
    let row = u64::from(width) * 4;
    let keys = [
        ns_string!("IOSurfaceWidth"),
        ns_string!("IOSurfaceHeight"),
        ns_string!("IOSurfaceBytesPerElement"),
        ns_string!("IOSurfaceBytesPerRow"),
        ns_string!("IOSurfacePixelFormat"),
    ];
    let values = [
        NSNumber::numberWithUnsignedLongLong(u64::from(width)),
        NSNumber::numberWithUnsignedLongLong(u64::from(height)),
        NSNumber::numberWithUnsignedLongLong(4),
        NSNumber::numberWithUnsignedLongLong(row),
        NSNumber::numberWithUnsignedLongLong(PIXEL_FORMAT_BGRA),
    ];
    let properties = NSDictionary::from_retained_objects(&keys, &values);
    // SAFETY: an NSDictionary is a toll-free bridged CFDictionary.
    let surface = unsafe { IOSurfaceCreate(Retained::as_ptr(&properties).cast()) };
    assert!(!surface.is_null(), "IOSurfaceCreate failed");
    // SAFETY: just created.
    let actual = unsafe { IOSurfaceGetBytesPerRow(surface) };
    assert_eq!(actual as u64, row, "IOSurface rows are not tightly packed");
    surface
}

/// Monochrome icon that macOS tints for light and dark menu bars.
pub fn tray_icon(builder: TrayIconBuilder, icon: Icon) -> TrayIconBuilder {
    builder.with_icon_templated(icon)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn config_dir() -> Option<PathBuf> {
    Some(home()?.join("Library/Application Support/Markuli"))
}

const AGENT_LABEL: &str = "com.markuli.app";

/// With the `dev-hooks` feature, `MARKULI_LAUNCH_AGENT_DIR` redirects the
/// plist, so manual tests never touch the real `~/Library/LaunchAgents`.
fn launch_agent_path() -> Option<PathBuf> {
    #[cfg(feature = "dev-hooks")]
    let over = std::env::var_os("MARKULI_LAUNCH_AGENT_DIR").map(PathBuf::from);
    #[cfg(not(feature = "dev-hooks"))]
    let over = None;
    let dir = over.or_else(|| Some(home()?.join("Library/LaunchAgents")))?;
    Some(dir.join(format!("{AGENT_LABEL}.plist")))
}

/// A user `LaunchAgent` that starts this executable at login. No `launchctl`
/// call: launchd reads the folder at the next login.
fn launch_agent_plist(exe: &str) -> String {
    let exe = exe
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n\
\t<key>Label</key>\n\t<string>{AGENT_LABEL}</string>\n\
\t<key>ProgramArguments</key>\n\t<array>\n\t\t<string>{exe}</string>\n\t</array>\n\
\t<key>RunAtLoad</key>\n\t<true/>\n\
\t<key>ProcessType</key>\n\t<string>Interactive</string>\n\
</dict>\n</plist>\n"
    )
}

pub fn set_launch_at_login(on: bool) -> Result<(), String> {
    let path = launch_agent_path().ok_or("no home directory")?;
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_str().ok_or("executable path is not UTF-8")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, launch_agent_plist(exe)).map_err(|e| e.to_string())
}

/// Cmd is the shortcut modifier on macOS.
pub fn command_held(modifiers: ModifiersState) -> bool {
    modifiers.super_key()
}

/// Replaces the clipboard with plain text (what Excalidraw pastes from).
pub fn set_clipboard_text(text: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: a constant string AppKit exports; it is valid for the process.
    let kind = unsafe { NSPasteboardTypeString };
    pasteboard.setString_forType(&NSString::from_str(text), kind);
}

extern "C" {
    fn malloc_zone_pressure_relief(zone: *mut c_void, goal: usize) -> usize;
}

/// Hands freed heap pages back to the OS after the Overlay is destroyed, so
/// they stop counting toward the footprint the idle budget is measured by.
pub fn release_memory() {
    // SAFETY: documented libmalloc call; a null zone means all zones and a
    // goal of 0 means "release as much as possible". No preconditions.
    unsafe {
        malloc_zone_pressure_relief(ptr::null_mut(), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::launch_agent_plist;

    #[test]
    fn launch_agent_runs_the_executable_at_load() {
        let plist = launch_agent_plist("/Applications/Markuli & Co/markuli");
        assert!(plist.contains("<string>/Applications/Markuli &amp; Co/markuli</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n\t<true/>"));
        assert!(plist.contains("<string>com.markuli.app</string>"));
    }
}
