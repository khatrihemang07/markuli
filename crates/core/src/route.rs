//! Event routing: how each [`Event`] changes the Annotator's state, and the
//! gestures behind pointer, key and toolbar input.

use crate::palette::{Palette, COLORS, WIDTHS};
use crate::style;
use crate::style::Appearance;
use crate::tools::{Ctx, StyleTarget, Tool, ToolKind, Tools};
use crate::{
    excalidraw, Annotator, Button, Choice, DisplayId, EditRequest, Element, Event, Ink, Key, Point,
    SlotKind, SlotValue, View,
};

impl Annotator {
    /// The single way into the core.
    pub fn handle(&mut self, event: Event) -> View {
        self.edit = None;
        match event {
            Event::ToggleDrawMode(display) => self.toggle(display),
            Event::SurfaceReset => self.paint.full(),
            Event::Resize { width, height } => {
                self.toolbar.resize(width, height);
                self.paint.full();
            }
            Event::Insets(insets) => {
                self.toolbar.set_insets(insets);
                self.paint.full();
            }
            Event::ToolbarPosition(position) => {
                self.toolbar.set_position(position);
                self.paint.full();
            }
            Event::Style(style) if style.is_valid() => {
                self.slots = style;
                self.resolve_style();
            }
            Event::Palette(palette) => {
                self.palette = palette.sanitized();
                self.resolve_style();
            }
            Event::EditColor { slot, rgb } if slot < COLORS => {
                self.palette.colors[slot] = rgb;
                self.edit_slot(SlotKind::Color, slot, Choice::Color(slot));
            }
            Event::EditWidth { slot, width } if slot < WIDTHS => {
                if let Some(width) = Palette::snap_width(width) {
                    self.palette.widths[slot] = width;
                    self.edit_slot(SlotKind::Width, slot, Choice::Width(slot));
                }
            }
            Event::EditEnd => self.finish_restyle(),
            Event::SecondaryClick(at) if self.draw_mode => self.secondary_click(at),
            Event::Theme(theme) => {
                self.toolbar.theme = theme;
                self.selection.set_theme(theme);
            }
            Event::Modifiers { shift } => self.shift = shift,
            Event::Pressure(p) => self.pressure = p.filter(|p| p.is_finite()),
            Event::ScaleFactor(scale) if scale.is_finite() && scale > 0.0 => {
                self.scale = scale;
                self.toolbar.set_scale(scale);
                self.paint.full();
            }
            Event::PointerDown(at) if self.draw_mode => self.pointer_down(at),
            Event::PointerMove(at) if self.draw_mode => self.pointer_move(at),
            Event::PointerUp(at) if self.draw_mode => self.pointer_up(at),
            Event::Key {
                key,
                command,
                shift,
                alt,
            } if self.draw_mode => self.key(key, command, shift, alt),
            Event::Clear => self.clear(),
            Event::Clock(ms) => self.laser.set_now(ms, &mut self.paint, self.scale),
            _ => {}
        }
        self.selection.flush(&self.ink, &mut self.paint, self.scale);
        self.view()
    }

    /// A style choice, from a key, a Toolbar click or a Palette edit. The
    /// active Tool's [`StyleTarget`] decides, never its kind: a Selection is
    /// restyled, the next Strokes take the Style, and a Tool with neither
    /// hands over to the Pen first.
    fn choose(&mut self, choice: Choice) {
        let target = self.tools.get(self.tools.active()).map(Tool::styles);
        let palette = self.palette;
        match target {
            Some(StyleTarget::Selection) if !self.selection.is_empty() => style::restyle(
                &mut self.ink,
                self.selection.ids(),
                |appearance| appearance.with(choice, &palette),
                &mut self.restyling,
                (&mut self.freehand, &mut self.paint, self.scale),
            ),
            Some(StyleTarget::NextStrokes) => self.choose_for_pen(choice),
            _ => {
                self.switch_tool(|tools| tools.select_by_kind(ToolKind::Pen));
                self.choose_for_pen(choice);
            }
        }
    }

