//! macOS: Accessory app, `CALayer` presentation, no permission prompts.

use markuli_core::{Damage, Format, Insets};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{msg_send, sel, AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSBitmapImageRep, NSCursor,
    NSDeviceRGBColorSpace, NSEvent, NSEventSubtype, NSEventType, NSImage, NSPasteboard,
    NSPasteboardTypeString, NSRunningApplication, NSScreen, NSScreenSaverWindowLevel, NSView,
    NSWindowCollectionBehavior, NSWorkspace,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{ns_string, NSDictionary, NSNumber, NSPoint, NSSize, NSString};
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

/// Physical pixels at each edge of `monitor` that macOS keeps for itself: the
/// menu bar, the Dock on any side, and a notch's safe area. The toolbar is
/// laid out clear of them.
pub fn insets(monitor: &MonitorHandle) -> Insets {
    let none = Insets::default();
    let Some(mtm) = MainThreadMarker::new() else {
        return none;
    };
    let screens = NSScreen::screens(mtm);
    let Some(primary) = screens.firstObject() else {
        return none;
    };
    let primary_height = primary.frame().size.height;
    let scale = monitor.scale_factor();
    let (left, top) = (
        f64::from(monitor.position().x) / scale,
        f64::from(monitor.position().y) / scale,
    );
    // The NSScreen whose frame is the monitor (global points, top-left origin).
    let Some(screen) = screens.iter().find(|screen| {
        let frame = screen.frame();
        (frame.origin.x - left).abs() < 1.0
            && (primary_height - frame.origin.y - frame.size.height - top).abs() < 1.0
    }) else {
        return none;
    };
    let (frame, visible) = (screen.frame(), screen.visibleFrame());
    let safe = screen.safeAreaInsets();
    // Cocoa's origin is bottom-left: the usable area against the frame, per side.
    let physical = |usable: f64, safe: f64| {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a few hundred points, non-negative"
        )]
        let pixels = (usable.max(safe).max(0.0) * scale).round() as u32;
        pixels
    };
    Insets {
        top: physical(
            (frame.origin.y + frame.size.height) - (visible.origin.y + visible.size.height),
            safe.top,
        ),
        bottom: physical(visible.origin.y - frame.origin.y, safe.bottom),
        left: physical(visible.origin.x - frame.origin.x, safe.left),
        right: physical(
            (frame.origin.x + frame.size.width) - (visible.origin.x + visible.size.width),
            safe.right,
        ),
    }
}

/// A mouse cursor for the Overlay: the system arrow, or a picture.
///
/// winit's custom cursors take image pixels as points, so on a 2x display a
/// 2x picture would show twice as large; an `NSCursor` of our own has the size
/// in points and the 2x pixels, so rings stay crisp.
pub struct Shape(Retained<NSCursor>);

impl Shape {
    pub fn new(_: &ActiveEventLoop, image: Option<&crate::cursor::Image>, scale: f64) -> Self {
        Self(
            image
                .and_then(|i| cursor_from(i, scale))
                .unwrap_or_else(NSCursor::arrowCursor),
        )
    }

    /// Makes it the cursor now.
    pub fn set(&self, _: &Window) {
        self.0.set();
    }

    /// Called on every pointer move: `AppKit` may have put another cursor back
    /// (entering the window, the app being activated).
    pub fn keep(&self, window: &Window) {
        self.set(window);
    }
}

fn cursor_from(image: &crate::cursor::Image, scale: f64) -> Option<Retained<NSCursor>> {
    let pixels = isize::try_from(image.size).ok()?;
    // SAFETY: a fresh bitmap that owns its own buffer (null planes); the
    // arguments describe 8-bit RGBA, tightly packed.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            ptr::null_mut(),
            pixels,
            pixels,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            pixels * 4,
            32,
        )
    }?;
    // SAFETY: the bitmap has `size * size * 4` bytes of its own, which is
    // `image.rgba.len()`, and nothing else touches them yet. The data is
    // premultiplied, which is what a bitmap without the non-premultiplied
    // flag expects.
    unsafe {
        core::slice::from_raw_parts_mut(rep.bitmapData(), image.rgba.len())
            .copy_from_slice(&image.rgba);
    }
    let points = f64::from(image.size) / scale;
    rep.setSize(NSSize::new(points, points));
    let picture = NSImage::initWithSize(NSImage::alloc(), NSSize::new(points, points));
    picture.addRepresentation(&rep);
    let hot_x = (f64::from(image.hot_x) + 0.5) / scale;
    let hot_y = (f64::from(image.hot_y) + 0.5) / scale;
    Some(NSCursor::initWithImage_hotSpot(
        NSCursor::alloc(),
        &picture,
        NSPoint::new(hot_x, hot_y),
    ))
}

