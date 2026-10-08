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
mod route;
mod selection;
mod style;
mod svg_path;
mod toolbar;
mod tools;
mod view;

pub use config::Config;
use freehand::Scratch;
use history::History;
pub use ink::{Element, Ink, Point};
use laser::Laser;
use panel::Panel;
pub use render::{Damage, Format};
use selection::Selection;
pub use style::Control;
use style::Style;
use toolbar::Toolbar;
pub use toolbar::{Button, Theme};
use tools::Tools;
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
    /// Height in physical pixels of the area at the top of the Overlay that
    /// belongs to the OS (macOS menu bar and notch, Windows work area). The
    /// toolbar and style panel are laid out below it; Ink can still go there.
    Insets {
        top: u32,
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
        /// Alt (Option) is held: Tool shortcut keys then do nothing.
        alt: bool,
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
    /// Draw Mode is on. The Overlay exists exactly then: Ink is shown only
    /// in Draw Mode, so leaving it destroys the Overlay and entering it again
    /// on the same display shows the same Ink.
    pub draw_mode: bool,
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
}
