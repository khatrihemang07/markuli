//! The Excalidraw-style toolbar: state and hit-testing.
//!
//! One row (top or bottom) or column (left or right) of the Overlay, in Draw Mode only, in four
//! islands: the Tool buttons, the five colors, the three widths, then undo,
//! redo and Clear. Metrics and colors are
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
use crate::palette::{Anchor, Palette, Side};
use crate::tools::Tools;
use layout::Layout;

pub(crate) use layout::Area;
pub use layout::{Insets, ToolbarPosition};
pub use theme::Theme;

/// A toolbar button. `Tool(i)` is the i-th registered Tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Tool(usize),
    /// The i-th color of the palette.
    Color(usize),
    /// The i-th width: thin, medium, bold.
    Width(usize),
    Undo,
    Redo,
    Clear,
}

/// What decides how the toolbar looks besides the Toolbar's own state.
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "a plain snapshot of independent button states, compared whole"
)]
pub(crate) struct UiState {
    pub active: usize,
    pub can_undo: bool,
    pub can_redo: bool,
    pub has_ink: bool,
    /// The chosen color and width (palette and preset indices); `None` when
    /// a Selection mixes values.
    pub color: Option<usize>,
    pub width: Option<usize>,
    /// The active Tool styles nothing: colors and widths are shown dimmed
    /// (a click still works: it hands over to the Pen).
    pub dimmed: bool,
    /// What the color swatches and width icons show.
    pub palette: Palette,
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
    insets: Insets,
    position: ToolbarPosition,
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
    /// Physical pixels at each edge that belong to the OS.
    insets: Insets,
    position: ToolbarPosition,
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
            insets: Insets::default(),
            position: ToolbarPosition::default(),
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

    pub fn set_insets(&mut self, insets: Insets) {
        self.insets = insets;
    }

    pub fn set_position(&mut self, position: ToolbarPosition) {
        self.position = position;
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
        let size = self.size?;
        Some(Layout::new(
            size,
            self.scale,
            self.insets,
            self.position,
            tools,
        ))
    }

    pub fn hit(&self, at: Point, tools: usize) -> Option<Hit> {
        let l = self.layout(tools)?;
        for i in 0..layout::count(tools) {
            if l.button_rect(i).contains(at) {
                return Some(Hit::Button(layout::button(i, tools)));
            }
        }
        l.islands()
            .iter()
            .any(|r| r.contains(at))
            .then_some(Hit::Dead)
    }

    /// The color or width button at `at` and where it is, for an edit request.
    pub fn palette_button_at(&self, at: Point, tools: usize) -> Option<(Button, Anchor, Side)> {
        let l = self.layout(tools)?;
        let i = (0..layout::count(tools)).find(|&i| l.button_rect(i).contains(at))?;
        let button = layout::button(i, tools);
        let r = l.button_rect(i);
        matches!(button, Button::Color(_) | Button::Width(_)).then_some((
            button,
            Anchor {
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
            },
            l.side(),
        ))
    }

    #[cfg(feature = "test-support")]
    pub fn center_of(&self, button: Button, tools: usize) -> Option<Point> {
        let l = self.layout(tools)?;
        let r = l.button_rect(layout::index_of(button, tools)?);
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
            insets: self.insets,
            position: self.position,
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
}

impl Chrome<'_> {
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
    }
}
