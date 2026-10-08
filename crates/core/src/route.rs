//! Event routing: how each [`Event`] changes the Annotator's state, and the
//! gestures behind pointer, key and toolbar input.

use crate::panel::Press;
use crate::style;
use crate::style::Appearance;
use crate::tools::{Ctx, StyleTarget, Tool, ToolKind, Tools};
use crate::{
    excalidraw, Annotator, Button, Control, DisplayId, Element, Event, Ink, Key, Point, View,
};

impl Annotator {
    /// The single way into the core.
    pub fn handle(&mut self, event: Event) -> View {
        match event {
            Event::ToggleDrawMode(display) => self.toggle(display),
            Event::SurfaceReset => self.paint.full(),
            Event::Resize { width, height } => {
                self.toolbar.resize(width, height);
                self.panel.resize(width, height);
                self.paint.full();
            }
            Event::Insets { top } => {
                self.toolbar.set_top(top);
                self.panel.set_top(top);
                self.paint.full();
            }
            Event::Theme(theme) => {
                self.toolbar.theme = theme;
                self.panel.theme = theme;
                self.selection.set_theme(theme);
            }
            Event::Modifiers { shift } => self.shift = shift,
            Event::Pressure(p) => self.pressure = p.filter(|p| p.is_finite()),
            Event::ScaleFactor(scale) if scale.is_finite() && scale > 0.0 => {
                self.scale = scale;
                self.toolbar.set_scale(scale);
                self.panel.set_scale(scale);
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

    /// A style choice, from a key or a panel click. The active Tool's
    /// [`StyleTarget`] decides, never its kind: a Selection is restyled, the
    /// next Strokes take the Style, and a Tool with neither hands over to the
    /// Pen first.
    fn choose(&mut self, change: impl Fn(Appearance) -> Appearance) {
        let target = self.tools.get(self.tools.active()).map(Tool::styles);
        match target {
            Some(StyleTarget::Selection) if !self.selection.is_empty() => style::restyle(
                &mut self.ink,
                self.selection.ids(),
                change,
                &mut self.restyling,
                (&mut self.freehand, &mut self.paint, self.scale),
            ),
            Some(StyleTarget::NextStrokes) => self.style = change(self.style),
            _ => {
                self.switch_tool(|tools| tools.select_by_kind(ToolKind::Pen));
                self.style = change(self.style);
            }
        }
    }

    /// A key is a single action: the restyle is logged right away.
    fn choose_by_key(&mut self, change: impl Fn(Appearance) -> Appearance) {
        self.choose(change);
        self.finish_restyle();
    }

    /// The panel gesture ended: a restyle becomes one operation-log entry.
    fn finish_restyle(&mut self) {
        style::finish(self.restyling.take(), &self.ink, &mut self.history);
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
        self.panel.release();
    }

    fn pointer_down(&mut self, at: Point) {
        self.finish_gesture();
        if self.toolbar.press_at(at, self.tools.len()) {
            return;
        }
        if self.panel_view().is_some() {
            match self.panel.press_at(at) {
                Press::Miss => {}
                Press::Dead => return,
                Press::Control(control) => {
                    self.choose(|look| look.with(control));
                    return;
                }
            }
        }
        let (tool, mut ctx) = self.tool();
        tool.pointer_down(&mut ctx, at);
    }

    fn pointer_move(&mut self, at: Point) {
        self.toolbar.hover_at(at, self.tools.len());
        if self.panel.pressing() {
            if let Some(control) = self.panel.drag_to(at) {
                self.choose(|look| look.with(control));
            }
        } else if self.panel_view().is_some() {
            self.panel.hover_at(at);
        } else {
            self.panel.forget_pointer();
        }
        if !self.toolbar.pressing() && !self.panel.pressing() {
            let (tool, mut ctx) = self.tool();
            tool.pointer_move(&mut ctx, at);
        }
    }

    fn pointer_up(&mut self, at: Point) {
        if self.panel.pressing() {
            self.panel.release();
            self.finish_restyle();
        } else if self.toolbar.pressing() {
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
            Button::Undo => self.undo(),
            Button::Redo => self.redo(),
            Button::Clear => self.clear(),
        }
    }

    fn key(&mut self, key: Key, command: bool, shift: bool, alt: bool) {
        let (tool, mut ctx) = self.tool();
        if tool.key(&mut ctx, key, command, shift) {
            return;
        }
        match (key, command, shift) {
            // Undo, redo and switching wait for the Stroke to end: it is not
            // logged yet.
            (Key::Char(_), _, _) if self.tool_busy() || self.panel.pressing() => {}
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
            (Key::Char(c @ '1'..='5'), false, _) if !alt => {
                let index = usize::from(u8::try_from(c).unwrap_or(b'1') - b'1');
                self.choose_by_key(|look| look.with(Control::Color(index)));
            }
            (Key::Char('['), false, _) => self.choose_by_key(Appearance::thinner),
            (Key::Char(']'), false, _) => self.choose_by_key(Appearance::bolder),
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
        self.panel.forget_pointer();
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
