//! The Excalidraw-style toolbar: state and hit-testing.
//!
//! Two islands at the top centre of the Overlay, in Draw Mode only: the
//! Tool buttons, then undo, redo and Clear. Metrics and colours are
//! Excalidraw's stock theme (`theme.scss`, `ToolIcon.scss`, `Island.scss`),
//! in logical pixels times the Overlay's scale factor. Deviation: the shadow
//! is a stack of rounded rectangles instead of a blur, and there is no
//! pressed-state border.
//!
//! Split in four: this file (state, hit-testing), `layout` (where things
//! are), `paint` (icons and fills) and `theme` (tokens and shapes).

mod layout;
mod paint;
mod theme;

use crate::ink::Point;
use crate::panel::{Panel, PanelView};
use crate::render::{Canvas, Format};
use crate::tools::Tools;
use layout::Layout;

pub(crate) use layout::Area;
pub use theme::Theme;
pub(crate) use theme::{fill_rounded, rounded_rect, shadow, solid_paint};

/// A toolbar button. `Tool(i)` is the i-th registered Tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Tool(usize),
    Undo,
    Redo,
    Clear,
}

/// What decides how the toolbar looks besides the Toolbar's own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UiState {
    pub active: usize,
    pub can_undo: bool,
    pub can_redo: bool,
    pub has_ink: bool,
}

/// Everything painted last time; a difference means a repaint is due.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    ui: UiState,
    hover: Option<Button>,
    press: Option<Button>,
    theme: Theme,
    size: (f32, f32),
    scale: f32,
    top: f32,
}

/// Where a pointer position lands on the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Button(Button),
    /// Island padding: inert, but still not the canvas.
    Dead,
}

#[derive(Debug)]
pub(crate) struct Toolbar {
    pub theme: Theme,
    /// Overlay size in physical pixels, and its scale factor.
    size: Option<(f32, f32)>,
    scale: f32,
    /// Physical pixels at the top that belong to the OS.
    top: f32,
    hover: Option<Button>,
    press: Option<Button>,
    over: bool,
    painted: Option<Look>,
}

impl Default for Toolbar {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            size: None,
            scale: 1.0,
            top: 0.0,
            hover: None,
            press: None,
            over: false,
            painted: None,
        }
    }
}

impl Toolbar {
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = Some((layout::to_f32(width), layout::to_f32(height)));
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn set_top(&mut self, top: u32) {
        self.top = layout::to_f32(top);
    }

    pub fn hover_at(&mut self, at: Point, tools: usize) {
        let hit = self.hit(at, tools);
        self.over = hit.is_some();
        self.hover = match hit {
            Some(Hit::Button(b)) => Some(b),
            _ => None,
        };
    }

    pub fn over(&self) -> bool {
        self.over
    }

    /// Presses on a button, if `at` is on one.
    pub fn press_at(&mut self, at: Point, tools: usize) -> bool {
        self.hover_at(at, tools);
        self.press = self.hover;
        self.over
    }

    /// Ends a press; yields the button when released on the same one.
    pub fn release_at(&mut self, at: Point, tools: usize) -> Option<Button> {
        let pressed = self.press.take()?;
        self.hover_at(at, tools);
        (self.hover == Some(pressed)).then_some(pressed)
    }

    pub fn pressing(&self) -> bool {
        self.press.is_some()
    }

    pub fn forget_pointer(&mut self) {
        (self.hover, self.press, self.over) = (None, None, false);
    }

    fn layout(&self, tools: usize) -> Option<Layout> {
        let (width, _) = self.size?;
        Some(Layout::new(width, self.scale, self.top, tools))
    }

    pub fn hit(&self, at: Point, tools: usize) -> Option<Hit> {
        let l = self.layout(tools)?;
        for i in 0..tools + 3 {
            if l.button_rect(i).contains(at) {
                return Some(Hit::Button(layout::button(i, tools)));
            }
        }
        l.islands()
            .iter()
            .any(|r| r.contains(at))
            .then_some(Hit::Dead)
    }

    #[cfg(feature = "test-support")]
    pub fn center_of(&self, button: Button, tools: usize) -> Option<Point> {
        let l = self.layout(tools)?;
        let i = match button {
            Button::Tool(i) if i < tools => i,
            Button::Tool(_) => return None,
            Button::Undo => tools,
            Button::Redo => tools + 1,
            Button::Clear => tools + 2,
        };
        let r = l.button_rect(i);
        Some(Point {
            x: r.x + r.w / 2.0,
            y: r.y + r.h / 2.0,
        })
    }

    fn look(&self, visible: bool, ui: UiState) -> Option<Look> {
        let size = self.size?;
        visible.then_some(Look {
            ui,
            hover: self.hover,
            press: self.press,
            theme: self.theme,
            size,
            scale: self.scale,
            top: self.top,
        })
    }

    /// True when the pixels on screen no longer match the state.
    pub fn is_dirty(&self, visible: bool, ui: UiState) -> bool {
        self.painted != self.look(visible, ui)
    }
}

/// The toolbar for one render pass: its state plus what it paints from.
pub(crate) struct Chrome<'a> {
    pub toolbar: &'a mut Toolbar,
    pub tools: &'a Tools,
    pub visible: bool,
    pub ui: UiState,
    /// The style panel, and what it shows (`None`: hidden).
    pub panel: &'a mut Panel,
    pub panel_view: Option<PanelView>,
}

impl Chrome<'_> {
    /// Where the style panel is on screen right now, if it is shown.
    pub fn panel_region(&self) -> Option<Area> {
        self.panel_view.map(|_| self.panel.region())
    }

    /// The region the style panel painted last (to clear it when it moves
    /// away or changes).
    pub fn panel_shown(&self) -> Option<Area> {
        self.panel.shown()
    }

    pub fn panel_dirty(&self) -> bool {
        self.panel.is_dirty(self.panel_view)
    }

    pub fn paint_panel(&self, target: &mut Canvas<'_, '_>, format: Format) {
        if let Some(view) = self.panel_view {
            self.panel.paint(view, target, format);
        }
    }

    /// Where the toolbar is on screen right now, if it is shown.
    pub fn region(&self) -> Option<Area> {
        if self.visible {
            Some(self.toolbar.layout(self.tools.len())?.region())
        } else {
            None
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.toolbar.is_dirty(self.visible, self.ui)
    }

    /// Records what is now on screen.
    pub fn done(&mut self) {
        self.toolbar.painted = self.toolbar.look(self.visible, self.ui);
        self.panel.done(self.panel_view);
    }
}
