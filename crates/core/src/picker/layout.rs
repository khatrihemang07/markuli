//! Where the popover and its parts are: placement next to the slot, clamped
//! inside the usable Overlay, and the rectangles of every part. Nothing here
//! is stored between events; it is recomputed from the slot's own geometry,
//! so the Toolbar Position, insets, scale and resize never leave it stale.

#![allow(
    clippy::many_single_char_names,
    reason = "x, y, w, h and s are plain geometry"
)]

use super::color::{COLUMNS, TONES};
use crate::ink::Point;
use crate::palette::{Side, SlotKind};
use crate::toolbar::Area;

/// Distance from the slot's button to the popover, logical px.
const GAP: f32 = 12.0;
/// Least distance from the popover to the edge of the usable Overlay.
const MARGIN: f32 = 8.0;
/// Inner padding of the popover.
const PAD: f32 = 12.0;
/// A cell's diameter and its pitch.
pub(super) const CELL: f32 = 20.0;
const PITCH: f32 = 26.0;
/// Reset, "..." and tab height; icon buttons are square.
const BUTTON: f32 = 28.0;
const TAB_W: f32 = 36.0;

/// The Color Picker and Width Picker sizes. The two Color tabs are the same
/// size, so switching tabs never moves the popover.
const COLOR_SIZE: (f32, f32) = (252.0, 264.0);
const WIDTH_SIZE: (f32, f32) = (232.0, 100.0);

/// Rows of the Color Picker, logical px from the popover's top.
const TABS_Y: f32 = PAD;
const NEUTRAL_CY: f32 = 58.0;
const GRID_CY: f32 = 90.0;
const RECENT_CY: f32 = 200.0;
const FOOTER_Y: f32 = 224.0;
/// Where the hairline above the footer is.
const DIVIDER_Y: f32 = 218.0;
const SQUARE: (f32, f32, f32, f32) = (PAD, 48.0, 228.0, 136.0);
const HUE: (f32, f32, f32, f32) = (PAD, 196.0, 228.0, 16.0);

/// Rows of the Width Picker.
const PREVIEW: (f32, f32, f32, f32) = (PAD, PAD, 172.0, 44.0);
const TRACK: (f32, f32, f32, f32) = (PAD, 64.0, 208.0, 24.0);
/// Knob radius: the travel of its center is the track minus one each end.
pub(super) const KNOB: f32 = 8.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    Presets,
    Custom,
}

impl Tab {
    pub const ALL: [Self; 2] = [Self::Presets, Self::Custom];
}

/// A part of the popover that reacts to the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Tab(Tab),
    Neutral(usize),
    /// A cell of the grid, in reading order.
    Preset(usize),
    Recent(usize),
    Reset,
    More,
    Square,
    Hue,
    Track,
    /// The popover itself: inert.
    Panel,
}

/// What a drag in progress moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Drag {
    Square,
    Hue,
    Track,
}

impl Part {
    /// The drag a press on this part starts, if it is a picture or the slider.
    pub fn drag(self) -> Option<Drag> {
        match self {
            Self::Square => Some(Drag::Square),
            Self::Hue => Some(Drag::Hue),
            Self::Track => Some(Drag::Track),
            _ => None,
        }
    }
}

/// Everything the placement depends on, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    /// The slot's button.
    pub anchor: Area,
    /// The side of the button the popover opens on.
    pub side: Side,
    /// The Overlay minus the OS insets.
    pub usable: Area,
    pub scale: f32,
}

/// One placed popover: the top-left corner, the scale, and what it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Geometry {
    x: f32,
    y: f32,
    scale: f32,
    pub kind: SlotKind,
    pub tab: Tab,
}

fn size(kind: SlotKind) -> (f32, f32) {
    match kind {
        SlotKind::Color => COLOR_SIZE,
        SlotKind::Width => WIDTH_SIZE,
    }
}

/// `lo..=hi` clamp that prefers `lo` when the range is empty, like
/// `Layout::across`: a popover wider than the Overlay starts at the margin.
fn clamp_start(v: f32, lo: f32, hi: f32) -> f32 {
    v.min(hi).max(lo)
}

impl Geometry {
    pub fn place(frame: &Frame, kind: SlotKind, tab: Tab) -> Self {
        let (w, h) = size(kind);
        let s = frame.scale;
        let (w, h, gap, margin) = (w * s, h * s, GAP * s, MARGIN * s);
        let (a, u) = (frame.anchor, frame.usable);
        let (cx, cy) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
        let (x, y) = match frame.side {
            Side::Below => (cx - w / 2.0, a.y + a.h + gap),
            Side::Above => (cx - w / 2.0, a.y - gap - h),
            Side::Right => (a.x + a.w + gap, cy - h / 2.0),
            Side::Left => (a.x - gap - w, cy - h / 2.0),
        };
        Self {
            // Whole pixels, so the Custom tab's pictures are not resampled.
            x: clamp_start(x, u.x + margin, u.x + u.w - margin - w).round(),
            y: clamp_start(y, u.y + margin, u.y + u.h - margin - h).round(),
            scale: s,
            kind,
            tab,
        }
    }

    pub fn rect_scale(&self) -> f32 {
        self.scale
    }

    /// The popover itself.
    pub fn rect(&self) -> Area {
        let (w, h) = size(self.kind);
        Area {
            x: self.x,
            y: self.y,
            w: w * self.scale,
            h: h * self.scale,
        }
    }

