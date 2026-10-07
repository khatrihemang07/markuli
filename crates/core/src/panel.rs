//! The style panel: stroke color, stroke width and opacity.
//!
//! A trimmed copy of Excalidraw's properties panel (an Island on the left of
//! the Overlay), in logical pixels times the scale factor. Metrics and colours
//! are Excalidraw's stock theme: 1.35 rem swatches with a 1 px border and a
//! 1 px outline 2 px outside the active one (`ColorPicker.scss`), 2 rem width
//! buttons, a 4 px range track with a 16 px thumb and the value under it
//! (`Range.scss`). Deviations: only the five quick picks (no shade grid, no
//! hex input), no section titles or bubble (Markuli has no font; digits are
//! stroked glyphs), swatches show their literal colour in both themes
//! (Excalidraw inverts them with its canvas filter, Markuli has none), and the
//! shadow is a stack of rounded rectangles.

use crate::icons;
use crate::ink::Point;
use crate::render::Format;
use crate::style::{Control, OPACITY_STEP, PALETTE, WIDTHS};
use crate::toolbar::{fill_rounded, rounded_rect, shadow, solid_paint, Area, Theme};
use tiny_skia::{FillRule, LineCap, PathBuilder, PixmapMut, Stroke, Transform};

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
const THUMB: f32 = 16.0;
const TRACK: f32 = 4.0;
const DIGITS: f32 = 9.0;
const DIGIT_GAP: f32 = 6.0;
const RADIUS: f32 = 8.0;
/// Line thickness of the three width buttons' icons.
const ICON_LINES: [f32; 3] = [1.5, 3.0, 4.5];
/// How far the shadow reaches, for damage and clearing.
const SHADOW: [f32; 4] = [16.0, 16.0, 16.0, 24.0];

const INNER: f32 = 5.0 * SWATCH + 4.0 * SWATCH_GAP;
const WIDTH: f32 = INNER + 2.0 * PAD;
const SWATCH_Y: f32 = TOP + PAD;
const BUTTON_Y: f32 = SWATCH_Y + SWATCH + ROW_GAP;
const SLIDER_Y: f32 = BUTTON_Y + BUTTON + ROW_GAP;
const DIGITS_BOTTOM: f32 = SLIDER_Y + THUMB + 4.0 + DIGITS;
const HEIGHT: f32 = DIGITS_BOTTOM - TOP + PAD;

/// What the panel shows as chosen. `None` means the Selection is mixed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PanelView {
    pub color: Option<[u8; 3]>,
    pub width: Option<f32>,
    pub opacity: u8,
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
}

#[derive(Debug)]
pub(crate) struct Panel {
    pub theme: Theme,
    scale: f32,
    size: Option<(f32, f32)>,
    /// Hover and press ignore the slider position (they are normalised to
    /// `Opacity(0)`), so moving along the slider repaints nothing extra.
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
            Press::Control(c) => Some(normal(c)),
            _ => None,
        };
    }

    pub fn press_at(&mut self, at: Point) -> Press {
        self.hover_at(at);
        let hit = self.hit(at);
        match hit {
            Press::Control(c) => self.press = Some(normal(c)),
            Press::Dead => self.pressed_dead = true,
            Press::Miss => {}
        }
        hit
    }

    /// While the slider is held, the opacity under the pointer (even when the
    /// pointer has left the panel).
    pub fn drag_to(&mut self, at: Point) -> Option<Control> {
        self.hover_at(at);
        matches!(self.press, Some(Control::Opacity(_)))
            .then(|| Control::Opacity(self.opacity_at(at.x)))
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
            w / self.scale >= MIN_WIDTH && h / self.scale >= TOP + HEIGHT + MARGIN
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
            y: y * s,
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

    fn slider(&self) -> Area {
        self.origin(PAD, SLIDER_Y, INNER, THUMB)
    }

    /// The Area of a control; the slider's is the whole track zone.
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
            Control::Opacity(_) => self.slider(),
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
        if self.slider().contains(at) {
            return Press::Control(Control::Opacity(self.opacity_at(at.x)));
        }
        if self.area().contains(at) {
            Press::Dead
        } else {
            Press::Miss
        }
    }

    /// The thumb centre travels the track minus one thumb width.
    fn thumb_x(&self, opacity: u8) -> f32 {
        let slider = self.slider();
        let travel = slider.w - THUMB * self.scale;
        slider.x + THUMB * self.scale / 2.0 + travel * f32::from(opacity.min(100)) / 100.0
    }

    fn opacity_at(&self, x: f32) -> u8 {
        let slider = self.slider();
        let travel = slider.w - THUMB * self.scale;
        let t = ((x - slider.x - THUMB * self.scale / 2.0) / travel).clamp(0.0, 1.0);
        let step = f32::from(OPACITY_STEP);
        round_to_u8((t * 100.0 / step).round() * step)
    }

    #[cfg(feature = "test-support")]
    pub fn center_of(&self, control: Control) -> Point {
        let r = self.rect(control);
        let x = match control {
            Control::Opacity(v) => self.thumb_x(v / OPACITY_STEP * OPACITY_STEP),
            _ => r.x + r.w / 2.0,
        };
        Point {
            x,
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

    pub fn paint(&self, view: PanelView, target: &mut PixmapMut<'_>, format: Format) {
        let (s, colors) = (self.scale, self.theme.palette());
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
                    target.stroke_path(&path, &solid(ring), &line, Transform::identity(), None);
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
                target.stroke_path(&path, &solid(ink), &stroke, Transform::identity(), None);
            }
        }
        self.paint_slider(view.opacity, target, format);
    }

    fn paint_slider(&self, opacity: u8, target: &mut PixmapMut<'_>, format: Format) {
        let (s, colors) = (self.scale, self.theme.palette());
        let solid = |rgb: [u8; 3]| solid_paint(rgb, 1.0, format);
        let zone = self.slider();
        let cy = zone.y + zone.h / 2.0;
        let cx = self.thumb_x(opacity);
        let track = Area {
            x: zone.x,
            y: cy - TRACK * s / 2.0,
            w: zone.w,
            h: TRACK * s,
        };
        fill_rounded(target, track, TRACK * s / 2.0, &solid(colors.track_rest));
        let filled = Area {
            w: cx - zone.x,
            ..track
        };
        fill_rounded(target, filled, TRACK * s / 2.0, &solid(colors.track_fill));
        if let Some(thumb) = PathBuilder::from_circle(cx, cy, THUMB * s / 2.0) {
            target.fill_path(
                &thumb,
                &solid(colors.thumb),
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }
        // The value under the thumb, hidden at 0 like Excalidraw's bubble.
        if opacity > 0 {
            let text = opacity.to_string();
            let width = DIGIT_GAP * s * to_f32(text.len());
            let mut right = cx - width / 2.0 + (DIGIT_GAP - 1.0) * s;
            for c in text.chars() {
                let corner = (right, (DIGITS_BOTTOM) * s, DIGITS * s);
                icons::draw_digit(target, c, corner, &solid(colors.icon));
                right += DIGIT_GAP * s;
            }
        }
    }
}

/// A press or hover on the slider, whatever the value.
fn normal(c: Control) -> Control {
    match c {
        Control::Opacity(_) => Control::Opacity(0),
        other => other,
    }
}

#[allow(clippy::cast_precision_loss, reason = "a handful of controls")]
fn to_f32(v: usize) -> f32 {
    v as f32
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is a multiple of 10 between 0 and 100"
)]
fn round_to_u8(v: f32) -> u8 {
    v.clamp(0.0, 100.0) as u8
}
