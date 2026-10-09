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
mod palette;
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
pub use palette::{Anchor, EditRequest, Palette, Side, SlotKind, SlotValue, MAX_WIDTH, MIN_WIDTH};
pub use render::{Damage, Format};
use selection::Selection;
use style::Appearance;
use style::Choice;
pub use style::Style;
use toolbar::Toolbar;
pub use toolbar::{Button, Insets, Theme, ToolbarPosition};
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
    /// The parts of each edge of the Overlay that belong to the OS (menu bar,
    /// notch, Dock, taskbar), in physical pixels. The Toolbar is laid out
    /// clear of them; Ink can still go there.
    Insets(Insets),
    /// Where the Toolbar sits. Sent at startup and when Settings changes it.
    ToolbarPosition(ToolbarPosition),
    /// The remembered Style of the Pen, sent at startup. Out-of-range
    /// indices are ignored.
    Style(Style),
    /// The remembered Palette, sent at startup. Widths are snapped to 0.25 and
    /// clamped to 0.5..=30. The Pen keeps its chosen slots.
    Palette(Palette),
    /// Sets color slot `slot` and chooses it, as a click on it would. An
    /// out-of-range slot is ignored. Reset is this event with the default
    /// ([`Palette::DEFAULT`]).
    EditColor {
        slot: usize,
        rgb: [u8; 3],
    },
    /// Sets width slot `slot` (snapped to 0.5, clamped to 0.5..=30; not finite
    /// is ignored) and chooses it. Consecutive edits of one slot restyle a
    /// Selection as one undo step, until [`Event::EditEnd`], a key or a click.
    EditWidth {
        slot: usize,
        width: f32,
    },
    /// The editor closed: the next edit is a new undo step. While an editor
    /// is open (from [`View::edit`] until this event) the first
    /// [`Event::PointerDown`] is the click that dismisses it: the core ends the
    /// edit as this event would and swallows that press, its moves and its
    /// release, so a dismissing click never draws. The platform still sends
    /// this event when an editor closes.
    EditEnd,
    /// A secondary click (right button, or Control+left on macOS) at this
    /// position. On a color or width button it sets [`View::edit`]; anywhere
    /// else it does nothing, and it never draws or changes the Tool.
    SecondaryClick(Point),
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
    /// The Clear hotkey: removes all Ink (undoable). Draw Mode stays as it is:
    /// nothing appears or disappears.
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
#[derive(Clone, Copy, Debug, PartialEq)]
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
    /// The Pen's Style (color and width indices). A Selection restyle does
    /// not change it. The platform saves it when it changes.
    pub style: Style,
    /// The Palette the slots hold. The platform saves it when it changes.
    pub palette: Palette,
    /// Set by a [`Event::SecondaryClick`] on a color or width button, until
    /// the next event: the platform opens that slot's editor.
    pub edit: Option<EditRequest>,
    /// The time (same clock as [`Event::Clock`]) the next frame is due. Set
    /// only while something animates (a visible Laser trail): wait for it,
    /// then send `Clock`. `None` means sleep until the next input.
    pub next_frame: Option<u64>,
}

/// Whether a Palette editor is open, as far as the core can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Editor {
    Closed,
    /// From the [`View::edit`] that asked for it until [`Event::EditEnd`] or
    /// the first press, which dismisses it.
    Open,
    /// The press that dismissed an editor is in flight: its moves and release
    /// are swallowed too, so it never draws.
    Dismissing,
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
    /// The Palette: what the color and width slots hold.
    palette: Palette,
    /// The Pen's chosen slots, and the same resolved against the Palette.
    /// Slots are kept apart from values so two slots may hold one value.
    slots: Style,
    style: Appearance,
    /// Set by a secondary click on a Palette button, until the next event.
    edit: Option<EditRequest>,
    editor: Editor,
    /// The slot whose edits are being coalesced into one restyle.
    editing: Option<(SlotKind, usize)>,
    /// The Appearance of the selected Elements before the restyle in progress.
    restyling: Option<Vec<(usize, Appearance)>>,
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
            palette: Palette::DEFAULT,
            slots: Style::default(),
            style: Appearance::default(),
            edit: None,
            editor: Editor::Closed,
            editing: None,
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