    fn choose_for_pen(&mut self, choice: Choice) {
        self.slots = choice.pinned(self.slots).slots(self.slots);
        self.resolve_style();
    }

    /// The Pen's look from its slots and the Palette.
    fn resolve_style(&mut self) {
        self.style = Appearance::of(self.slots, &self.palette);
    }

    /// A key or click is a single action: the restyle is logged right away.
    fn choose_by_key(&mut self, choice: Choice) {
        self.choose(choice);
        self.finish_restyle();
    }

    /// One edit of a Palette slot: it also chooses the slot. Edits of one
    /// slot in a row are one restyle until [`Self::finish_restyle`].
    fn edit_slot(&mut self, kind: SlotKind, index: usize, choice: Choice) {
        if self.editing != Some((kind, index)) {
            self.finish_restyle();
        }
        self.resolve_style();
        self.choose(choice);
        self.editing = Some((kind, index));
    }

    /// A choice is a single action: a restyle becomes one operation-log entry.
    fn finish_restyle(&mut self) {
        self.editing = None;
        style::finish(self.restyling.take(), &self.ink, &mut self.history);
    }

    /// A secondary click: on a color or width button, ask for its editor.
    fn secondary_click(&mut self, at: Point) {
        let Some((button, anchor)) = self.toolbar.palette_button_at(at, self.tools.len()) else {
            return;
        };
        let (kind, index, value) = match button {
            Button::Color(i) => (
                SlotKind::Color,
                i,
                self.palette.colors.get(i).copied().map(SlotValue::Color),
            ),
            Button::Width(i) => (
                SlotKind::Width,
                i,
                self.palette.widths.get(i).copied().map(SlotValue::Width),
            ),
            _ => return,
        };
        self.edit = value.map(|value| EditRequest {
            kind,
            index,
            anchor,
            value,
        });
    }

    /// The active Tool with what it may change.
    fn tool(&mut self) -> (&mut dyn Tool, Ctx<'_>) {
        let ctx = Ctx {
            ink: &mut self.ink,
            history: &mut self.history,
            paint: &mut self.paint,
            freehand: &mut self.freehand,
            selection: &mut self.selection,
            laser: &mut self.laser,
            next_id: &mut self.next_id,
            scale: self.scale,
            pressure: self.pressure,
            shift: self.shift,
            style: self.style,
        };
        (self.tools.active_mut(), ctx)
    }

    pub(super) fn tool_busy(&self) -> bool {
        self.tools.get(self.tools.active()).is_some_and(Tool::busy)
    }

    /// Commits the gesture in progress to the operation log.
    fn finish_gesture(&mut self) {
        let (tool, mut ctx) = self.tool();
        tool.finish(&mut ctx);
        self.finish_restyle();
    }

    fn pointer_down(&mut self, at: Point) {
        self.finish_gesture();
        if self.toolbar.press_at(at, self.tools.len()) {
            return;
        }
        let (tool, mut ctx) = self.tool();
        tool.pointer_down(&mut ctx, at);
    }

    fn pointer_move(&mut self, at: Point) {
        self.toolbar.hover_at(at, self.tools.len());
        if !self.toolbar.pressing() {
            let (tool, mut ctx) = self.tool();
            tool.pointer_move(&mut ctx, at);
        }
    }

    fn pointer_up(&mut self, at: Point) {
        if self.toolbar.pressing() {
            if let Some(button) = self.toolbar.release_at(at, self.tools.len()) {
                self.activate(button);
            }
        } else {
            self.toolbar.hover_at(at, self.tools.len());
            let (tool, mut ctx) = self.tool();
            tool.pointer_up(&mut ctx, at);
        }
    }

    /// A toolbar click does exactly what the matching key does.
    fn activate(&mut self, button: Button) {
        match button {
            Button::Tool(index) => self.switch_tool(|tools| tools.select(index)),
            Button::Color(i) => self.choose_by_key(Choice::Color(i)),
            Button::Width(i) => self.choose_by_key(Choice::Width(i)),
            Button::Undo => self.undo(),
            Button::Redo => self.redo(),
            Button::Clear => self.clear(),
        }
    }

