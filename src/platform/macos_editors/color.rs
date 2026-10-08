//! The shared color panel as the editor of a color slot.

use super::target::{color_of, frame, reset_button, Target};
use objc2::rc::Retained;
use objc2::{msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSColorPanel, NSView, NSWindowLevel};

/// What the open color panel needs to be put back as it was.
pub(super) struct Panel {
    /// Kept alive: the panel's target and delegate are weak references.
    _target: Retained<Target>,
    /// The shared panel's level before we raised it above the Overlay.
    level: NSWindowLevel,
}

pub(super) fn open(
    mtm: MainThreadMarker,
    view: &NSView,
    (slot, generation): (usize, u64),
    rgb: [u8; 3],
) -> Panel {
    let target = Target::new(mtm, slot, generation);
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
    // and `Panel` keeps the target alive until `close` clears them.
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
    Panel {
        _target: target,
        level,
    }
}

pub(super) fn close(panel: &Panel, mtm: MainThreadMarker) {
    let shared = NSColorPanel::sharedColorPanel(mtm);
    super::detach(&shared);
    // SAFETY: clearing the target and action is always valid.
    unsafe {
        shared.setTarget(None);
        shared.setAction(None);
    }
    shared.setAccessoryView(None);
    shared.setLevel(panel.level);
    shared.orderOut(None);
}
