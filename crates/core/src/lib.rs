//! Markuli annotator core: pure, platform-free.
//!
//! One way in: [`Annotator::handle`] takes an [`Event`]. Out come the
//! [`View`] state, the [`Ink`], and pixels via [`Annotator::render`].

mod config;
mod excalidraw;
pub mod freehand;
mod history;
mod icons;
mod ink;
pub mod laser;
mod panel;
mod render;
mod selection;
mod style;
mod svg_path;
mod toolbar;
mod tools;

pub use config::Config;
use freehand::Scratch;
use history::History;
pub use ink::{Element, Ink, Point};
use laser::Laser;
use panel::{Panel, PanelView, Press};
pub use render::{Damage, Format};
use selection::Selection;
pub use style::Control;
use style::Style;
pub use toolbar::{Button, Theme};
use toolbar::{Chrome, Toolbar, UiState};
use tools::{Ctx, StyleTarget, Tool, Tools};
pub use tools::{Cursor, ToolKind};

/// Identifies a display. Opaque to the core: it only compares them, so that
/// moving the Overlay to another display can Clear the Ink (ADR-0002).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayId(u64);

impl DisplayId {
    /// The platform layer derives the value from whatever makes a display
    /// the same display (position, size, scale).
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

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
    /// Whether Shift is held, for the pointer events that follow.
    Modifiers {
        shift: bool,
    },
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
    /// The time, in milliseconds from any fixed origin. Send it before
    /// pointer events and whenever [`View::next_frame`] is due. It is how the
    /// Laser fades without the core owning a clock.
    Clock(u64),
}

