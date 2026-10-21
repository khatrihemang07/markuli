//! What the Color Picker and Width Picker mean for the Palette, the
//! Selection and Recent Colors: opening, the pointer gestures on the popover,
//! and closing. The picker itself only turns the pointer into an
//! [`Outcome`] (`picker/mod.rs`).

use crate::picker::{Frame, Look, Outcome};
use crate::style::Choice;
use crate::view::Chosen;
use crate::{Anchor, Annotator, Button, EditRequest, Palette, Point, SlotKind, SlotValue};

impl Annotator {
    /// The open picker's placement inputs, from the slot's current place.
    pub(crate) fn picker_frame(&self) -> Option<Frame> {
        let (kind, index) = self.picker.slot()?;
        let (anchor, side, usable) = self.toolbar.slot_geometry(kind, index, self.tools.len())?;
        Some(Frame {
            anchor,
            side,
            usable,
            scale: self.scale,
        })
    }

    /// How the open picker looks now.
    pub(crate) fn picker_look(&self) -> Option<Look> {
        let (kind, index) = self.picker.slot()?;
        let frame = self.picker_frame()?;
        let value = self.slot_value(kind, index)?;
        self.picker.look(
            &frame,
            self.toolbar.theme,
            value,
            self.style.color,
            self.recent,
        )
    }

    fn slot_value(&self, kind: SlotKind, index: usize) -> Option<SlotValue> {
        self.palette.get(kind, index)
    }

    /// Opens the picker of a slot, replacing an open one.
    fn open_picker(&mut self, kind: SlotKind, index: usize) {
        let Some(value) = self.slot_value(kind, index) else {
            return;
        };
        self.finish_restyle();
        self.picker.open(kind, index, value);
    }

    /// A click on the chosen slot opens its picker. A dimmed slot (Eraser,
    /// Laser) was only switched to the Pen by the click.
    pub(super) fn open_if_chosen(&mut self, button: Button, was: &Chosen) {
        if was.dimmed {
            return;
        }
        match button {
            Button::Color(i) if was.color == Some(i) => self.open_picker(SlotKind::Color, i),
            Button::Width(i) if was.width == Some(i) => self.open_picker(SlotKind::Width, i),
            _ => {}
        }
    }

    /// A secondary click: on a slot, its picker; elsewhere, closes the
    /// picker. It never starts anything while a gesture is in progress.
    pub(super) fn secondary_click(&mut self, at: Point) {
        if self.tool_busy() || self.toolbar.pressing() || self.picker.dragging() {
            return;
        }
        if let Some(frame) = self.picker_frame() {
            if self.picker.contains(at, &frame) {
                return;
            }
        }
        match self.toolbar.slot_at(at, self.tools.len()) {
            Some((kind, index)) => self.open_picker(kind, index),
            None => self.picker.close(),
        }
    }

    /// A press while the picker is open: on it, or the dismissing click.
    pub(super) fn picker_down(&mut self, at: Point) {
        let (Some(frame), Some((kind, index))) = (self.picker_frame(), self.picker.slot()) else {
            self.picker.close();
            return;
        };
        self.dismissing = true;
        let Some(value) = self.slot_value(kind, index) else {
            return;
        };
        if !self.picker.contains(at, &frame) {
            self.finish_restyle();
            self.picker.close();
            return;
        }
        match self.picker.press(at, &frame, value, &self.recent) {
            Outcome::Pick(rgb) => {
                self.set_slot(kind, index, SlotValue::Color(rgb));
                self.finish_restyle();
                self.recent.push(rgb);
                self.picker.close();
            }
            Outcome::Set(value) => {
                // A drag has started: its moves and release are the picker's.
                self.dismissing = false;
                self.set_slot(kind, index, value);
            }
            Outcome::Reset => {
                if let Some(default) = Palette::default_slot(kind, index) {
                    self.set_slot(kind, index, default);
                }
                self.finish_restyle();
                self.picker.close();
            }
            Outcome::More => {
                self.picker.close();
                self.edit = self.system_panel_request(index, &frame);
            }
            Outcome::Tab | Outcome::Inert => {}
        }
    }

    fn system_panel_request(&self, index: usize, frame: &Frame) -> Option<EditRequest> {
        let value = self.slot_value(SlotKind::Color, index)?;
        let a = frame.anchor;
        Some(EditRequest {
            kind: SlotKind::Color,
            index,
            anchor: Anchor {
                x: a.x,
                y: a.y,
                w: a.w,
                h: a.h,
            },
            side: frame.side,
            value,
        })
    }

    /// The slot takes `value` and is chosen, like any edit of it. A pick
    /// from the system color panel still waiting for its `EditEnd` is
    /// superseded: it must not join the Recent Colors after, say, a Reset.
    fn set_slot(&mut self, kind: SlotKind, index: usize, value: SlotValue) {
        if !self.palette.set(kind, index, value) {
            return;
        }
        if kind == SlotKind::Color && self.panel_run == Some(index) {
            self.panel_run = None;
        }
        let choice = match kind {
            SlotKind::Color => Choice::Color(index),
            SlotKind::Width => Choice::Width(index),
        };
        self.edit_slot(kind, index, choice);
    }

    pub(super) fn picker_drag(&mut self, at: Point) {
        let (Some(frame), Some((kind, index))) = (self.picker_frame(), self.picker.slot()) else {
            return;
        };
        if let Some(value) = self.picker.drag(at, &frame) {
            self.set_slot(kind, index, value);
        }
    }

    pub(super) fn picker_hover(&mut self, at: Point) {
        if let Some(frame) = self.picker_frame() {
            self.picker.hover(at, &frame, &self.recent);
        }
        self.toolbar.hover_at(at, self.tools.len());
    }

    /// The release that ends a drag: the pick is final and the picker closes.
    pub(super) fn picker_release(&mut self) {
        self.end_drag();
        self.picker.close();
    }

    /// The last color of a drag joins the Recent Colors; the restyle it made
    /// of a Selection is one undo step.
    fn end_drag(&mut self) {
        let slot = self.picker.slot();
        self.picker.release();
        self.finish_restyle();
        if let Some((kind @ SlotKind::Color, index)) = slot {
            if let Some(SlotValue::Color(rgb)) = self.palette.get(kind, index) {
                self.recent.push(rgb);
            }
        }
    }

    /// Esc with a picker open closes it and nothing else. In a drag it keeps
    /// the color dragged to; the mouse button is still down, so the release
    /// that follows is swallowed.
    pub(super) fn picker_escape(&mut self) {
        if self.picker.dragging() {
            self.end_drag();
            self.dismissing = true;
        }
        self.picker.close();
    }
}
