//! The Excalidraw-style toolbar: layout, hit-testing and painting.
//!
//! Two islands at the top centre of the Overlay, in Draw Mode only: the
//! Tool buttons, then undo, redo and Clear. Metrics and colours are
//! Excalidraw's stock theme (`theme.scss`, `ToolIcon.scss`, `Island.scss`),
//! in logical pixels times the Overlay's scale factor. Deviations: the hint
//! digits are stroked glyphs (no font), the shadow is a stack of rounded
//! rectangles instead of a blur, and there is no pressed-state border.

use crate::icons::{self, Icon};
use crate::ink::Point;
use crate::render::Format;
use crate::tools::Tools;
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, PixmapMut, Transform};

/// The OS light or dark theme, an input to the core.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// A toolbar button. `Tool(i)` is the i-th registered Tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Tool(usize),
    Undo,
    Redo,
    Clear,
}

/// Logical pixel metrics from Excalidraw's CSS.
const PAD: f32 = 4.0;
const BUTTON: f32 = 32.0;
const GAP: f32 = 4.0;
const ISLAND_GAP: f32 = 8.0;
const TOP: f32 = 16.0;
const RADIUS: f32 = 8.0;
const ICON: f32 = 16.0;
/// How far the shadow reaches around the islands, for damage and clearing.
const SHADOW: [f32; 4] = [16.0, 16.0, 16.0, 24.0];

struct Palette {
    island: [u8; 3],
    icon: [u8; 3],
    hover: [u8; 3],
    selected: [u8; 3],
    selected_icon: [u8; 3],
    disabled: [u8; 3],
    hint: [u8; 3],
}

const LIGHT: Palette = Palette {
    island: [0xff, 0xff, 0xff],
    icon: [0x1b, 0x1b, 0x1f],
    hover: [0xf1, 0xf0, 0xff],
    selected: [0xe0, 0xdf, 0xff],
    selected_icon: [0x03, 0x00, 0x64],
    disabled: [0xb8, 0xb8, 0xb8],
    hint: [0xb8, 0xb8, 0xb8],
};

const DARK: Palette = Palette {
    island: [0x23, 0x23, 0x29],
    icon: [0xe3, 0xe3, 0xe8],
    hover: [0x32, 0x30, 0x39],
    selected: [0x40, 0x3e, 0x6a],
    selected_icon: [0xe0, 0xdf, 0xff],
    disabled: [0x5c, 0x5c, 0x5c],
    hint: [0x7a, 0x7a, 0x7a],
};

/// What decides how the toolbar looks besides the Toolbar's own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UiState {
    pub active: usize,
    pub can_undo: bool,
    pub can_redo: bool,
    pub has_ink: bool,
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
}

/// Where a pointer position lands on the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Button(Button),
    /// Island padding: inert, but still not the canvas.
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Area {
    fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.w && p.y < self.y + self.h
    }

    pub fn intersects(&self, other: &Area) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    fn grow(&self, [l, t, r, b]: [f32; 4]) -> Area {
        Area {
            x: self.x - l,
            y: self.y - t,
            w: self.w + l + r,
            h: self.h + t + b,
        }
    }

    fn union(&self, o: &Area) -> Area {
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

#[derive(Debug)]
pub(crate) struct Toolbar {
    pub theme: Theme,
    /// Overlay size in physical pixels, and its scale factor.
    size: Option<(f32, f32)>,
    scale: f32,
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
            hover: None,
            press: None,
            over: false,
            painted: None,
        }
    }
}

/// The toolbar's geometry for one Overlay size and Tool count.
struct Layout {
    x0: f32,
    scale: f32,
    tools: usize,
    first_width: f32,
}

