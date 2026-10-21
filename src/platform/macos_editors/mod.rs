//! The macOS system color panel, opened by the Color Picker's "..." for exact
//! values (ADR-0004). It is not modal: `open_editor` returns at once and later
//! colors arrive as `EditorEvent`s. The core owns the Palette and the undo
//! step (`Event::EditColor`, `EditEnd`).

mod color;
mod target;

use markuli_core::{EditRequest, Event, SlotValue};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, MainThreadMarker};
use objc2_app_kit::NSView;
use std::cell::{Cell, RefCell};
use winit::raw_window_handle::RawWindowHandle;

/// The panel on screen, tagged with the generation it was opened under.
struct Open {
    generation: u64,
    panel: color::Panel,
}

thread_local! {
    /// The panel on screen, if any. At most one: opening another closes it.
    /// `AppKit` is main-thread only, so a thread-local is the whole story.
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
    /// Counts opened panels. A close notice from a panel that is no longer
    /// the current one (it arrives after a newer one opened) is stale.
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Opens the system color panel for `request` (a color slot) next to its
/// button on the Overlay `owner`. Not modal: returns at once, and later colors
/// arrive as `EditorEvent`s. The caller closes a panel already open first
/// (`close_editor`), so that its `EditEnd` reaches the core before the new
/// request does; one still open here is dropped without a word.
pub fn open_editor(request: EditRequest, owner: RawWindowHandle, send: &mut dyn FnMut(Event)) {
    close_silently();
    let (Some(mtm), SlotValue::Color(rgb)) = (MainThreadMarker::new(), request.value) else {
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
    let generation = GENERATION.get() + 1;
    GENERATION.set(generation);
    // The color panel is not dismissed by a click on the Overlay: the user
    // draws with it open. End the core's "editing" state now; edits still
    // coalesce until a click or key.
    send(Event::EditEnd);
    let panel = color::open(mtm, &view, (request.index, generation), rgb);
    OPEN.with_borrow_mut(|open| *open = Some(Open { generation, panel }));
}

/// Closes the panel, if one is open, and ends its edit through `send`.
pub fn close_editor(send: &mut dyn FnMut(Event)) {
    if close_silently() {
        send(Event::EditEnd);
    }
}

/// The panel reported that it closed: ends the edit, unless the report is
/// from a panel that is no longer the current one (a newer one opened before
/// the report was handled).
pub fn editor_closed(generation: u64, send: &mut dyn FnMut(Event)) {
    let current = OPEN.with_borrow(|open| open.as_ref().map(|open| open.generation));
    if current == Some(generation) {
        close_editor(send);
    }
}

/// Takes the open panel off screen; whether there was one.
fn close_silently() -> bool {
    let Some(open) = OPEN.with_borrow_mut(Option::take) else {
        return false;
    };
    if let Some(mtm) = MainThreadMarker::new() {
        color::close(&open.panel, mtm);
    }
    true
}

/// Stops `object` telling its (about to die) delegate anything.
fn detach(object: &AnyObject) {
    // SAFETY: NSColorPanel (an NSWindow) has `setDelegate:` taking an object
    // or nil.
    unsafe {
        let _: () = msg_send![object, setDelegate: std::ptr::null::<AnyObject>()];
    }
}
