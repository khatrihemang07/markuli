//! The Objective-C target and delegate of the macOS color panel, and the sRGB
//! helpers. It only reports values (`EditorEvent`); the core owns the Palette.

use crate::editors::{emit, EditorEvent};
use objc2::rc::Retained;
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSColor, NSColorPanel, NSColorSpace};
use objc2_foundation::{NSNotification, NSObject};
use std::cell::Cell;

#[derive(Default)]
pub(super) struct Ivars {
    slot: Cell<usize>,
    /// Which opened panel this target belongs to; its `Closed` carries it so
    /// the app can tell a stale close from the current panel's.
    generation: Cell<u64>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements and `Target` does not
    // implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MarkuliEditorTarget"]
    #[ivars = Ivars]
    pub(super) struct Target;

    impl Target {
        #[unsafe(method(colorChanged:))]
        fn color_changed(&self, panel: &NSColorPanel) {
            if let Some(rgb) = srgb_of(&panel.color()) {
                emit(EditorEvent::Color { slot: self.ivars().slot.get(), rgb });
            }
        }

        // The color panel's delegate call: closing ends the edit.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _note: &NSNotification) {
            self.closed();
        }
    }
);

impl Target {
    pub(super) fn new(mtm: MainThreadMarker, slot: usize, generation: u64) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            slot: Cell::new(slot),
            generation: Cell::new(generation),
        });
        // SAFETY: plain NSObject init on the main thread.
        unsafe { msg_send![super(this), init] }
    }

    fn closed(&self) {
        emit(EditorEvent::Closed {
            generation: self.ivars().generation.get(),
        });
    }
}

pub(super) fn color_of(rgb: [u8; 3]) -> Retained<NSColor> {
    let c = |b: u8| f64::from(b) / 255.0;
    NSColor::colorWithSRGBRed_green_blue_alpha(c(rgb[0]), c(rgb[1]), c(rgb[2]), 1.0)
}

/// A color component in 0..=1 as a byte.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 first"
)]
fn byte_of(component: f64) -> u8 {
    (component * 255.0).round().clamp(0.0, 255.0) as u8
}

fn srgb_of(color: &NSColor) -> Option<[u8; 3]> {
    let color = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some([
        byte_of(color.redComponent()),
        byte_of(color.greenComponent()),
        byte_of(color.blueComponent()),
    ])
}
