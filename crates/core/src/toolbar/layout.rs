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

/// Colours and widths in the row: the palette and the width presets.
pub(super) const COLORS: usize = 5;
pub(super) const WIDTHS: usize = 3;
/// Undo, redo, Clear.
const ACTIONS: usize = 3;
const ISLANDS: usize = 4;

/// Buttons in the row for `tools` Tools.
pub(super) fn count(tools: usize) -> usize {
    tools + COLORS + WIDTHS + ACTIONS
}

/// The toolbar's geometry for one Overlay size and Tool count: four islands
/// (Tools, colours, widths, actions) in one row.
pub(super) struct Layout {
    x0: f32,
    top: f32,
    pub scale: f32,
    sizes: [usize; ISLANDS],
}

fn island_width(n: usize) -> f32 {
    2.0 * PAD + to_f32_usize(n) * BUTTON + to_f32_usize(n.saturating_sub(1)) * GAP
}

impl Layout {
    /// `width` is the Overlay width in physical pixels, `top` the physical
    /// pixels at the top that belong to the OS.
    pub fn new(width: f32, scale: f32, top: f32, tools: usize) -> Self {
        let sizes = [tools, COLORS, WIDTHS, ACTIONS];
        let total: f32 = sizes.iter().map(|&n| island_width(n)).sum::<f32>()
            + ISLAND_GAP * to_f32_usize(ISLANDS - 1);
        Self {
            x0: ((width / scale - total) / 2.0).max(0.0),
            top,
            scale,
            sizes,
        }
    }

    /// Logical x of the left edge of island `g`.
    fn island_x(&self, g: usize) -> f32 {
        let before = self.sizes.iter().take(g);
        self.x0 + before.map(|&n| island_width(n) + ISLAND_GAP).sum::<f32>()
    }

    pub fn button_rect(&self, i: usize) -> Area {
        let (mut g, mut j) = (0, i);
        while g + 1 < ISLANDS && j >= self.sizes[g] {
            j -= self.sizes[g];
            g += 1;
        }
        let s = self.scale;
        Area {
            x: (self.island_x(g) + PAD + to_f32_usize(j) * (BUTTON + GAP)) * s,
            y: (TOP + PAD) * s + self.top,
            w: BUTTON * s,
            h: BUTTON * s,
        }
    }

    pub fn islands(&self) -> [Area; ISLANDS] {
        let s = self.scale;
        let height = (2.0 * PAD + BUTTON) * s;
        std::array::from_fn(|g| Area {
            x: self.island_x(g) * s,
            y: TOP * s + self.top,
            w: island_width(self.sizes[g]) * s,
            h: height,
        })
    }

    /// Islands plus shadow: the area a repaint covers.
    pub fn region(&self) -> Area {
        let [first, rest @ ..] = self.islands();
        rest.iter()
            .fold(first, |all, r| all.union(r))
            .grow(SHADOW.map(|v| v * self.scale))
    }
}

/// The i-th button of a toolbar with `tools` Tools: Tools, colours, widths,
/// then undo, redo and Clear.
pub(super) fn button(i: usize, tools: usize) -> Button {
    let Some(i) = i.checked_sub(tools) else {
        return Button::Tool(i);
    };
    match i {
        0..COLORS => Button::Color(i),
        _ => match i - COLORS {
            j @ 0..WIDTHS => Button::Width(j),
            j => match j - WIDTHS {
                0 => Button::Undo,
                1 => Button::Redo,
                _ => Button::Clear,
            },
        },
    }
}

/// The position of `button` in the row, if there is such a button.
#[cfg(feature = "test-support")]
pub(super) fn index_of(button: Button, tools: usize) -> Option<usize> {
    match button {
        Button::Tool(i) => (i < tools).then_some(i),
        Button::Color(i) => (i < COLORS).then_some(tools + i),
        Button::Width(i) => (i < WIDTHS).then_some(tools + COLORS + i),
        Button::Undo => Some(tools + COLORS + WIDTHS),
        Button::Redo => Some(tools + COLORS + WIDTHS + 1),
        Button::Clear => Some(tools + COLORS + WIDTHS + 2),
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