impl Toolbar {
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = Some((to_f32(width), to_f32(height)));
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
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
        let (width, _) = self.size?;
        let scale = self.scale;
        let island = |n: usize| {
            2.0 * PAD + to_f32_usize(n) * BUTTON + to_f32_usize(n.saturating_sub(1)) * GAP
        };
        let first_width = island(tools);
        let total = first_width + ISLAND_GAP + island(3);
        Some(Layout {
            x0: ((width / scale - total) / 2.0).max(0.0),
            scale,
            tools,
            first_width,
        })
    }

    fn button_rect(l: &Layout, i: usize) -> Area {
        let (offset, j) = if i < l.tools {
            (PAD, i)
        } else {
            (l.first_width + ISLAND_GAP + PAD, i - l.tools)
        };
        let s = l.scale;
        Area {
            x: (l.x0 + offset + to_f32_usize(j) * (BUTTON + GAP)) * s,
            y: (TOP + PAD) * s,
            w: BUTTON * s,
            h: BUTTON * s,
        }
    }

    fn islands(l: &Layout) -> [Area; 2] {
        let s = l.scale;
        let height = (2.0 * PAD + BUTTON) * s;
        let island = |x: f32, n: usize| Area {
            x: x * s,
            y: TOP * s,
            w: (2.0 * PAD + to_f32_usize(n) * BUTTON + to_f32_usize(n.saturating_sub(1)) * GAP) * s,
            h: height,
        };
        [
            island(l.x0, l.tools),
            island(l.x0 + l.first_width + ISLAND_GAP, 3),
        ]
    }

    fn button(i: usize, tools: usize) -> Button {
        match i.checked_sub(tools) {
            None => Button::Tool(i),
            Some(0) => Button::Undo,
            Some(1) => Button::Redo,
            Some(_) => Button::Clear,
        }
    }

    pub fn hit(&self, at: Point, tools: usize) -> Option<Hit> {
        let l = self.layout(tools)?;
        for i in 0..tools + 3 {
            if Self::button_rect(&l, i).contains(at) {
                return Some(Hit::Button(Self::button(i, tools)));
            }
        }
        Self::islands(&l)
            .iter()
            .any(|r| r.contains(at))
            .then_some(Hit::Dead)
    }

    pub fn center_of(&self, button: Button, tools: usize) -> Option<Point> {
        let l = self.layout(tools)?;
        let i = match button {
            Button::Tool(i) if i < tools => i,
            Button::Tool(_) => return None,
            Button::Undo => tools,
            Button::Redo => tools + 1,
            Button::Clear => tools + 2,
        };
        let r = Self::button_rect(&l, i);
        Some(Point {
            x: r.x + r.w / 2.0,
            y: r.y + r.h / 2.0,
        })
    }

    /// Islands plus shadow: the area a repaint covers.
    fn region(&self, tools: usize) -> Option<Area> {
        let l = self.layout(tools)?;
        let [a, b] = Self::islands(&l);
        let s = l.scale;
        Some(a.union(&b).grow(SHADOW.map(|v| v * s)))
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
            self.toolbar.region(self.tools.len())
        } else {
            None
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.toolbar.is_dirty(self.visible, self.ui)
    }

    pub fn paint(&self, target: &mut PixmapMut<'_>, format: Format) {
        let (toolbar, tools) = (&*self.toolbar, self.tools);
        let Some(l) = toolbar.layout(tools.len()) else {
            return;
        };
        let s = l.scale;
        let colors = match toolbar.theme {
            Theme::Light => &LIGHT,
            Theme::Dark => &DARK,
        };
        let solid = |rgb: [u8; 3]| solid_paint(rgb, 1.0, format);
        for island in Toolbar::islands(&l) {
            shadow(target, island, s, format);
            fill_rounded(target, island, RADIUS * s, &solid(colors.island));
        }
        for i in 0..tools.len() + 3 {
            let rect = Toolbar::button_rect(&l, i);
            let button = Toolbar::button(i, tools.len());
            let selected = button == Button::Tool(self.ui.active);
            let hot = toolbar.hover == Some(button) || toolbar.press == Some(button);
            if selected || hot {
                let bg = if selected {
                    colors.selected
                } else {
                    colors.hover
                };
                fill_rounded(target, rect, RADIUS * s, &solid(bg));
            }
            let enabled = match button {
                Button::Tool(_) => true,
                Button::Undo => self.ui.can_undo,
                Button::Redo => self.ui.can_redo,
                Button::Clear => self.ui.has_ink,
            };
            let ink = match (selected, enabled) {
                (true, _) => colors.selected_icon,
                (false, true) => colors.icon,
                (false, false) => colors.disabled,
            };
            let icon: &Icon = match button {
                Button::Tool(t) => match tools.get(t) {
                    Some(tool) => tool.icon(),
                    None => continue,
                },
                Button::Undo => &icons::UNDO,
                Button::Redo => &icons::REDO,
                Button::Clear => &icons::TRASH,
            };
            let size = ICON * s;
            let at = (
                rect.x + (rect.w - size) / 2.0,
                rect.y + (rect.h - size) / 2.0,
                size,
            );
            icons::draw(target, icon, at, &solid(ink));
            if let Button::Tool(t) = button {
                let digit = tools
                    .get(t)
                    .and_then(|tool| tool.keys().iter().copied().find(char::is_ascii_digit));
                if let Some(d) = digit {
                    let corner = (
                        rect.x + rect.w - 4.0 * s,
                        rect.y + rect.h - 4.0 * s,
                        7.0 * s,
                    );
                    icons::draw_digit(target, d, corner, &solid(colors.hint));
                }
            }
        }
    }

    /// Records what is now on screen.
    pub fn done(&mut self) {
        self.toolbar.painted = self.toolbar.look(self.visible, self.ui);
    }
}

