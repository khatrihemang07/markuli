//! Markuli annotator core: pure, platform-free.
//!
//! One way in: [`Annotator::handle`] takes an [`Event`]. Out come the
//! [`View`] state, the [`Ink`], and pixels via [`Annotator::render`].

mod config;
pub mod freehand;
mod history;
mod icons;
mod ink;
mod render;
mod svg_path;
mod toolbar;
mod tools;

pub use config::Config;
use freehand::Scratch;
use history::History;
pub use ink::{Element, Ink, Point};
pub use render::{Damage, Format};
pub use toolbar::{Button, Theme};
use toolbar::{Chrome, Toolbar, UiState};
pub use tools::Cursor;
use tools::{Ctx, Tool, Tools};

/// Identifies a display. Opaque to the core: it only compares them, so that
/// moving the Overlay to another display can Clear the Ink (ADR-0002).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayId(pub u64);

/// Everything the platform layer can tell the core.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// The toggle hotkey was pressed while the cursor was on this display.
    ToggleDrawMode(DisplayId),
    /// The platform's pixel buffer is new or was wiped: redraw everything.
    SurfaceReset,
    /// The Overlay's size in physical pixels, for laying out the toolbar.
    /// Send it when the Overlay is created.
    Resize {
        width: u32,
        height: u32,
    },
    /// The OS light or dark theme.
    Theme(Theme),
    /// Physical pixels.
    PointerDown(Point),
    PointerMove(Point),
    PointerUp(Point),
    /// Pressure (0..=1) of the pointer events that follow, until changed.
    /// `None` is a mouse or trackpad: the Stroke simulates pressure.
    Pressure(Option<f32>),
    /// Physical pixels per logical pixel of the Overlay (default 1).
    ScaleFactor(f32),
    /// A key press in Draw Mode. The platform layer resolves `command` to
    /// Cmd on macOS and Ctrl on Windows.
    Key {
        key: Key,
        command: bool,
        shift: bool,
    },
    /// The Clear hotkey: removes all Ink and leaves Draw Mode.
    Clear,
}

/// The keys the core reacts to; the platform layer drops the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    /// A character key, lowercase.
    Char(char),
}

/// What the platform layer needs to know after each event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub draw_mode: bool,
    /// An Overlay window must exist (Draw Mode is on, or Ink is visible).
    pub overlay_needed: bool,
    /// The display the Overlay belongs to, once Draw Mode was first entered.
    pub display: Option<DisplayId>,
    /// `render` has pixels to produce.
    pub needs_render: bool,
    /// The cursor shape for the pointer's current position.
    pub cursor: Cursor,
}

impl View {
    /// Outside Draw Mode, clicks and scrolls reach the apps underneath.
    #[must_use]
    pub fn click_through(&self) -> bool {
        !self.draw_mode
    }
}

#[derive(Debug)]
pub struct Annotator {
    ink: Ink,
    draw_mode: bool,
    display: Option<DisplayId>,
    history: History,
    paint: render::Pending,
    freehand: Scratch,
    scale: f32,
    pressure: Option<f32>,
    next_id: u64,
    tools: Tools,
    toolbar: Toolbar,
}

impl Default for Annotator {
    fn default() -> Self {
        Self {
            ink: Ink::default(),
            draw_mode: false,
            display: None,
            history: History::default(),
            paint: render::Pending::default(),
            freehand: Scratch::default(),
            scale: 1.0,
            pressure: None,
            next_id: 1,
            tools: Tools::new(),
            toolbar: Toolbar::default(),
        }
    }
}