    fn key(&mut self, key: Key, command: bool, shift: bool, alt: bool) {
        self.finish_restyle();
        let (tool, mut ctx) = self.tool();
        if tool.key(&mut ctx, key, command, shift) {
            return;
        }
        match (key, command, shift) {
            // Undo, redo and switching wait for the Stroke to end: it is not
            // logged yet.
            (Key::Char(_), _, _) if self.tool_busy() => {}
            // A style choice would change the Style under the press.
            (Key::Char('1'..='5' | '[' | ']'), false, _) if self.toolbar.pressing() => {}
            (Key::Char('z'), true, false) => self.undo(),
            (Key::Char('z'), true, true) | (Key::Char('y'), true, false) => self.redo(),
            (Key::Char('a'), true, false) => {
                // Like Excalidraw, Select All leaves the Pen for the Select Tool.
                self.switch_tool(|tools| {
                    tools.select_by_key('v');
                });
                self.selection.select_all(&self.ink);
            }
            (Key::Char('c'), true, false) => self.copy(),
            // Shift+digit stays ignored: on AZERTY the digits are shifted.
            (Key::Char(c @ '1'..='5'), false, false) if !alt => {
                let index = c
                    .to_digit(10)
                    .and_then(|d| usize::try_from(d.checked_sub(1)?).ok())
                    .unwrap_or(0);
                self.choose_by_key(Choice::Color(index));
            }
            (Key::Char('['), false, _) => self.choose_by_key(Choice::Thinner),
            (Key::Char(']'), false, _) => self.choose_by_key(Choice::Bolder),
            (Key::Char(c), false, false) if !alt => {
                self.switch_tool(|tools| tools.press(c));
            }
            _ => {}
        }
    }

    /// Changes the active Tool; the Selection belongs to the Select Tool and
    /// is dropped when another one takes over.
    fn switch_tool(&mut self, change: impl FnOnce(&mut Tools)) {
        let before = self.tools.active();
        change(&mut self.tools);
        if self.tools.active() != before {
            self.selection.clear();
        }
    }

    /// Cmd/Ctrl+C: the Selection, or all Ink when nothing is selected.
    fn copy(&mut self) {
        let chosen = |e: &&Element| self.selection.is_empty() || self.selection.contains(e.id());
        self.copied = excalidraw::clipboard(self.ink.elements().iter().filter(chosen));
    }

    fn undo(&mut self) {
        if self.history.undo(&mut self.ink, &mut self.freehand) {
            self.ink_replaced();
        }
    }

    fn redo(&mut self) {
        if self.history.redo(&mut self.ink, &mut self.freehand) {
            self.ink_replaced();
        }
    }

    /// Undo or redo changed the Ink under the Selection.
    fn ink_replaced(&mut self) {
        self.selection.retain_existing(&self.ink);
        self.selection.touch();
        self.paint.full();
    }

    fn clear(&mut self) {
        self.finish_gesture();
        if !self.ink.is_empty() {
            self.history.record_clear(self.ink.take());
        }
        // Draw Mode is untouched: Clear empties the Ink, nothing more.
        self.selection.clear();
        self.laser.clear();
        self.paint.full();
    }

    fn toggle(&mut self, display: DisplayId) {
        self.finish_gesture();
        self.toolbar.forget_pointer();
        self.selection.clear();
        if self.draw_mode {
            self.draw_mode = false;
            self.laser.clear();
        } else {
            if self.display != Some(display) {
                // ADR-0002: the history belongs to the Ink that was Cleared.
                self.ink = Ink::default();
                self.history.reset();
            }
            self.display = Some(display);
            self.draw_mode = true;
            self.tools.reset();
        }
        // Backdrop (hit-test layer) appears and disappears with Draw Mode.
        self.paint.full();
    }
}
