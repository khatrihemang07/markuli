//! macOS Palette editors: the shared color panel for a color slot, a popover
//! with a slider for a width slot. Both only report values (`EditorEvent`);
//! the core owns the Palette and the undo step (`Event::EditColor`,
//! `EditWidth`, `EditEnd`).

use crate::editors::{emit, EditorEvent};
use markuli_core::{Anchor, EditRequest, Event, Palette, SlotValue, MAX_WIDTH, MIN_WIDTH};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSButton, NSColor, NSColorPanel, NSColorSpace, NSPopover, NSPopoverBehavior, NSSlider,
    NSTextField, NSView, NSViewController, NSWindowLevel,
};
use objc2_foundation::{NSNotification, NSObject, NSPoint, NSRect, NSRectEdge, NSSize, NSString};
use std::cell::{Cell, OnceCell, RefCell};
use winit::raw_window_handle::RawWindowHandle;

// ---- Pure conversions (unit-tested below) --------------------------------

/// A color component in 0..=1 as a byte.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 first"
)]
fn byte_of(component: f64) -> u8 {
    (component * 255.0).round().clamp(0.0, 255.0) as u8
}

fn rgb_of(red: f64, green: f64, blue: f64) -> [u8; 3] {
    [byte_of(red), byte_of(green), byte_of(blue)]
}

/// The slot value a slider or field position stands for: snapped to 0.5 and
/// clamped to the slot range; `None` for NaN or infinity.
#[allow(clippy::cast_possible_truncation, reason = "widths are tiny")]
fn width_of(value: f64) -> Option<f32> {
    Palette::snap_width(value as f32)
}

/// The Anchor (physical pixels, top-left origin) as an `AppKit` rectangle in
/// points: `(x, y, w, h)` in a view `view_height` points tall.
fn view_rect(anchor: Anchor, scale: f64, view_height: f64, flipped: bool) -> (f64, f64, f64, f64) {
    let (x, y) = (f64::from(anchor.x) / scale, f64::from(anchor.y) / scale);
    let (w, h) = (f64::from(anchor.w) / scale, f64::from(anchor.h) / scale);
    (x, if flipped { y } else { view_height - y - h }, w, h)
}

/// Where a popover fits best around a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Below,
    Above,
    Left,
    Right,
}

/// The side to open the popover on: away from the view edge the button is
/// nearest to, since that is where the Toolbar sits and the rest of it lies
/// along that edge. All in points, top-left origin; ties prefer the top edge,
/// then bottom, left, right.
fn roomiest_side(anchor: Anchor, scale: f64, view_w: f64, view_h: f64) -> Side {
    let (x, y, w, h) = view_rect(anchor, scale, view_h, true);
    let gaps = [
        (Side::Below, y),
        (Side::Above, view_h - (y + h)),
        (Side::Right, x),
        (Side::Left, view_w - (x + w)),
    ];
    let mut best = gaps[0];
    for gap in gaps {
        if gap.1 < best.1 {
            best = gap;
        }
    }
    best.0
}

fn edge_of(side: Side, flipped: bool) -> NSRectEdge {
    match (side, flipped) {
        (Side::Below, false) | (Side::Above, true) => NSRectEdge::MinY,
        (Side::Above, false) | (Side::Below, true) => NSRectEdge::MaxY,
        (Side::Left, _) => NSRectEdge::MinX,
        (Side::Right, _) => NSRectEdge::MaxX,
    }
}

// ---- The target of every control -----------------------------------------

#[derive(Default)]
struct Ivars {
    slot: Cell<usize>,
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
    struct Target;

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
            let Some(rgb) = Palette::DEFAULT.colors.get(slot).copied() else { return };
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
            let default = Palette::DEFAULT.widths.get(self.ivars().slot.get()).copied();
            if let Some(width) = default {
                self.set_width(f64::from(width));
            }
        }

        // The color panel's delegate call and the popover's: either closing
        // ends the edit.
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _note: &NSNotification) {
            emit(EditorEvent::Closed);
        }

        #[unsafe(method(popoverDidClose:))]
        fn popover_did_close(&self, _note: &NSNotification) {
            emit(EditorEvent::Closed);
        }
    }
);

impl Target {
    fn new(mtm: MainThreadMarker, slot: usize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars {
            slot: Cell::new(slot),
            ..Ivars::default()
        });
        // SAFETY: plain NSObject init on the main thread.
        unsafe { msg_send![super(this), init] }
    }

    /// Shows `value` in both controls and reports it.
    fn set_width(&self, value: f64) {
        let Some(width) = width_of(value) else { return };
        let ivars = self.ivars();
        if let Some(slider) = ivars.slider.get() {
            slider.setDoubleValue(f64::from(width));
        }
        if let Some(field) = ivars.field.get() {
            field.setStringValue(&NSString::from_str(&width.to_string()));
        }
        emit(EditorEvent::Width {
            slot: ivars.slot.get(),
            width,
        });
    }
}

