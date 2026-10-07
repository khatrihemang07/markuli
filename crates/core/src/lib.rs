//! Markuli annotator core: pure, platform-free.
//!
//! One way in: [`Annotator::handle`] takes an [`Event`]. Out come the
//! [`View`] state, the [`Ink`], and pixels via [`Annotator::render`].

mod config;
mod ink;
mod render;

pub use config::Config;
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
    PointerDown(Point),
    PointerMove(Point),
    PointerUp(Point),
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

#[derive(Debug, Default)]
pub struct Annotator {
    ink: Ink,
    draw_mode: bool,
    display: Option<DisplayId>,
    stroking: bool,
    paint: render::Pending,
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
            Event::PointerDown(at) if self.draw_mode => {
                self.stroking = true;
                self.ink.add(ink::Element::start(at));
            }
            Event::PointerMove(at) | Event::PointerUp(at) if self.stroking => {
                if let Some(element) = self.ink.last_mut() {
                    element.push(at);
                }
                if matches!(event, Event::PointerUp(_)) {
                    self.stroking = false;
                }
            }
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
            needs_render: self.paint.is_pending() || self.ink.has_unrendered(),
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
            &mut self.ink,
            &mut self.paint,
            self.draw_mode,
            target,
            format,
        )
    }

    fn toggle(&mut self, display: DisplayId) {
        self.stroking = false;
        if self.draw_mode {
            self.draw_mode = false;
        } else {
            if self.display != Some(display) {
                self.ink.clear();
            }
            self.display = Some(display);
            self.draw_mode = true;
        }
        // Backdrop (hit-test layer) appears and disappears with Draw Mode.
        self.paint.full();
    }
}