/// The app that was frontmost before the Overlay took the focus.
pub struct Previous(Retained<NSRunningApplication>);

/// The app the user was working in, unless it is Markuli itself (Settings
/// open): the one to give the focus back to when Draw Mode ends. This is the
/// menu bar owner: `frontmostApplication` already answers Markuli itself while
/// the toggle hotkey is handled, `menuBarOwningApplication` still names the
/// app that had the focus.
pub fn frontmost_other() -> Option<Previous> {
    let app = NSWorkspace::sharedWorkspace().menuBarOwningApplication()?;
    (app != NSRunningApplication::currentApplication()).then_some(Previous(app))
}

impl Previous {
    /// Activates the app again, so keystrokes go back to it and to its key
    /// window without a click. Does nothing when the user already switched
    /// apps while in Draw Mode (Markuli is no longer the active app then).
    pub fn restore(self) {
        if !NSRunningApplication::currentApplication().isActive() {
            return;
        }
        // SAFETY: `respondsToSelector:` is declared on NSObject; `activate`
        // (macOS 14+) is the cooperative activation `activateWithOptions:`
        // was deprecated for, and Markuli is the active app, so it may yield.
        let responds: Bool = unsafe { msg_send![&*self.0, respondsToSelector: sel!(activate)] };
        if responds.as_bool() {
            // SAFETY: a plain no-argument message on a live NSRunningApplication.
            let _: Bool = unsafe { msg_send![&*self.0, activate] };
        } else {
            self.0
                .activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }
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

/// `kIOSurfaceLockReadOnly`: the CPU only reads, so no seed bump.
const LOCK_READ_ONLY: u32 = 1;

pub struct Presenter {
    layer: Retained<CALayer>,
    /// The second surface exists only without the `setContentsChanged` SPI
    /// (see `present`); it is null otherwise.
    surfaces: [IOSurfaceRef; 2],
    /// The surface the layer shows (the last presented one).
    shown: usize,
    /// `CALayer.setContentsChanged` exists: one surface is enough.
    notify: bool,
    /// Double-buffered only: the part of `shown` the other surface lacks.
    stale: Option<Damage>,
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
        // The Overlay sets its own cursor (`Shape`); winit's cursor rect would
        // put its stock cursor back whenever AppKit refreshes the rects.
        ns_window.disableCursorRects();

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
        let notify = can_notify_contents_changed(&layer);
        let surfaces = [
            create_surface(padded_width, height),
            if notify {
                ptr::null_mut()
            } else {
                create_surface(padded_width, height)
            },
        ];
        let crop = f64::from(width) / f64::from(padded_width);
        layer.setContentsRect(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(crop, 1.0)));
        Self {
            layer,
            surfaces,
            shown: 0,
            notify,
            stale: None,
            padded_width,
            height,
            locked: false,
        }
    }

    /// The surface the core renders into next.
    fn back(&self) -> usize {
        if self.notify {
            0
        } else {
            1 - self.shown
        }
    }