/// The keys the core reacts to; the platform layer drops the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    /// Delete or Backspace.
    Delete,
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
    /// The active Tool.
    pub tool: ToolKind,
    /// The time (same clock as [`Event::Clock`]) the next frame is due. Set
    /// only while something animates (a visible Laser trail): wait for it,
    /// then send `Clock`. `None` means sleep until the next input.
    pub next_frame: Option<u64>,
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
    laser: Laser,
    scale: f32,
    pressure: Option<f32>,
    shift: bool,
    selection: Selection,
    copied: Option<String>,
    next_id: u64,
    tools: Tools,
    toolbar: Toolbar,
    /// The style of the next Strokes, set by the style panel.
    style: Style,
    panel: Panel,
    /// Style of the selected Elements before the panel gesture in progress.
    restyling: Option<Vec<(usize, Style)>>,
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
            laser: Laser::default(),
            scale: 1.0,
            pressure: None,
            shift: false,
            selection: Selection::default(),
            copied: None,
            next_id: 1,
            tools: Tools::new(),
            toolbar: Toolbar::default(),
            style: Style::default(),
            panel: Panel::default(),
            restyling: None,
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
                self.panel.resize(width, height);
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
            } if self.draw_mode => self.key(key, command, shift),
            Event::Clear => self.clear(),
            Event::Clock(ms) => self.laser.set_now(ms, &mut self.paint, self.scale),
            _ => {}
        }
        self.selection.flush(&self.ink, &mut self.paint, self.scale);
        self.view()
    }

    #[must_use]
    pub fn view(&self) -> View {
        let over_toolbar = (self.toolbar.over() || self.panel.over()) && !self.tool_busy();
        View {
            draw_mode: self.draw_mode,
            overlay_needed: self.draw_mode || !self.ink.is_empty(),
            display: self.display,
            needs_render: self.paint.is_pending()
                || self.toolbar.is_dirty(self.draw_mode, self.ui())
                || self.panel.is_dirty(self.panel_view()),
            cursor: match self.tools.get(self.tools.active()) {
                Some(tool) if !over_toolbar => tool.cursor(),
                _ => Cursor::Arrow,
            },
            tool: self.tools.active_kind(),
            next_frame: self.laser.next_frame(),
        }
    }

    #[must_use]
    pub fn ink(&self) -> &Ink {
        &self.ink
    }

    /// The ids ([`Element::id`]) of the selected Elements, in the order they
    /// were selected.
    #[must_use]
    pub fn selection(&self) -> &[u64] {
        self.selection.ids()
    }

    /// The Excalidraw clipboard JSON of the last copy (Cmd/Ctrl+C), once.
    /// The platform layer calls this after each key event and writes the text
    /// to the OS clipboard.
    pub fn take_copy(&mut self) -> Option<String> {
        self.copied.take()
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

    /// The centre of a style panel control in Overlay pixels (for
    /// `Opacity`, the thumb at that value); `None` while the panel is hidden.
    /// It shows for the Pen and, with a Selection, for the Select Tool.
    #[must_use]
    pub fn panel_center(&self, control: Control) -> Option<Point> {
        self.panel_view().map(|_| self.panel.center_of(control))
    }

    /// Draws what changed since the last call into `target` (premultiplied
    /// pixels, `format` channel order) and returns the changed region.
    pub fn render(
        &mut self,
        target: &mut tiny_skia::PixmapMut<'_>,
        format: Format,
    ) -> Option<Damage> {
        let panel_view = self.panel_view();
        let chrome = Chrome {
            visible: self.draw_mode,
            ui: self.ui(),
            toolbar: &mut self.toolbar,
            tools: &self.tools,
            panel_view,
            panel: &mut self.panel,
        };
        render::render(
            &self.ink,
            &self.laser,
            &self.selection,
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

    /// What the style panel shows, or `None` while it is hidden: in Draw Mode,
    /// for the Pen (the style of the next Strokes) and for the Select Tool
    /// once something is selected (the Selection's style).
    fn panel_view(&self) -> Option<PanelView> {
        if !self.draw_mode || !self.panel.fits() {
            return None;
        }
        let target = self.tools.get(self.tools.active()).map(Tool::styles);
        match target {
            Some(StyleTarget::NextStrokes) => Some(PanelView {
                color: Some(self.style.color),
                width: Some(self.style.width),
                opacity: self.style.opacity,
            }),
            Some(StyleTarget::Selection) if !self.selection.is_empty() => {
                let chosen = self
                    .ink
                    .elements()
                    .iter()
                    .filter(|e| self.selection.contains(e.id()));
                chosen.map(Element::style).fold(None, |view, s| {
                    Some(match view {
                        None => PanelView {
                            color: Some(s.color),
                            width: Some(s.width),
                            opacity: s.opacity,
                        },
                        Some(v) => PanelView {
                            color: v.color.filter(|&c| c == s.color),
                            #[allow(clippy::float_cmp, reason = "widths come from a fixed list")]
                            width: v.width.filter(|&w| w == s.width),
                            ..v
                        },
                    })
                })
            }
            _ => None,
        }
    }

    /// A style panel choice: it styles the Selection if there is one, else
    /// the next Strokes.
    fn choose(&mut self, control: Control) {
        if self.selection.is_empty() {
            self.style = self.style.with(control);
        } else {
            style::restyle(
                &mut self.ink,
                self.selection.ids(),
                control,
                &mut self.restyling,
                (&mut self.freehand, &mut self.paint, self.scale),
            );
        }
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

    fn tool_busy(&self) -> bool {
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
                    self.choose(control);
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
                self.choose(control);
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

    fn key(&mut self, key: Key, command: bool, shift: bool) {
        let (tool, mut ctx) = self.tool();
        if tool.key(&mut ctx, key, command, shift) {
            return;
        }
        match (key, command, shift) {
            // Undo, redo and switching wait for the Stroke to end: it is not
            // logged yet.
            (Key::Char(_), _, _) if self.tool_busy() || self.panel.pressing() => {}
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
            (Key::Char(c), false, false) => {
                self.switch_tool(|tools| {
                    tools.select_by_key(c);
                });
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
        self.draw_mode = false;
        self.selection.clear();
        self.laser.clear();
        self.toolbar.forget_pointer();
        self.panel.forget_pointer();
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
