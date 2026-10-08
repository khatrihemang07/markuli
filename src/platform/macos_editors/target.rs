//! The Objective-C target of every control in the macOS editors, and the small
//! `AppKit` helpers they share. It only reports values (`EditorEvent`); the core
//! owns the Palette.

use crate::editors::{emit, EditorEvent};
use markuli_core::{Palette, SlotKind, SlotValue};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSButton, NSColor, NSColorPanel, NSColorSpace, NSSlider, NSTextField};
use objc2_foundation::{NSNotification, NSObject, NSPoint, NSRect, NSSize, NSString};
use std::cell::{Cell, OnceCell};

#[derive(Default)]
pub(super) struct Ivars {
    slot: Cell<usize>,
    /// Which opened editor this target belongs to; its `Closed` carries it so
    /// the app can tell a stale close from the current editor's.
    generation: Cell<u64>,
    slider: OnceCell<Retained<NSSlider>>,
    field: OnceCell<Retained<NSTextField>>,
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

        #[unsafe(method(resetColor:))]
        fn reset_color(&self, _sender: Option<&AnyObject>) {
            let Some(mtm) = MainThreadMarker::new() else { return };
            let slot = self.ivars().slot.get();
            let Some(SlotValue::Color(rgb)) = Palette::default_slot(SlotKind::Color, slot) else {
                return;
            };
            NSColorPanel::sharedColorPanel(mtm).setColor(&color_of(rgb));
            emit(EditorEvent::Color { slot, rgb });
        }

        #[unsafe(method(widthSlider:))]
        fn width_slider(&self, sender: &NSSlider) {
            self.set_width(sender.doubleValue());
        }

        #[unsafe(method(widthField:))]
        fn width_field(&self, sender: &NSTextField) {
            self.set_width(sender.doubleValue());
        }

        #[unsafe(method(resetWidth:))]
        fn reset_width(&self, _sender: Option<&AnyObject>) {
            if let Some(SlotValue::Width(width)) =
                Palette::default_slot(SlotKind::Width, self.ivars().slot.get())
            {
                self.set_width(f64::from(width));
            }
        }

        // The color panel's delegate call and the popover's: either closing
        // ends the edit.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _note: &NSNotification) {
            self.closed();
        }

        #[unsafe(method(popoverDidClose:))]
        fn popover_did_close(&self, _note: &NSNotification) {
            self.closed();
        }
    }
);

impl Target {
    pub(super) fn new(mtm: MainThreadMarker, slot: usize, generation: u64) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            slot: Cell::new(slot),
            generation: Cell::new(generation),
            ..Ivars::default()
        });
        // SAFETY: plain NSObject init on the main thread.
        unsafe { msg_send![super(this), init] }
    }

    /// Hands the width slider and number field to the target, which keeps
    /// them in step.
    pub(super) fn set_controls(&self, slider: Retained<NSSlider>, field: Retained<NSTextField>) {
        let _ = self.ivars().slider.set(slider);
        let _ = self.ivars().field.set(field);
    }

    fn closed(&self) {
        emit(EditorEvent::Closed {
            generation: self.ivars().generation.get(),
        });
    }

    /// Shows `value` in both controls and reports it.
    fn set_width(&self, value: f64) {
        #[allow(clippy::cast_possible_truncation, reason = "widths are tiny")]
        let Some(width) = Palette::snap_width(value as f32) else {
            return;
        };
        let ivars = self.ivars();
        if let Some(slider) = ivars.slider.get() {
            slider.setDoubleValue(f64::from(width));
        }
        if let Some(field) = ivars.field.get() {
            field.setStringValue(&NSString::from_str(&Palette::width_text(width)));
        }
        emit(EditorEvent::Width {
            slot: ivars.slot.get(),
            width,
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

pub(super) fn frame(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

pub(super) fn reset_button(
    target: &Target,
    action: Sel,
    mtm: MainThreadMarker,
) -> Retained<NSButton> {
    // SAFETY: the target implements both reset actions.
    unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::from_str("Reset"),
            Some(target),
            Some(action),
            mtm,
        )
    }
}