    /// The surface's pixels, locked for CPU writes until `present`. Without
    /// the SPI this is the back surface, brought up to date first: the core
    /// renders incrementally, so it must find the last frame there.
    pub fn buffer(&mut self) -> PixmapMut<'_> {
        let back = self.surfaces[self.back()];
        if !self.locked {
            // SAFETY: `back` is a live surface created in `new`.
            let status = unsafe { IOSurfaceLock(back, 0, ptr::null_mut()) };
            assert_eq!(status, 0, "IOSurfaceLock failed");
            self.locked = true;
            if let Some(damage) = self.stale.take() {
                self.copy_from_shown(back, damage);
            }
        }
        let len = self.padded_width as usize * self.height as usize * 4;
        // SAFETY: the surface owns `len` bytes at its base address (rows are
        // tightly packed, checked in `create_surface`) while it is locked, and
        // `&mut self` guarantees exclusive access.
        let bytes = unsafe {
            core::slice::from_raw_parts_mut(IOSurfaceGetBaseAddress(back).cast::<u8>(), len)
        };
        PixmapMut::from_bytes(bytes, self.padded_width, self.height)
            .expect("buffer length matches its size")
    }

    /// Copies `damage` of the shown surface into the locked `back` surface.
    fn copy_from_shown(&self, back: IOSurfaceRef, damage: Damage) {
        let shown = self.surfaces[self.shown];
        let row = self.padded_width as usize * 4;
        let left = (damage.x.min(self.padded_width) as usize) * 4;
        let right = (damage.x.saturating_add(damage.width).min(self.padded_width) as usize) * 4;
        let rows =
            damage.y.min(self.height)..damage.y.saturating_add(damage.height).min(self.height);
        // SAFETY: both surfaces are live, tightly packed `row * height`
        // bytes, and distinct; `shown` is locked read-only for the copy and
        // `back` is already locked by `buffer`. Every copied range lies
        // inside a row (clamped above).
        unsafe {
            IOSurfaceLock(shown, LOCK_READ_ONLY, ptr::null_mut());
            let (from, to) = (
                IOSurfaceGetBaseAddress(shown).cast::<u8>(),
                IOSurfaceGetBaseAddress(back).cast::<u8>(),
            );
            for y in rows {
                let at = y as usize * row + left;
                ptr::copy_nonoverlapping(from.add(at), to.add(at), right.saturating_sub(left));
            }
            IOSurfaceUnlock(shown, LOCK_READ_ONLY, ptr::null_mut());
        }
    }

    /// Unlocks the surface and puts it on screen.
    ///
    /// Assigning the SAME `IOSurface` to `contents` again is a no-op to Core
    /// Animation: it compares objects, not the surface's seed, so the
    /// compositor keeps the old frame (ADR-0003). `setContentsChanged` (the
    /// SPI `WebKit` and Chromium use for `IOSurface` layers) says the pixels
    /// changed. Where it is missing, two surfaces alternate, so `contents`
    /// really changes every frame.
    ///
    /// The surface may be read by the compositor at any time, so the core
    /// writes each damaged band once, already composed (see `render.rs`): no
    /// half-painted frame is ever visible, and one surface is enough.
    pub fn present(&mut self, damage: Damage) {
        if self.locked {
            // SAFETY: locked by `buffer`, same surface.
            unsafe { IOSurfaceUnlock(self.surfaces[self.back()], 0, ptr::null_mut()) };
            self.locked = false;
        }
        if !self.notify {
            self.shown = 1 - self.shown;
            self.stale = Some(damage);
        }
        CATransaction::begin();
        // Without this, every contents change cross-fades for 0.25 s.
        CATransaction::setDisableActions(true);
        // SAFETY: an `IOSurfaceRef` is an Objective-C object that `contents`
        // accepts; `setContentsChanged` was checked with `respondsToSelector:`.
        unsafe {
            let object: &AnyObject = &*self.surfaces[self.shown].cast::<AnyObject>();
            self.layer.setContents(Some(object));
            if self.notify {
                let _: () = msg_send![&*self.layer, setContentsChanged];
            }
        }
        CATransaction::commit();
    }
}

/// Whether `CALayer` has the private `setContentsChanged`. With the
/// `dev-hooks` feature, `MARKULI_PRESENT_FALLBACK` forces the answer to no,
/// so the double-buffered path can be tested on a machine that has the SPI.
fn can_notify_contents_changed(layer: &CALayer) -> bool {
    #[cfg(feature = "dev-hooks")]
    if std::env::var_os("MARKULI_PRESENT_FALLBACK").is_some() {
        return false;
    }
    // SAFETY: `respondsToSelector:` is declared on NSObject.
    let responds: Bool = unsafe { msg_send![layer, respondsToSelector: sel!(setContentsChanged)] };
    responds.as_bool()
}

impl Drop for Presenter {
    fn drop(&mut self) {
        // SAFETY: the surfaces were created (+1) in `new` and are released
        // once; an unpresented lock is dropped first.
        unsafe {
            if self.locked {
                IOSurfaceUnlock(self.surfaces[self.back()], 0, ptr::null_mut());
            }
            self.layer.setContents(None);
            CFRelease(self.surfaces[0]);
            if !self.surfaces[1].is_null() {
                CFRelease(self.surfaces[1]);
            }
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

/// Control+click is the secondary click on a one-button trackpad.
pub fn secondary_click_modifier(modifiers: ModifiersState) -> bool {
    modifiers.control_key()
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