    /// A rectangle given in logical px from the popover's corner.
    fn at(&self, (x, y, w, h): (f32, f32, f32, f32)) -> Area {
        let s = self.scale;
        Area {
            x: self.x + x * s,
            y: self.y + y * s,
            w: w * s,
            h: h * s,
        }
    }

    /// The pitch-sized square around the center of a cell of a row.
    fn cell(&self, i: usize, cy: f32) -> Area {
        #[allow(clippy::cast_precision_loss, reason = "a handful of cells")]
        let cx = PAD + CELL / 2.0 + PITCH * i as f32;
        self.at((cx - PITCH / 2.0, cy - PITCH / 2.0, PITCH, PITCH))
    }

    /// The ring and hover disc of a cell are drawn around this center.
    pub fn center_of(&self, part: Part) -> Option<(f32, f32)> {
        self.rect_of(part)
            .map(|r| (r.x + r.w / 2.0, r.y + r.h / 2.0))
    }

    pub fn rect_of(&self, part: Part) -> Option<Area> {
        let color = self.kind == SlotKind::Color;
        let presets = color && self.tab == Tab::Presets;
        let custom = color && self.tab == Tab::Custom;
        Some(match part {
            Part::Tab(t) if color => {
                let i = Tab::ALL.iter().position(|&x| x == t)?;
                #[allow(clippy::cast_precision_loss, reason = "two tabs")]
                self.at((PAD + TAB_W * i as f32, TABS_Y, TAB_W, BUTTON))
            }
            Part::Neutral(i) if presets && i < COLUMNS => self.cell(i, NEUTRAL_CY),
            Part::Preset(i) if presets && i < COLUMNS * TONES => {
                #[allow(clippy::cast_precision_loss, reason = "four rows")]
                let cy = GRID_CY + PITCH * (i / COLUMNS) as f32;
                self.cell(i % COLUMNS, cy)
            }
            Part::Recent(i) if presets && i < crate::recent::CAPACITY => self.cell(i, RECENT_CY),
            Part::Reset if color => self.at((PAD, FOOTER_Y, BUTTON, BUTTON)),
            Part::Reset => self.at((
                size(self.kind).0 - PAD - BUTTON,
                PREVIEW.1 + (PREVIEW.3 - BUTTON) / 2.0,
                BUTTON,
                BUTTON,
            )),
            Part::More if custom => {
                self.at((size(self.kind).0 - PAD - BUTTON, FOOTER_Y, BUTTON, BUTTON))
            }
            Part::Square if custom => self.at(SQUARE),
            Part::Hue if custom => self.at(HUE),
            Part::Track if !color => self.at(TRACK),
            Part::Panel => self.rect(),
            _ => return None,
        })
    }

    /// The preview well of the Width Picker.
    pub fn preview(&self) -> Area {
        self.at(PREVIEW)
    }

    /// Where the current color is shown in the footer of the Color Picker.
    pub fn chip(&self) -> (f32, f32) {
        let (w, _) = size(self.kind);
        let r = self.at((w / 2.0, FOOTER_Y + BUTTON / 2.0, 0.0, 0.0));
        (r.x, r.y)
    }

    /// The hairline above the footer.
    pub fn divider(&self) -> Area {
        self.at((PAD, DIVIDER_Y, size(self.kind).0 - 2.0 * PAD, 1.0))
    }

    /// The part under `at`, `Panel` for the popover's own padding, `None`
    /// outside it.
    pub fn hit(&self, at: Point) -> Option<Part> {
        if !self.rect().contains(at) {
            return None;
        }
        let part = parts().find(|&p| self.rect_of(p).is_some_and(|r| r.contains(at)));
        Some(part.unwrap_or(Part::Panel))
    }

    /// Where `at` falls in `part`'s rectangle, 0..=1 on each axis, clamped:
    /// a drag keeps tracking outside the part.
    pub fn fraction(&self, part: Part, at: Point) -> Option<(f32, f32)> {
        let r = self.rect_of(part)?;
        let f = |v: f32, start: f32, len: f32| ((v - start) / len).clamp(0.0, 1.0);
        Some((f(at.x, r.x, r.w), f(at.y, r.y, r.h)))
    }

    /// The point at fractions `(fx, fy)` of `part`, for tests: strictly
    /// inside the rectangle, so a press lands on it.
    #[cfg(feature = "test-support")]
    pub fn point_at(&self, part: Part, (fx, fy): (f32, f32)) -> Option<Point> {
        let r = self.rect_of(part)?;
        let inside = |start: f32, len: f32, f: f32| start + (f * len).min(len - 1e-3);
        Some(Point {
            x: inside(r.x, r.w, fx),
            y: inside(r.y, r.h, fy),
        })
    }
}

/// The parts a hit test visits.
fn parts() -> impl Iterator<Item = Part> {
    let tabs = Tab::ALL.into_iter().map(Part::Tab);
    let rows = (0..COLUMNS).flat_map(|i| [Part::Neutral(i), Part::Recent(i)]);
    let grid = (0..COLUMNS * TONES).map(Part::Preset);
    let singles = [
        Part::Reset,
        Part::More,
        Part::Square,
        Part::Hue,
        Part::Track,
    ];
    tabs.chain(rows).chain(grid).chain(singles)
}
