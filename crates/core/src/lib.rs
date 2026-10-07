//! Markuli annotator core: pure, platform-free.
//!
//! One way in: [`Annotator::handle`] takes an [`Event`]. Out come the
//! [`View`] state, the [`Ink`], and pixels via [`Annotator::render`].

mod config;
pub mod freehand;
mod history;
mod ink;
mod render;

pub use config::Config;
use freehand::Scratch;
use history::History;
pub use ink::{Element, Ink, Point};
pub use render::{Damage, Format};

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
    stroking: bool,
    history: History,
    paint: render::Pending,
    freehand: Scratch,
    scale: f32,
    pressure: Option<f32>,
    next_id: u64,
}

impl Default for Annotator {
    fn default() -> Self {
        Self {
            ink: Ink::default(),
            draw_mode: false,
            display: None,
            stroking: false,
            history: History::default(),
            paint: render::Pending::default(),
            freehand: Scratch::default(),
            scale: 1.0,
            pressure: None,
            next_id: 1,
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
            Event::Pressure(p) => self.pressure = p.filter(|p| p.is_finite()),
            Event::ScaleFactor(scale) if scale.is_finite() && scale > 0.0 => {
                self.scale = scale;
                self.paint.full();
            }
            Event::PointerDown(at) if self.draw_mode => {
                // A lost PointerUp must not merge two Strokes into one log entry.
                self.finish_stroke();
                if let Some(at) = self.logical(at) {
                    self.stroking = true;
                    let element = Element::start(self.next_id, at, self.pressure);
                    self.next_id += 1;
                    self.ink.add(element);
                    self.advance(Element::preview);
                }
            }
            Event::PointerMove(at) if self.stroking => {
                if let Some(at) = self.logical(at) {
                    let pressure = self.pressure;
                    self.advance(|e, scratch| e.push(at, pressure, scratch));
                }
            }
            Event::PointerUp(at) if self.stroking => {
                let at = self.logical(at);
                let pressure = self.pressure;
                self.advance(|e, scratch| match at {
                    Some(at) => e.commit(at, pressure, scratch),
                    None => e.finish(scratch),
                });
                self.stroking = false;
                self.history.record_add();
            }
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
        View {
            draw_mode: self.draw_mode,
            overlay_needed: self.draw_mode || !self.ink.is_empty(),
            display: self.display,
            needs_render: self.paint.is_pending(),
        }
    }

    #[must_use]
    pub fn ink(&self) -> &Ink {
        &self.ink
    }

    /// Draws what changed since the last call into `target` (premultiplied
    /// pixels, `format` channel order) and returns the changed region.
    pub fn render(
        &mut self,
        target: &mut tiny_skia::PixmapMut<'_>,
        format: Format,
    ) -> Option<Damage> {
        render::render(
            &self.ink,
            &mut self.paint,
            self.draw_mode,
            self.scale,
            target,
            format,
        )
    }

    /// Physical pointer position to logical Element coordinates.
    fn logical(&self, at: Point) -> Option<Point> {
        (at.x.is_finite() && at.y.is_finite()).then(|| Point {
            x: at.x / self.scale,
            y: at.y / self.scale,
        })
    }

    /// Runs `step` on the Stroke in progress and marks what it changed.
    fn advance(&mut self, step: impl FnOnce(&mut Element, &mut Scratch) -> Option<ink::Rect>) {
        let scale = self.scale;
        if let Some(element) = self.ink.last_mut() {
            let (x, y) = (element.x(), element.y());
            if let Some(local) = step(element, &mut self.freehand) {
                self.paint.damage_local(local, (x, y), scale);
            }
        }
    }

    /// Ends the Stroke in progress without a final point and logs it.
    fn finish_stroke(&mut self) {
        if self.stroking {
            self.advance(Element::finish);
            self.stroking = false;
            self.history.record_add();
        }
    }

    fn key(&mut self, key: Key, command: bool, shift: bool) {
        match (key, command, shift) {
            // Esc only cancels the Stroke in progress; it never Clears.
            (Key::Escape, _, _) => {
                if self.stroking {
                    self.stroking = false;
                    self.ink.pop();
                    self.paint.full();
                }
            }
            // Undo and redo wait for the Stroke to end: it is not logged yet.
            (Key::Char(_), true, _) if self.stroking => {}
            (Key::Char('z'), true, false) => self.undo(),
            (Key::Char('z'), true, true) | (Key::Char('y'), true, false) => self.redo(),
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
        self.finish_stroke();
        if !self.ink.is_empty() {
            self.history.record_clear(self.ink.take());
        }
        self.draw_mode = false;
        self.paint.full();
    }

    fn toggle(&mut self, display: DisplayId) {
        self.finish_stroke();
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
