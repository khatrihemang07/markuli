//! Where the toolbar's islands and buttons are: pixel areas and the
//! geometry of one Overlay size and Tool count.

use super::Button;
use crate::ink::Point;

/// Logical pixel metrics from Excalidraw's CSS.
const PAD: f32 = 4.0;
const BUTTON: f32 = 32.0;
const GAP: f32 = 4.0;
const ISLAND_GAP: f32 = 8.0;
const TOP: f32 = 16.0;
/// How far the shadow reaches around the islands, for damage and clearing.
pub(super) const SHADOW: [f32; 4] = [16.0, 16.0, 16.0, 24.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Area {
    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.w && p.y < self.y + self.h
    }

    pub fn intersects(&self, other: &Area) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    pub fn grow(&self, [l, t, r, b]: [f32; 4]) -> Area {
        Area {
            x: self.x - l,
            y: self.y - t,
            w: self.w + l + r,
            h: self.h + t + b,
        }
    }

    pub fn union(&self, o: &Area) -> Area {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Area {
            x,
            y,
            w: (self.x + self.w).max(o.x + o.w) - x,
            h: (self.y + self.h).max(o.y + o.h) - y,
        }
    }

    pub fn from_bounds([l, t, r, b]: [f32; 4]) -> Area {
        Area {
            x: l,
            y: t,
            w: r - l,
            h: b - t,
        }
    }

    pub fn to_bounds(self) -> [f32; 4] {
        [self.x, self.y, self.x + self.w, self.y + self.h]
    }
}

/// The toolbar's geometry for one Overlay size and Tool count.
pub(super) struct Layout {
    x0: f32,
    top: f32,
    pub scale: f32,
    tools: usize,
    first_width: f32,
}

fn island_width(n: usize) -> f32 {
    2.0 * PAD + to_f32_usize(n) * BUTTON + to_f32_usize(n.saturating_sub(1)) * GAP
}

impl Layout {
    /// `width` is the Overlay width in physical pixels, `top` the physical
    /// pixels at the top that belong to the OS.
    pub fn new(width: f32, scale: f32, top: f32, tools: usize) -> Self {
        let first_width = island_width(tools);
        let total = first_width + ISLAND_GAP + island_width(3);
        Self {
            x0: ((width / scale - total) / 2.0).max(0.0),
            top,
            scale,
            tools,
            first_width,
        }
    }

    pub fn button_rect(&self, i: usize) -> Area {
        let (offset, j) = if i < self.tools {
            (PAD, i)
        } else {
            (self.first_width + ISLAND_GAP + PAD, i - self.tools)
        };
        let s = self.scale;
        Area {
            x: (self.x0 + offset + to_f32_usize(j) * (BUTTON + GAP)) * s,
            y: (TOP + PAD) * s + self.top,
            w: BUTTON * s,
            h: BUTTON * s,
        }
    }

    pub fn islands(&self) -> [Area; 2] {
        let s = self.scale;
        let height = (2.0 * PAD + BUTTON) * s;
        let island = |x: f32, n: usize| Area {
            x: x * s,
            y: TOP * s + self.top,
            w: island_width(n) * s,
            h: height,
        };
        [
            island(self.x0, self.tools),
            island(self.x0 + self.first_width + ISLAND_GAP, 3),
        ]
    }

    /// Islands plus shadow: the area a repaint covers.
    pub fn region(&self) -> Area {
        let [a, b] = self.islands();
        a.union(&b).grow(SHADOW.map(|v| v * self.scale))
    }
}

/// The i-th button of a toolbar with `tools` Tools.
pub(super) fn button(i: usize, tools: usize) -> Button {
    match i.checked_sub(tools) {
        None => Button::Tool(i),
        Some(0) => Button::Undo,
        Some(1) => Button::Redo,
        Some(_) => Button::Clear,
    }
}

#[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
pub(super) fn to_f32(v: u32) -> f32 {
    v as f32
}

#[allow(clippy::cast_precision_loss, reason = "a handful of buttons")]
fn to_f32_usize(v: usize) -> f32 {
    v as f32
}
