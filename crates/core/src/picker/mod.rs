//! The Color Picker and the Width Picker: one popover the core draws next to
//! a Palette slot (ADR-0004).
//!
//! The state is small: which slot, which tab, what the pointer is over and
//! what it is dragging. Geometry is never stored; `layout` recomputes it from
//! the slot's own position on every use. The picture is `paint`, the Custom
//! tab's pixel buffers are `custom`, and `color` holds the math and the
//! Presets.
//!
//! The pointer only ever produces an [`Outcome`]; what it means for the
//! Palette, the Selection and Recent Colors is the router's business
//! (`route/picker.rs`).

mod color;
mod custom;
mod layout;
mod paint;
mod sliders;
mod width;

use crate::ink::Point;
use crate::palette::{SlotKind, SlotValue};
use crate::recent::RecentColors;
use crate::toolbar::{Area, Theme};
use color::{Hsv, NEUTRALS};
use custom::Custom;
pub(crate) use layout::Frame;
use layout::{Geometry, Part, Tab};

/// A constant stand-in for the Custom color while another tab is up, so that a
/// hue remembered meanwhile never repaints the Presets.
const NO_HSV: Hsv = Hsv {
    h: 0.0,
    s: 0.0,
    v: 0.0,
};

/// How far the shadow reaches around the popover, logical px.
const SHADOW: [f32; 4] = [16.0, 16.0, 16.0, 24.0];

/// What a press or drag on the popover asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Outcome {
    /// A Preset or Recent Color: set the slot, choose it, close.
    Pick([u8; 3]),
    /// The tab changed; the popover stays.
    Tab,
    /// A drag started or moved: the slot takes this value, live.
    Set(SlotValue),
    Reset,
    /// "...": hand over to the system color panel.
    More,
    /// A press on nothing: swallowed.
    Inert,
}

/// What a drag in progress moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drag {
    Square,
    Hue,
    Track,
}

#[derive(Clone, Copy, Debug)]
struct Open {
    kind: SlotKind,
    index: usize,
    hover: Option<Part>,
    drag: Option<Drag>,
    /// The Custom tab's color while it is up. A gray has no hue, so this
    /// holds the hue the drag came from.
    hsv: Hsv,
}

/// Everything that decides how the popover looks, besides the screen it is
/// painted on. A difference from the last painted one means a repaint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    geom: Geometry,
    hover: Option<Part>,
    hsv: Hsv,
    value: SlotValue,
    /// The Pen's color, for the Width Picker's preview stroke.
    stroke: [u8; 3],
    recent: RecentColors,
    theme: Theme,
}

impl Look {
    /// The popover plus its shadow, in physical pixels.
    pub fn region(&self) -> Area {
        let s = self.geom.rect_scale();
        self.geom.rect().grow(SHADOW.map(|v| v * s))
    }
}

/// The popover for one render pass: its state plus how it should look now.
pub(crate) struct Popover<'a> {
    pub picker: &'a mut Picker,
    pub look: Option<Look>,
}

#[derive(Debug, Default)]
pub(crate) struct Picker {
    open: Option<Open>,
    /// The last tab used: the next Color Picker opens on it (this session only).
    tab: Tab,
    /// The last hue dragged to, for a color that has none.
    hue: f32,
    /// The pointer is over the popover (for the cursor).
    over: bool,
    custom: Custom,
    painted: Option<Look>,
}

impl Picker {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn slot(&self) -> Option<(SlotKind, usize)> {
        self.open.map(|o| (o.kind, o.index))
    }

    pub fn dragging(&self) -> bool {
        self.open.is_some_and(|o| o.drag.is_some())
    }

    pub fn over(&self) -> bool {
        self.over
    }

    /// Opens on `kind`'s slot `index`, replacing any open popover. `value`
    /// is what the slot holds.
    pub fn open(&mut self, kind: SlotKind, index: usize, value: SlotValue) {
        let hsv = self.hsv_of(value);
        self.open = Some(Open {
            kind,
            index,
            hover: None,
            drag: None,
            hsv,
        });
    }

    pub fn close(&mut self) {
        self.open = None;
        self.over = false;
    }

    /// Closes and gives back the Custom tab's buffers (Draw Mode ended).
    pub fn free(&mut self) {
        self.close();
        self.custom = Custom::default();
        self.painted = None;
    }

    fn hsv_of(&self, value: SlotValue) -> Hsv {
        match value {
            SlotValue::Color(rgb) => Hsv::of(rgb, self.hue),
            SlotValue::Width(_) => Hsv {
                h: self.hue,
                s: 1.0,
                v: 1.0,
            },
        }
    }

    fn geometry(&self, frame: &Frame) -> Option<Geometry> {
        let open = self.open?;
        Some(Geometry::place(frame, open.kind, self.tab))
    }

    pub fn contains(&self, at: Point, frame: &Frame) -> bool {
        self.geometry(frame).is_some_and(|g| g.rect().contains(at))
    }

    /// The part under `at` that reacts to the pointer; Recent cells beyond the
    /// list and the popover's padding are nothing.
    fn part_at(g: &Geometry, at: Point, recent: &RecentColors) -> Option<Part> {
        match g.hit(at)? {
            Part::Recent(i) if i >= recent.as_slice().len() => None,
            Part::Panel => None,
            part => Some(part),
        }
    }

    /// The pointer moved over the popover without a button down.
    pub fn hover(&mut self, at: Point, frame: &Frame, recent: &RecentColors) {
        let Some(g) = self.geometry(frame) else {
            return;
        };
        let part = Self::part_at(&g, at, recent)
            // Pictures and the slider are not buttons: nothing to highlight.
            .filter(|p| !matches!(p, Part::Square | Part::Hue | Part::Track));
        self.over = g.rect().contains(at);
        if let Some(open) = self.open.as_mut() {
            open.hover = part;
        }
    }