impl Annotator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The single way into the core.
    pub fn handle(&mut self, event: Event) -> View {
        match event {
            Event::ToggleDrawMode(display) => self.toggle(display),
            Event::SurfaceReset => self.paint.full(),
            Event::Resize { width, height } => {
                self.toolbar.resize(width, height);
                self.paint.full();
            }
            Event::Theme(theme) => self.toolbar.theme = theme,
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
            } if self.draw_mode => self.key(key, command, shift),
            Event::Clear => self.clear(),
            _ => {}
        }
        self.view()
    }

    #[must_use]
    pub fn view(&self) -> View {
        let over_toolbar = self.toolbar.over() && !self.tool_busy();
        View {
            draw_mode: self.draw_mode,
            overlay_needed: self.draw_mode || !self.ink.is_empty(),
            display: self.display,
            needs_render: self.paint.is_pending()
                || self.toolbar.is_dirty(self.draw_mode, self.ui()),
            cursor: match self.tools.get(self.tools.active()) {
                Some(tool) if !over_toolbar => tool.cursor(),
                _ => Cursor::Arrow,
            },
        }
    }

    #[must_use]
    pub fn ink(&self) -> &Ink {
        &self.ink
    }

    /// The index of the active Tool, in registration order.
    #[must_use]
    pub fn active_tool(&self) -> usize {
        self.tools.active()
    }

    /// The centre of a toolbar button in Overlay pixels; `None` while the
    /// toolbar is hidden (outside Draw Mode, or before the first `Resize`).
    #[must_use]
    pub fn button_center(&self, button: Button) -> Option<Point> {
        if !self.draw_mode {
            return None;
        }
        self.toolbar.center_of(button, self.tools.len())
    }

    /// Draws what changed since the last call into `target` (premultiplied
    /// pixels, `format` channel order) and returns the changed region.
    pub fn render(
        &mut self,
        target: &mut tiny_skia::PixmapMut<'_>,
        format: Format,
    ) -> Option<Damage> {
        let chrome = Chrome {
            visible: self.draw_mode,
            ui: self.ui(),
            toolbar: &mut self.toolbar,
            tools: &self.tools,
        };
        render::render(
            &self.ink,
            &mut self.paint,
            self.draw_mode,
            self.scale,
            target,
            format,
            chrome,
        )
    }

    fn ui(&self) -> UiState {
        UiState {
            active: self.tools.active(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            has_ink: !self.ink.is_empty(),
        }
    }

    /// The active Tool with what it may change.
    fn tool(&mut self) -> (&mut dyn Tool, Ctx<'_>) {
        let ctx = Ctx {
            ink: &mut self.ink,
            history: &mut self.history,
            paint: &mut self.paint,
            freehand: &mut self.freehand,
            next_id: &mut self.next_id,
            scale: self.scale,
            pressure: self.pressure,
        };
        (self.tools.active_mut(), ctx)
    }

    fn tool_busy(&self) -> bool {
        self.tools.get(self.tools.active()).is_some_and(Tool::busy)
    }

    /// Commits the gesture in progress to the operation log.
    fn finish_gesture(&mut self) {
        let (tool, mut ctx) = self.tool();
        tool.finish(&mut ctx);
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
            Button::Tool(index) => self.tools.select(index),
            Button::Undo => self.undo(),
            Button::Redo => self.redo(),
            Button::Clear => self.clear(),
        }
    }

    fn key(&mut self, key: Key, command: bool, shift: bool) {
        let (tool, mut ctx) = self.tool();
        if tool.key(&mut ctx, key, command, shift) {
            return;
        }
        match (key, command, shift) {
            // Undo, redo and switching wait for the Stroke to end: it is not
            // logged yet.
            (Key::Char(_), _, _) if self.tool_busy() => {}
            (Key::Char('z'), true, false) => self.undo(),
            (Key::Char('z'), true, true) | (Key::Char('y'), true, false) => self.redo(),
            (Key::Char(c), false, false) => {
                self.tools.select_by_key(c);
            }
            _ => {}
        }
    }

    fn undo(&mut self) {
        if self.history.undo(&mut self.ink) {
            self.paint.full();
        }
    }

    fn redo(&mut self) {
        if self.history.redo(&mut self.ink) {
            self.paint.full();
        }
    }

    fn clear(&mut self) {
        self.finish_gesture();
        if !self.ink.is_empty() {
            self.history.record_clear(self.ink.take());
        }
        self.draw_mode = false;
        self.toolbar.forget_pointer();
        self.paint.full();
    }

    fn toggle(&mut self, display: DisplayId) {
        self.finish_gesture();
        self.toolbar.forget_pointer();
        if self.draw_mode {
            self.draw_mode = false;
        } else {
            if self.display != Some(display) {
                // ADR-0002: the history belongs to the Ink that was Cleared.
                self.ink = Ink::default();
                self.history.reset();
            }
            self.display = Some(display);
            self.draw_mode = true;
        }
        // Backdrop (hit-test layer) appears and disappears with Draw Mode.
        self.paint.full();
    }
}
