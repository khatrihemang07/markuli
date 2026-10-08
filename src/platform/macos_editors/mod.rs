//! macOS Palette editors: the shared color panel for a color slot, a popover
//! with a slider for a width slot. They are not modal: `open_editor` returns
//! at once and later edits arrive as `EditorEvent`s. The core owns the Palette
//! and the undo step (`Event::EditColor`, `EditWidth`, `EditEnd`).

mod color;
mod target;
mod width;

use markuli_core::{EditRequest, Event, SlotValue};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, MainThreadMarker};
use objc2_app_kit::NSView;
use std::cell::{Cell, RefCell};
use winit::raw_window_handle::RawWindowHandle;

enum Kind {
    Color(color::Panel),
    Width(width::Popover),
}

/// The editor on screen, tagged with the generation it was opened under.
struct Open {
    generation: u64,
    kind: Kind,
}

thread_local! {
    /// The editor on screen, if any. At most one: opening another closes it.
    /// `AppKit` is main-thread only, so a thread-local is the whole story.
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
    /// Counts opened editors. A close notice from an editor that is no longer
    /// the current one (it arrives after a newer one opened) is stale.
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Opens the editor for `request` next to its button on the Overlay `owner`.
/// Not modal: returns at once, and later edits arrive as `EditorEvent`s. The
/// caller closes an editor already open first (`close_editor`), so that its
/// `EditEnd` reaches the core before the new request does; one still open
/// here is dropped without a word.
pub fn open_editor(request: EditRequest, owner: RawWindowHandle, send: &mut dyn FnMut(Event)) {
    close_silently();
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
    let generation = GENERATION.get() + 1;
    GENERATION.set(generation);
    let slot = (request.index, generation);
    let kind = match request.value {
        SlotValue::Color(rgb) => {
            // The color panel is not dismissed by a click on the canvas: the
            // user draws with it open. So the core must not treat the next
            // press as a dismissing one; end its "editing" state now. Edits
            // still coalesce until a click or key.
            send(Event::EditEnd);
            Kind::Color(color::open(mtm, &view, slot, rgb))
        }
        SlotValue::Width(w) => Kind::Width(width::open(
            mtm,
            &view,
            scale,
            slot,
            w,
            (request.anchor, request.side),
        )),
    };
    OPEN.with_borrow_mut(|open| *open = Some(Open { generation, kind }));
}

/// Closes the editor, if one is open, and ends its edit through `send`.
pub fn close_editor(send: &mut dyn FnMut(Event)) {
    if close_silently() {
        send(Event::EditEnd);
    }
}

/// An editor reported that it closed: ends the edit, unless the report is
/// from an editor that is no longer the current one (a newer one opened
/// before the report was handled).
pub fn editor_closed(generation: u64, send: &mut dyn FnMut(Event)) {
    let current = OPEN.with_borrow(|open| open.as_ref().map(|open| open.generation));
    if current == Some(generation) {
        close_editor(send);
    }
}

/// Takes the open editor off screen; whether there was one.
fn close_silently() -> bool {
    let Some(open) = OPEN.with_borrow_mut(Option::take) else {
        return false;
    };
    match &open.kind {
        Kind::Color(panel) => {
            if let Some(mtm) = MainThreadMarker::new() {
                color::close(panel, mtm);
            }
        }
        Kind::Width(popover) => width::close(popover),
    }
    true
}

/// Stops `object` telling its (about to die) delegate anything.
fn detach(object: &AnyObject) {
    // SAFETY: both NSColorPanel (NSWindow) and NSPopover have `setDelegate:`
    // taking an object or nil.
    unsafe {
        let _: () = msg_send![object, setDelegate: std::ptr::null::<AnyObject>()];
    }
}
