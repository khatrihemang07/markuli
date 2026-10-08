//! The style panel: stroke color and stroke width.
//!
//! A trimmed copy of Excalidraw's properties panel (an Island on the left of
//! the Overlay), in logical pixels times the scale factor. Metrics and colours
//! are Excalidraw's stock theme: 1.35 rem swatches with a 1 px border and a
//! 1 px outline 2 px outside the active one (`ColorPicker.scss`), 2 rem width
//! buttons. Deviations: only the five quick picks (no shade grid, no hex
//! input), no section titles (Markuli has no font), no opacity slider (every
//! Stroke is opaque), swatches show their literal colour in both themes
//! (Excalidraw inverts them with its canvas filter, Markuli has none), and the
//! shadow is a stack of rounded rectangles.

use crate::ink::Point;
use crate::render::{Canvas, Format};
use crate::style::{Control, PALETTE, WIDTHS};
use crate::toolbar::{fill_rounded, rounded_rect, shadow, solid_paint, Area, Theme};
use tiny_skia::{LineCap, PathBuilder, Stroke, Transform};

/// Logical pixel metrics.
const MARGIN: f32 = 16.0;
/// Narrowest Overlay (logical) that shows the panel. The panel (left edge
/// `MARGIN`, `WIDTH` wide, shadow 16 px) and the centred four-Tool toolbar
/// (shadow included) stay clear of each other down to 696; below that their
/// shadows would overlap, so the panel is hidden rather than drawn over the
/// toolbar. 720 leaves a little slack. More Tools widen the toolbar and
/// would need a larger value.
const MIN_WIDTH: f32 = 720.0;
const TOP: f32 = 72.0;
const PAD: f32 = 12.0;
const SWATCH: f32 = 22.0;
const SWATCH_GAP: f32 = 8.0;
const ROW_GAP: f32 = 16.0;
const BUTTON: f32 = 32.0;
const BUTTON_GAP: f32 = 8.0;
const RADIUS: f32 = 8.0;
/// Line thickness of the three width buttons' icons.
const ICON_LINES: [f32; 3] = [1.5, 3.0, 4.5];
/// How far the shadow reaches, for damage and clearing.
const SHADOW: [f32; 4] = [16.0, 16.0, 16.0, 24.0];

const INNER: f32 = 5.0 * SWATCH + 4.0 * SWATCH_GAP;
const WIDTH: f32 = INNER + 2.0 * PAD;
const SWATCH_Y: f32 = TOP + PAD;
const BUTTON_Y: f32 = SWATCH_Y + SWATCH + ROW_GAP;
const HEIGHT: f32 = BUTTON_Y + BUTTON - TOP + PAD;

/// What the panel shows as chosen. `None` means the Selection is mixed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PanelView {
    pub color: Option<[u8; 3]>,
    pub width: Option<f32>,
}

/// Where a press landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Press {
    /// Not on the panel.
    Miss,
    /// Panel padding: inert, but still not the canvas.
    Dead,
    Control(Control),
}

/// Everything painted last time; a difference means a repaint is due.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    view: PanelView,
    hover: Option<Control>,
    press: Option<Control>,
    theme: Theme,
    scale: f32,
    top: f32,
}

#[derive(Debug)]
pub(crate) struct Panel {
    pub theme: Theme,
    scale: f32,
    /// Physical pixels at the top that belong to the OS; the panel starts
    /// below them.
    top: f32,
    size: Option<(f32, f32)>,
    hover: Option<Control>,
    press: Option<Control>,
    /// A press on the panel's padding.
    pressed_dead: bool,
    over: bool,
    painted: Option<Look>,
    /// The region painted last time, to clear it when the panel goes away.
    shown: Option<Area>,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            scale: 1.0,
            top: 0.0,
            size: None,
            hover: None,
            press: None,
            pressed_dead: false,
            over: false,
            painted: None,
            shown: None,
        }
    }
}

impl Panel {
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    #[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
    pub fn set_top(&mut self, top: u32) {
        self.top = top as f32;
    }

    pub fn over(&self) -> bool {
        self.over
    }

    pub fn pressing(&self) -> bool {
        self.press.is_some() || self.pressed_dead
    }

    pub fn forget_pointer(&mut self) {
        (self.hover, self.press, self.pressed_dead, self.over) = (None, None, false, false);
    }

    pub fn hover_at(&mut self, at: Point) {
        let hit = self.hit(at);
        self.over = hit != Press::Miss;
        self.hover = match hit {
            Press::Control(c) => Some(c),
            _ => None,
        };
    }

    pub fn press_at(&mut self, at: Point) -> Press {
        self.hover_at(at);
        let hit = self.hit(at);
        match hit {
            Press::Control(c) => self.press = Some(c),
            Press::Dead => self.pressed_dead = true,
            Press::Miss => {}
        }
        hit
    }

    pub fn release(&mut self) {
        (self.press, self.pressed_dead) = (None, false);
    }

