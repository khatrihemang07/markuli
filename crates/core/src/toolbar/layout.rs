//! Where the toolbar's islands and buttons are: pixel areas and the
//! geometry of one Overlay size and Tool count.

use super::Button;
use crate::ink::Point;
use crate::palette::{Side, COLORS, WIDTHS};

/// Logical pixel metrics from Excalidraw's CSS.
const PAD: f32 = 4.0;
const BUTTON: f32 = 32.0;
const GAP: f32 = 4.0;
const ISLAND_GAP: f32 = 8.0;
/// Distance from the edge of the usable area to the Toolbar.
const EDGE: f32 = 16.0;
/// Same at the bottom, so the shadow stays off the Dock.
const BOTTOM: f32 = 24.0;
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

/// The buttons after the widths, in row order.
const ACTION_BUTTONS: [Button; 3] = [Button::Undo, Button::Redo, Button::Clear];
const ACTIONS: usize = ACTION_BUTTONS.len();
const ISLANDS: usize = 4;

/// Buttons in the row for `tools` Tools.
pub(super) fn count(tools: usize) -> usize {
    tools + COLORS + WIDTHS + ACTIONS
}

/// Where the Toolbar sits: a row at the top or bottom, or a column at the
/// left or right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolbarPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl ToolbarPosition {
    /// Every position, in the order Settings lists them.
    pub const ALL: [Self; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];

    /// The name used in the config file.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// The parts of the Overlay's edges that belong to the OS (menu bar and
/// notch, Dock, Windows taskbar), in physical pixels. The Toolbar stays
/// clear of them; Ink can still go there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub top: u32,
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
}

impl Insets {
    /// `[left, top, right, bottom]` in physical pixels, as floats.
    fn bounds(self) -> [f32; 4] {
        [self.left, self.top, self.right, self.bottom].map(to_f32)
    }
}

/// The toolbar's geometry for one Overlay size and Tool count: four islands
/// (Tools, colors, widths, actions) in one row or column.
pub(super) struct Layout {
    /// Physical position of the bounding box's top-left corner.
    origin: (f32, f32),
    /// Where the Toolbar really sits (a column that does not fit is a row).
    position: ToolbarPosition,
    vertical: bool,
    pub scale: f32,
    sizes: [usize; ISLANDS],
}

fn island_width(n: usize) -> f32 {
    2.0 * PAD + to_f32_usize(n) * BUTTON + to_f32_usize(n.saturating_sub(1)) * GAP
}

impl Layout {
    /// `size` is the Overlay size in physical pixels. A column that does not
    /// fit between the insets (with a margin at both ends) falls back to the
    /// Top row.
    #[allow(
        clippy::many_single_char_names,
        reason = "l, t, r, b and w, h are the edges and size"
    )]
    pub fn new(
        size: (f32, f32),
        scale: f32,
        insets: Insets,
        position: ToolbarPosition,
        tools: usize,
    ) -> Self {
        let sizes = [tools, COLORS, WIDTHS, ACTIONS];
        let length = sizes.iter().map(|&n| island_width(n)).sum::<f32>()
            + ISLAND_GAP * to_f32_usize(ISLANDS - 1);
        let thick = 2.0 * PAD + BUTTON;
        let [l, t, r, b] = insets.bounds();
        let (w, h) = size;
        let column_fits = (length + 2.0 * EDGE) * scale <= h - t - b;
        let position = match position {
            ToolbarPosition::Left | ToolbarPosition::Right if !column_fits => ToolbarPosition::Top,
            p => p,
        };
        let across = |lo: f32, hi: f32| (lo + ((hi - lo) - length * scale) / 2.0).max(lo);
        let origin = match position {
            ToolbarPosition::Top => (across(l, w - r), t + EDGE * scale),
            ToolbarPosition::Bottom => (across(l, w - r), h - b - (BOTTOM + thick) * scale),
            ToolbarPosition::Left => (l + EDGE * scale, across(t, h - b)),
            ToolbarPosition::Right => (w - r - (EDGE + thick) * scale, across(t, h - b)),
        };
        Self {
            origin,
            position,
            vertical: matches!(position, ToolbarPosition::Left | ToolbarPosition::Right),
            scale,
            sizes,
        }
    }

    /// The side of a button an editor opens on: facing away from the edge.
    pub fn side(&self) -> Side {
        match self.position {
            ToolbarPosition::Top => Side::Below,
            ToolbarPosition::Bottom => Side::Above,
            ToolbarPosition::Left => Side::Right,
            ToolbarPosition::Right => Side::Left,
        }
    }

    /// Logical offset along the row or column of the start of island `g`.
    fn island_start(&self, g: usize) -> f32 {
        let before = self.sizes.iter().take(g);
        before.map(|&n| island_width(n) + ISLAND_GAP).sum::<f32>()
    }

    /// The pixel area at logical `main` offset along the toolbar and `cross`
    /// offset across it, each as (start, length).
    #[allow(
        clippy::many_single_char_names,
        reason = "x, y, w, h and the scale are plain geometry"
    )]
    fn area(&self, (main, main_len): (f32, f32), (cross, cross_len): (f32, f32)) -> Area {
        let s = self.scale;
        let (x, y, w, h) = if self.vertical {
            (cross, main, cross_len, main_len)
        } else {
            (main, cross, main_len, cross_len)
        };
        Area {
            x: self.origin.0 + x * s,
            y: self.origin.1 + y * s,
            w: w * s,
            h: h * s,
        }
    }

    pub fn button_rect(&self, i: usize) -> Area {
        let (mut g, mut j) = (0, i);
        while g + 1 < ISLANDS && j >= self.sizes[g] {
            j -= self.sizes[g];
            g += 1;
        }
        let main = self.island_start(g) + PAD + to_f32_usize(j) * (BUTTON + GAP);
        self.area((main, BUTTON), (PAD, BUTTON))
    }

    pub fn islands(&self) -> [Area; ISLANDS] {
        std::array::from_fn(|g| {
            let len = island_width(self.sizes[g]);
            self.area((self.island_start(g), len), (0.0, 2.0 * PAD + BUTTON))
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

/// The i-th button of a toolbar with `tools` Tools: Tools, colors, widths,
/// then undo, redo and Clear.
pub(super) fn button(i: usize, tools: usize) -> Button {
    let Some(i) = i.checked_sub(tools) else {
        return Button::Tool(i);
    };
    if i < COLORS {
        return Button::Color(i);
    }
    let i = i - COLORS;
    if i < WIDTHS {
        return Button::Width(i);
    }
    ACTION_BUTTONS[(i - WIDTHS).min(ACTIONS - 1)]
}

/// The position of `button` in the row, if there is such a button.
#[cfg(feature = "test-support")]
pub(super) fn index_of(button: Button, tools: usize) -> Option<usize> {
    let action = |a: Button| ACTION_BUTTONS.iter().position(|&b| b == a);
    match button {
        Button::Tool(i) => (i < tools).then_some(i),
        Button::Color(i) => (i < COLORS).then_some(tools + i),
        Button::Width(i) => (i < WIDTHS).then_some(tools + COLORS + i),
        other => action(other).map(|j| tools + COLORS + WIDTHS + j),
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
