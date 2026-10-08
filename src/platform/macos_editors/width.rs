//! A popover with a slider, a number field and Reset as the editor of a
//! width slot.

use super::target::{frame, reset_button, Target};
use markuli_core::{Anchor, Palette, Side, MAX_WIDTH, MIN_WIDTH};
use objc2::rc::Retained;
use objc2::{msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSPopover, NSPopoverBehavior, NSSlider, NSTextField, NSView, NSViewController,
};
use objc2_foundation::{NSRectEdge, NSString};

/// The Anchor (physical pixels, top-left origin) as an `AppKit` rectangle in
/// points: `(x, y, w, h)` in a view `view_height` points tall.
fn view_rect(anchor: Anchor, scale: f64, view_height: f64, flipped: bool) -> (f64, f64, f64, f64) {
    let (x, y) = (f64::from(anchor.x) / scale, f64::from(anchor.y) / scale);
    let (w, h) = (f64::from(anchor.w) / scale, f64::from(anchor.h) / scale);
    (x, if flipped { y } else { view_height - y - h }, w, h)
}

/// `AppKit`'s edge for a side; a flipped view has its y axis the other way.
fn edge_of(side: Side, flipped: bool) -> NSRectEdge {
    match (side, flipped) {
        (Side::Below, false) | (Side::Above, true) => NSRectEdge::MinY,
        (Side::Above, false) | (Side::Below, true) => NSRectEdge::MaxY,
        (Side::Left, _) => NSRectEdge::MinX,
        (Side::Right, _) => NSRectEdge::MaxX,
    }
}

/// The open popover, kept for `close`.
pub(super) struct Popover {
    /// Kept alive: the popover's delegate and the controls' targets are weak.
    _target: Retained<Target>,
    popover: Retained<NSPopover>,
}

fn controls(
    mtm: MainThreadMarker,
    target: &Target,
    width: f32,
) -> (Retained<NSView>, Retained<NSSlider>, Retained<NSTextField>) {
    let content = NSView::initWithFrame(NSView::alloc(mtm), frame(0.0, 0.0, 250.0, 76.0));
    let (min, max) = (f64::from(MIN_WIDTH), f64::from(MAX_WIDTH));
    // SAFETY: the target implements `widthSlider:`.
    let slider = unsafe {
        NSSlider::sliderWithValue_minValue_maxValue_target_action(
            f64::from(width),
            min,
            max,
            Some(target),
            Some(sel!(widthSlider:)),
            mtm,
        )
    };
    slider.setFrame(frame(12.0, 42.0, 160.0, 20.0));
    content.addSubview(&slider);

    let text = NSString::from_str(&Palette::width_text(width));
    let field = NSTextField::textFieldWithString(&text, mtm);
    field.setFrame(frame(182.0, 40.0, 56.0, 22.0));
    // SAFETY: the target implements `widthField:`.
    unsafe {
        field.setTarget(Some(target));
        field.setAction(Some(sel!(widthField:)));
    }
    content.addSubview(&field);

    let reset = reset_button(target, sel!(resetWidth:), mtm);
    reset.setFrame(frame(12.0, 10.0, 80.0, 24.0));
    content.addSubview(&reset);
    (content, slider, field)
}

pub(super) fn open(
    mtm: MainThreadMarker,
    view: &NSView,
    scale: f64,
    (slot, generation): (usize, u64),
    width: f32,
    (anchor, side): (Anchor, Side),
) -> Popover {
    let target = Target::new(mtm, slot, generation);
    let (content, slider, field) = controls(mtm, &target, width);
    target.set_controls(slider, field);

    let controller = NSViewController::new(mtm);
    controller.setView(&content);
    let popover = NSPopover::new(mtm);
    popover.setBehavior(NSPopoverBehavior::Transient);
    popover.setContentViewController(Some(&controller));
    // SAFETY: NSPopover's delegate is weak; `Popover` keeps the target alive,
    // `close` clears the delegate first, and the target implements
    // `popoverDidClose:`.
    unsafe {
        let _: () = msg_send![&*popover, setDelegate: &*target];
    }

    let flipped = view.isFlipped();
    let (x, y, w, h) = view_rect(anchor, scale, view.bounds().size.height, flipped);
    popover.showRelativeToRect_ofView_preferredEdge(
        frame(x, y, w, h),
        view,
        edge_of(side, flipped),
    );
    Popover {
        _target: target,
        popover,
    }
}

pub(super) fn close(popover: &Popover) {
    super::detach(&popover.popover);
    popover.popover.close();
}