    /// The Overlay's size in physical pixels (its `Resize`).
    #[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = Some((width as f32, height as f32));
    }

    /// Whether the Overlay has room for the panel: clear of the centred
    /// toolbar (so their shadows never meet) and tall enough. A smaller
    /// Overlay (or none yet) shows no panel instead of a clipped one.
    pub fn fits(&self) -> bool {
        self.size.is_some_and(|(w, h)| {
            w / self.scale >= MIN_WIDTH && (h - self.top) / self.scale >= TOP + HEIGHT + MARGIN
        })
    }

    /// Logical x of the panel's left edge: Excalidraw's properties panel sits
    /// on the left edge of the canvas, under the toolbar row.
    #[allow(clippy::unused_self, reason = "kept as a method beside `origin`")]
    fn left(&self) -> f32 {
        MARGIN
    }

    /// An area given in logical pixels, `x` from the panel's left edge.
    #[allow(clippy::many_single_char_names, reason = "plain geometry")]
    fn origin(&self, x: f32, y: f32, w: f32, h: f32) -> Area {
        let s = self.scale;
        Area {
            x: (x + self.left()) * s,
            y: y * s + self.top,
            w: w * s,
            h: h * s,
        }
    }

    fn area(&self) -> Area {
        self.origin(0.0, TOP, WIDTH, HEIGHT)
    }

    /// The panel plus its shadow: the area a repaint covers.
    pub fn region(&self) -> Area {
        self.area().grow(SHADOW.map(|v| v * self.scale))
    }

    pub fn shown(&self) -> Option<Area> {
        self.shown
    }

    /// The Area of a control.
    fn rect(&self, control: Control) -> Area {
        match control {
            Control::Color(i) => self.origin(
                PAD + to_f32(i) * (SWATCH + SWATCH_GAP),
                SWATCH_Y,
                SWATCH,
                SWATCH,
            ),
            Control::Width(i) => self.origin(
                PAD + to_f32(i) * (BUTTON + BUTTON_GAP),
                BUTTON_Y,
                BUTTON,
                BUTTON,
            ),
        }
    }

    fn controls() -> impl Iterator<Item = Control> {
        let colors = (0..PALETTE.len()).map(Control::Color);
        colors.chain((0..WIDTHS.len()).map(Control::Width))
    }

    pub fn hit(&self, at: Point) -> Press {
        if let Some(c) = Self::controls().find(|&c| self.rect(c).contains(at)) {
            return Press::Control(c);
        }
        if self.area().contains(at) {
            Press::Dead
        } else {
            Press::Miss
        }
    }

    #[cfg(feature = "test-support")]
    pub fn center_of(&self, control: Control) -> Point {
        let r = self.rect(control);
        Point {
            x: r.x + r.w / 2.0,
            y: r.y + r.h / 2.0,
        }
    }

    fn look(&self, view: Option<PanelView>) -> Option<Look> {
        view.map(|view| Look {
            view,
            hover: self.hover,
            press: self.press,
            theme: self.theme,
            scale: self.scale,
            top: self.top,
        })
    }

    /// True when the pixels on screen no longer match the state.
    pub fn is_dirty(&self, view: Option<PanelView>) -> bool {
        self.painted != self.look(view)
    }

    /// Records what is now on screen.
    pub fn done(&mut self, view: Option<PanelView>) {
        self.painted = self.look(view);
        self.shown = view.map(|_| self.region());
    }

    pub fn paint(&self, view: PanelView, target: &mut Canvas<'_, '_>, format: Format) {
        let (s, colors) = (self.scale, self.theme.tokens());
        let solid = |rgb: [u8; 3]| solid_paint(rgb, 1.0, format);
        shadow(target, self.area(), s, format);
        fill_rounded(target, self.area(), RADIUS * s, &solid(colors.island));
        for (i, &color) in PALETTE.iter().enumerate() {
            let control = Control::Color(i);
            let r = self.rect(control);
            let outer = |grow: f32| Area {
                x: r.x - grow * s,
                y: r.y - grow * s,
                w: r.w + 2.0 * grow * s,
                h: r.h + 2.0 * grow * s,
            };
            fill_rounded(target, r, 4.0 * s, &solid(colors.swatch_border));
            fill_rounded(target, outer(-1.0), 3.0 * s, &solid(color));
            let ring = if view.color == Some(color) {
                Some(colors.swatch_active)
            } else if self.hover == Some(control) {
                Some(colors.swatch_border)
            } else {
                None
            };
            if let Some(ring) = ring {
                let line = Stroke {
                    width: s,
                    ..Stroke::default()
                };
                if let Some(path) = rounded_rect(outer(2.5), 5.0 * s) {
                    target.stroke_path(&path, &solid(ring), &line, Transform::identity());
                }
            }
        }
        for (i, (&width, &line_width)) in WIDTHS.iter().zip(&ICON_LINES).enumerate() {
            let control = Control::Width(i);
            let r = self.rect(control);
            let selected = view.width == Some(width);
            if selected || self.hover == Some(control) {
                let bg = if selected {
                    colors.selected
                } else {
                    colors.hover
                };
                fill_rounded(target, r, RADIUS * s, &solid(bg));
            }
            let ink = if selected {
                colors.selected_icon
            } else {
                colors.icon
            };
            let mut line = PathBuilder::new();
            let y = r.y + r.h / 2.0;
            line.move_to(r.x + 9.0 * s, y);
            line.line_to(r.x + 23.0 * s, y);
            if let Some(path) = line.finish() {
                let stroke = Stroke {
                    width: line_width * s,
                    line_cap: LineCap::Round,
                    ..Stroke::default()
                };
                target.stroke_path(&path, &solid(ink), &stroke, Transform::identity());
            }
        }
    }
}

#[allow(clippy::cast_precision_loss, reason = "a handful of controls")]
fn to_f32(v: usize) -> f32 {
    v as f32
}