/// Stock `--shadow-island`, approximated by stacked translucent rounded
/// rectangles, outermost first: (grow, offset down, alpha), logical pixels.
fn shadow(target: &mut PixmapMut<'_>, island: Area, s: f32, format: Format) {
    const LAYERS: [(f32, f32, f32); 6] = [
        (13.0, 7.0, 0.010),
        (10.0, 6.0, 0.015),
        (7.0, 4.0, 0.020),
        (4.0, 2.0, 0.025),
        (1.5, 0.0, 0.050),
        (0.5, 0.0, 0.090),
    ];
    for (grow, dy, alpha) in LAYERS {
        let r = Area {
            x: island.x - grow * s,
            y: island.y - grow * s + dy * s,
            w: island.w + 2.0 * grow * s,
            h: island.h + 2.0 * grow * s,
        };
        fill_rounded(
            target,
            r,
            (RADIUS + grow) * s,
            &solid_paint([0, 0, 0], alpha, format),
        );
    }
}

fn solid_paint(rgb: [u8; 3], alpha: f32, format: Format) -> Paint<'static> {
    let [r, g, b] = if format == Format::Bgra {
        [rgb[2], rgb[1], rgb[0]]
    } else {
        rgb
    };
    let mut paint = Paint::default();
    let mut color = Color::from_rgba8(r, g, b, 255);
    color.apply_opacity(alpha);
    paint.set_color(color);
    paint.anti_alias = true;
    paint
}

fn fill_rounded(target: &mut PixmapMut<'_>, r: Area, radius: f32, paint: &Paint<'_>) {
    if let Some(path) = rounded_rect(r, radius) {
        target.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
    }
}

/// A rounded rectangle from four cubic corner arcs.
fn rounded_rect(r: Area, radius: f32) -> Option<Path> {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
    let k = radius * (1.0 - 0.552_284_8);
    let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
    let mut pb = PathBuilder::new();
    pb.move_to(x0 + radius, y0);
    pb.line_to(x1 - radius, y0);
    pb.cubic_to(x1 - k, y0, x1, y0 + k, x1, y0 + radius);
    pb.line_to(x1, y1 - radius);
    pb.cubic_to(x1, y1 - k, x1 - k, y1, x1 - radius, y1);
    pb.line_to(x0 + radius, y1);
    pb.cubic_to(x0 + k, y1, x0, y1 - k, x0, y1 - radius);
    pb.line_to(x0, y0 + radius);
    pb.cubic_to(x0, y0 + k, x0 + k, y0, x0 + radius, y0);
    pb.close();
    pb.finish()
}

#[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
fn to_f32(v: u32) -> f32 {
    v as f32
}

#[allow(clippy::cast_precision_loss, reason = "a handful of buttons")]
fn to_f32_usize(v: usize) -> f32 {
    v as f32
}
