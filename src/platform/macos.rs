//! macOS: Accessory app, `CALayer` presentation, no permission prompts.

use markuli_core::{Damage, Format};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{msg_send, MainThreadMarker};
use objc2_app_kit::{
    NSEvent, NSScreen, NSScreenSaverWindowLevel, NSView, NSWindowCollectionBehavior,
};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
};
use objc2_quartz_core::{CALayer, CATransaction};
use std::ffi::c_void;
use std::ptr;
use tiny_skia::PixmapMut;
use tray_icon::{Icon, TrayIconBuilder};
use winit::event_loop::{ActiveEventLoop, EventLoopBuilder};
use winit::monitor::MonitorHandle;
use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowAttributes, WindowLevel};

pub const FORMAT: Format = Format::Rgba;

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

pub struct Presenter {
    layer: Retained<CALayer>,
    width: u32,
    height: u32,
    /// Premultiplied RGBA, the buffer the core draws into. A `CGImage` on this
    /// memory is handed to Core Animation, which copies it on commit.
    pixels: Vec<u8>,
}

impl Presenter {
    /// Prepares the window as an Overlay and allocates its buffer.
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

        // SAFETY: plain property messages on a live NSView.
        let layer: Retained<CALayer> = unsafe {
            let _: () = msg_send![&*view, setWantsLayer: Bool::YES];
            msg_send![&*view, layer]
        };
        layer.setContentsScale(window.scale_factor());

        let size = window.inner_size();
        let (width, height) = (size.width.max(1), size.height.max(1));
        let pixels = vec![0_u8; width as usize * height as usize * 4];
        Self {
            layer,
            width,
            height,
            pixels,
        }
    }

    pub fn buffer(&mut self) -> PixmapMut<'_> {
        PixmapMut::from_bytes(&mut self.pixels, self.width, self.height)
            .expect("buffer length matches its size")
    }

    /// Publishes the buffer. Core Animation has no partial update for a
    /// `CGImage`, so the damage rectangle is not needed here.
    pub fn present(&mut self, _damage: Damage) {
        let provider = {
            // SAFETY: `pixels` outlives the call; Core Animation copies the
            // bytes on commit, below. No release callback: we own the memory.
            unsafe {
                CGDataProvider::with_data(
                    ptr::null_mut(),
                    self.pixels.as_ptr().cast::<c_void>(),
                    self.pixels.len(),
                    None,
                )
            }
        }
        .expect("data provider");
        let colors = CGColorSpace::new_device_rgb().expect("device RGB colour space");
        // RGBA byte order with premultiplied alpha equals tiny-skia's layout.
        let info = CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0);
        // SAFETY: dimensions and stride match the provider's data length.
        let image = unsafe {
            CGImage::new(
                self.width as usize,
                self.height as usize,
                8,
                32,
                self.width as usize * 4,
                Some(&colors),
                info,
                Some(&provider),
                ptr::null(),
                false,
                CGColorRenderingIntent::RenderingIntentDefault,
            )
        }
        .expect("CGImage");
        CATransaction::begin();
        // Without this, every contents change cross-fades for 0.25 s.
        CATransaction::setDisableActions(true);
        // SAFETY: contents accepts a CGImage.
        unsafe {
            let object: &AnyObject = &*ptr::from_ref(&*image).cast::<AnyObject>();
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

/// Monochrome icon that macOS tints for light and dark menu bars.
pub fn tray_icon(builder: TrayIconBuilder, icon: Icon) -> TrayIconBuilder {
    builder.with_icon_templated(icon)
}