fn color_of(rgb: [u8; 3]) -> Retained<NSColor> {
    let c = |b: u8| f64::from(b) / 255.0;
    NSColor::colorWithSRGBRed_green_blue_alpha(c(rgb[0]), c(rgb[1]), c(rgb[2]), 1.0)
}

fn srgb_of(color: &NSColor) -> Option<[u8; 3]> {
    let color = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some(rgb_of(
        color.redComponent(),
        color.greenComponent(),
        color.blueComponent(),
    ))
}

fn frame(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

fn reset_button(target: &Target, action: Sel, mtm: MainThreadMarker) -> Retained<NSButton> {
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

// ---- The open editor ------------------------------------------------------

enum Open {
    Color {
        /// Kept alive: the panel's target and delegate are weak references.
        _target: Retained<Target>,
        /// The shared panel's level before we raised it above the Overlay.
        level: NSWindowLevel,
    },
    Width {
        _target: Retained<Target>,
        popover: Retained<NSPopover>,
    },
}

thread_local! {
    /// The editor on screen, if any. At most one: opening another closes it.
    /// `AppKit` is main-thread only, so a thread-local is the whole story.
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
}

/// Opens the editor for `request` next to its button on the Overlay `owner`.
/// Not modal: returns at once, and later edits arrive as `EditorEvent`s. An
/// editor already open is closed first, which ends its edit through `send`.
pub fn open_editor(request: EditRequest, owner: RawWindowHandle, send: &mut dyn FnMut(Event)) {
    close_editor(send);
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = owner else {
        return;
    };
    // SAFETY: the handle comes from the live Overlay window on the main
    // thread and `ns_view` is its NSView.
    let Some(view) = (unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }) else {
        return;
    };
    let scale = view
        .window()
        .map_or(1.0, |window| window.backingScaleFactor());
    let open = match request.value {
        SlotValue::Color(rgb) => open_color(mtm, &view, request.index, rgb),
        SlotValue::Width(width) => {
            open_width(mtm, &view, scale, request.index, width, request.anchor)
        }
    };
    OPEN.with_borrow_mut(|slot| *slot = Some(open));
}

/// Closes the editor, if one is open, and ends its edit through `send`.
pub fn close_editor(send: &mut dyn FnMut(Event)) {
    let Some(open) = OPEN.with_borrow_mut(Option::take) else {
        return;
    };
    match open {
        Open::Color { level, .. } => {
            if let Some(mtm) = MainThreadMarker::new() {
                let panel = NSColorPanel::sharedColorPanel(mtm);
                detach(&panel);
                // SAFETY: clearing the target and action is always valid.
                unsafe {
                    panel.setTarget(None);
                    panel.setAction(None);
                }
                panel.setAccessoryView(None);
                panel.setLevel(level);
                panel.orderOut(None);
            }
        }
        Open::Width { popover, .. } => {
            detach(&popover);
            popover.close();
        }
    }
    send(Event::EditEnd);
}

/// Stops `object` telling its (about to die) delegate anything.
fn detach(object: &AnyObject) {
    // SAFETY: both NSColorPanel (NSWindow) and NSPopover have `setDelegate:`
    // taking an object or nil.
    unsafe {
        let _: () = msg_send![object, setDelegate: std::ptr::null::<AnyObject>()];
    }
}

fn open_color(mtm: MainThreadMarker, view: &NSView, slot: usize, rgb: [u8; 3]) -> Open {
    let target = Target::new(mtm, slot);
    let panel = NSColorPanel::sharedColorPanel(mtm);
    let level = panel.level();
    // Above the Overlay (always on top) so it is not hidden behind the Ink.
    if let Some(overlay) = view.window() {
        panel.setLevel(overlay.level() + 1);
    }
    panel.setShowsAlpha(false);
    panel.setContinuous(true);
    panel.setHidesOnDeactivate(false);
    panel.setColor(&color_of(rgb));
    // SAFETY: the target implements `colorChanged:`; both references are weak
    // and `Open::Color` keeps the target alive until `close` clears them.
    unsafe {
        panel.setTarget(Some(&target));
        panel.setAction(Some(sel!(colorChanged:)));
    }

    let accessory = NSView::initWithFrame(NSView::alloc(mtm), frame(0.0, 0.0, 120.0, 36.0));
    let reset = reset_button(&target, sel!(resetColor:), mtm);
    reset.setFrame(frame(10.0, 6.0, 80.0, 24.0));
    accessory.addSubview(&reset);
    panel.setAccessoryView(Some(&accessory));

    // SAFETY: NSWindow's delegate is weak; the target outlives it (see above)
    // and implements `windowWillClose:`.
    unsafe {
        let _: () = msg_send![&*panel, setDelegate: &*target];
    }
    // Not key: the Overlay keeps the keyboard for Tool keys and undo.
    panel.orderFront(None);
    Open::Color {
        _target: target,
        level,
    }
}

fn open_width(
    mtm: MainThreadMarker,
    view: &NSView,
    scale: f64,
    slot: usize,
    width: f32,
    anchor: Anchor,
) -> Open {
    let target = Target::new(mtm, slot);
    let content = NSView::initWithFrame(NSView::alloc(mtm), frame(0.0, 0.0, 250.0, 76.0));

    let (min, max) = (f64::from(MIN_WIDTH), f64::from(MAX_WIDTH));
    // SAFETY: the target implements `widthSlider:`.
    let slider = unsafe {
        NSSlider::sliderWithValue_minValue_maxValue_target_action(
            f64::from(width),
            min,
            max,
            Some(&target),
            Some(sel!(widthSlider:)),
            mtm,
        )
    };
    slider.setFrame(frame(12.0, 42.0, 160.0, 20.0));
    content.addSubview(&slider);

    let field = NSTextField::textFieldWithString(&NSString::from_str(&width.to_string()), mtm);
    field.setFrame(frame(182.0, 40.0, 56.0, 22.0));
    // SAFETY: the target implements `widthField:`.
    unsafe {
        field.setTarget(Some(&target));
        field.setAction(Some(sel!(widthField:)));
    }
    content.addSubview(&field);

    let reset = reset_button(&target, sel!(resetWidth:), mtm);
    reset.setFrame(frame(12.0, 10.0, 80.0, 24.0));
    content.addSubview(&reset);
    let _ = target.ivars().slider.set(slider);
    let _ = target.ivars().field.set(field);

    let controller = NSViewController::new(mtm);
    controller.setView(&content);
    let popover = NSPopover::new(mtm);
    popover.setBehavior(NSPopoverBehavior::Transient);
    popover.setContentViewController(Some(&controller));
    // SAFETY: NSPopover's delegate is weak; `Open::Width` keeps the target
    // alive, `close` clears the delegate first, and the target implements
    // `popoverDidClose:`.
    unsafe {
        let _: () = msg_send![&*popover, setDelegate: &*target];
    }

    let flipped = view.isFlipped();
    let bounds = view.bounds();
    let (x, y, w, h) = view_rect(anchor, scale, bounds.size.height, flipped);
    let side = roomiest_side(anchor, scale, bounds.size.width, bounds.size.height);
    popover.showRelativeToRect_ofView_preferredEdge(
        frame(x, y, w, h),
        view,
        edge_of(side, flipped),
    );
    Open::Width {
        _target: target,
        popover,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUTTON: Anchor = Anchor {
        x: 400.0,
        y: 20.0,
        w: 60.0,
        h: 40.0,
    };

    #[test]
    fn components_round_to_the_nearest_byte() {
        assert_eq!(rgb_of(0.0, 1.0, 0.5), [0, 255, 128]);
        assert_eq!(rgb_of(0.8784, 0.1922, 0.1922), [0xe0, 0x31, 0x31]);
    }

    #[test]
    fn components_outside_the_range_are_clamped() {
        assert_eq!(rgb_of(-0.2, 1.4, f64::NAN), [0, 255, 0]);
    }

    #[test]
    fn widths_snap_to_half_steps_inside_the_range() {
        assert_eq!(width_of(2.26), Some(2.5));
        assert_eq!(width_of(0.0), Some(0.5));
        assert_eq!(width_of(99.0), Some(20.0));
        assert_eq!(width_of(f64::NAN), None);
    }

    #[test]
    fn an_anchor_is_converted_to_points_with_the_y_axis_of_the_view() {
        // Scale 2: the anchor origin (400, 20) px is (200, 10) points.
        assert_eq!(
            view_rect(BUTTON, 2.0, 600.0, true),
            (200.0, 10.0, 30.0, 20.0)
        );
        // Bottom-left origin in a 600 pt tall view: 600 - 10 - 20.
        assert_eq!(
            view_rect(BUTTON, 2.0, 600.0, false),
            (200.0, 570.0, 30.0, 20.0)
        );
    }

    #[test]
    fn the_popover_opens_away_from_the_nearest_edge() {
        // A Toolbar at the top: below. At the bottom: above.
        assert_eq!(roomiest_side(BUTTON, 2.0, 1000.0, 600.0), Side::Below);
        let bottom = Anchor {
            y: 1100.0,
            ..BUTTON
        };
        assert_eq!(roomiest_side(bottom, 2.0, 1000.0, 600.0), Side::Above);
        // A column on the left: to its right; on the right: to its left.
        let left = Anchor {
            x: 10.0,
            y: 500.0,
            w: 40.0,
            h: 40.0,
        };
        assert_eq!(roomiest_side(left, 2.0, 1000.0, 600.0), Side::Right);
        let right = Anchor { x: 1950.0, ..left };
        assert_eq!(roomiest_side(right, 2.0, 1000.0, 600.0), Side::Left);
    }

    #[test]
    fn a_side_is_the_opposite_edge_in_a_flipped_view() {
        assert_eq!(edge_of(Side::Below, false), NSRectEdge::MinY);
        assert_eq!(edge_of(Side::Below, true), NSRectEdge::MaxY);
        assert_eq!(edge_of(Side::Right, true), NSRectEdge::MaxX);
    }
}