    /// A press inside the popover. `value` is what the slot holds now.
    pub fn press(
        &mut self,
        at: Point,
        frame: &Frame,
        value: SlotValue,
        recent: &RecentColors,
    ) -> Outcome {
        let Some(g) = self.geometry(frame) else {
            return Outcome::Inert;
        };
        let Some(part) = Self::part_at(&g, at, recent) else {
            return Outcome::Inert;
        };
        match part {
            Part::Tab(tab) => {
                self.tab = tab;
                let hsv = self.hsv_of(value);
                if let Some(open) = self.open.as_mut() {
                    open.hsv = hsv;
                    open.hover = None;
                }
                Outcome::Tab
            }
            Part::Neutral(i) => NEUTRALS
                .get(i)
                .map_or(Outcome::Inert, |&c| Outcome::Pick(c)),
            Part::Preset(i) => color::preset(i).map_or(Outcome::Inert, Outcome::Pick),
            Part::Recent(i) => recent
                .as_slice()
                .get(i)
                .map_or(Outcome::Inert, |&c| Outcome::Pick(c)),
            Part::Reset => Outcome::Reset,
            Part::More => Outcome::More,
            Part::Square | Part::Hue | Part::Track => {
                let drag = match part {
                    Part::Square => Drag::Square,
                    Part::Hue => Drag::Hue,
                    _ => Drag::Track,
                };
                if let Some(open) = self.open.as_mut() {
                    open.drag = Some(drag);
                    open.hover = None;
                }
                self.drag(at, frame).map_or(Outcome::Inert, Outcome::Set)
            }
            Part::Panel => Outcome::Inert,
        }
    }

    /// The pointer moved with the button down on the square, the strip or
    /// the slider: the slot's new value. It keeps tracking outside them.
    pub fn drag(&mut self, at: Point, frame: &Frame) -> Option<SlotValue> {
        let g = self.geometry(frame)?;
        let open = self.open.as_mut()?;
        match open.drag? {
            Drag::Square => {
                let (fx, fy) = g.fraction(Part::Square, at)?;
                open.hsv.s = fx;
                open.hsv.v = 1.0 - fy;
            }
            Drag::Hue => {
                let (fx, _) = g.fraction(Part::Hue, at)?;
                open.hsv.h = fx * 360.0;
            }
            Drag::Track => {
                let track = g.rect_of(Part::Track)?;
                let w = width::width_at(at.x, track, g.rect_scale())?;
                return Some(SlotValue::Width(w));
            }
        }
        self.hue = open.hsv.h;
        Some(SlotValue::Color(open.hsv.rgb()))
    }

    /// Ends a drag; tells whether there was one.
    pub fn release(&mut self) -> bool {
        self.open
            .as_mut()
            .is_some_and(|open| open.drag.take().is_some())
    }

    /// The popover as it should look now, if it is open.
    pub fn look(
        &self,
        frame: &Frame,
        theme: Theme,
        value: SlotValue,
        stroke: [u8; 3],
        recent: RecentColors,
    ) -> Option<Look> {
        let open = self.open?;
        let geom = self.geometry(frame)?;
        let custom = open.kind == SlotKind::Color && self.tab == Tab::Custom;
        Some(Look {
            geom,
            hover: open.hover,
            hsv: if custom { open.hsv } else { NO_HSV },
            value,
            stroke: if open.kind == SlotKind::Width {
                stroke
            } else {
                [0; 3]
            },
            recent,
            theme,
        })
    }

    /// Whether `look` differs from what was last painted.
    pub fn is_dirty(&self, look: Option<Look>) -> bool {
        self.painted != look
    }

    pub fn painted(&self) -> Option<Look> {
        self.painted
    }

    /// Records what is now on screen.
    pub fn done(&mut self, look: Option<Look>) {
        self.painted = look;
    }

    /// Where `part` is, for tests that click the popover.
    #[cfg(feature = "test-support")]
    pub fn point(&self, part: PickerPart, frame: &Frame) -> Option<Point> {
        let g = self.geometry(frame)?;
        let middle = (0.5, 0.5);
        let tab = |i: usize| layout::Tab::ALL.get(i).copied().map(Part::Tab);
        match part {
            PickerPart::Tab(i) => g.point_at(tab(i)?, middle),
            PickerPart::Preset(i) => g.point_at(Part::Preset(i), middle),
            PickerPart::Neutral(i) => g.point_at(Part::Neutral(i), middle),
            PickerPart::Recent(i) => g.point_at(Part::Recent(i), middle),
            PickerPart::Reset => g.point_at(Part::Reset, middle),
            PickerPart::More => g.point_at(Part::More, middle),
            PickerPart::Square(x, y) => g.point_at(Part::Square, (x, y)),
            PickerPart::Hue(x) => g.point_at(Part::Hue, (x, 0.5)),
            PickerPart::Track(f) => {
                let track = g.rect_of(Part::Track)?;
                let (from, to) = width::travel(track, g.rect_scale());
                Some(Point {
                    x: from + f.clamp(0.0, 1.0) * (to - from),
                    y: track.y + track.h / 2.0,
                })
            }
        }
    }
}

/// A part of the open popover, for tests that click it. Positions are in
/// reading order; `Square` takes the saturation (left to right) and value
/// (top to bottom, so 0 is the brightest) as fractions of the square, `Hue`
/// and `Track` a fraction along the strip or the slider.
#[cfg(feature = "test-support")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PickerPart {
    Tab(usize),
    /// A cell of the grid: row by row, nine hues across, four tones down.
    Preset(usize),
    Neutral(usize),
    Recent(usize),
    Reset,
    More,
    Square(f32, f32),
    Hue(f32),
    Track(f32),
}
