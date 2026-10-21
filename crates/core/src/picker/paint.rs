//! Painting the popover: the island, tabs, cells, the Custom pictures and the
//! Width Picker's slider. The look follows the Toolbar's tokens: the same
//! island, shadow, radius, hover and selected fills, the same ring.

use super::color::{self, NEUTRALS, PRESETS};
use super::layout::{Geometry, Part, Tab, CELL, KNOB};
use super::{width, Look, Picker};
use crate::icons::{self, Icon};
use crate::palette::{SlotKind, SlotValue};
use crate::recent::CAPACITY;
use crate::render::{Canvas, Format};
use crate::toolbar::{fill_rounded, push_rounded, shadow, solid_paint, Area, Tokens, RADIUS};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, Rect, Stroke};

/// Icon edge in a 28 px button, logical px.
const ICON: f32 = 16.0;
/// Radii of the hover disc and the selection ring around a cell.
const DISC: f32 = 12.5;
const RING: f32 = 13.0;
/// The Custom pictures' corner radius, and the strip's (a pill: half of 16).
const PICTURE_RADIUS: f32 = 6.0;
const STRIP_RADIUS: f32 = 8.0;
/// A cell's color is 0 to 255 over the island; its outline hardly shows.
const HAIRLINE: f32 = 0.6;
/// An empty Recent Colors cell is only an outline.
const EMPTY: f32 = 0.9;

/// What every shape of one popover is painted from.
struct Brush<'a, 'c, 'd> {
    target: &'a mut Canvas<'c, 'd>,
    tokens: &'static Tokens,
    format: Format,
    s: f32,
}

impl Picker {
    pub fn paint(&mut self, target: &mut Canvas<'_, '_>, look: &Look, format: Format) {
        let g = look.geom;
        let rect = g.rect();
        let s = g.rect_scale();
        let tokens = look.theme.tokens();
        shadow(target, rect, s, format);
        fill_rounded(
            target,
            rect,
            RADIUS * s,
            &solid_paint(tokens.island, 1.0, format),
        );
        let mut brush = Brush {
            target,
            tokens,
            format,
            s,
        };
        match (g.kind, g.tab) {
            (SlotKind::Width, _) => brush.width(look),
            (SlotKind::Color, tab) => {
                brush.tabs(look);
                match tab {
                    Tab::Presets => brush.presets(look),
                    Tab::Custom => brush.custom(&mut self.custom, look),
                }
                brush.footer(look);
            }
        }
    }
}

fn circle(pb: &mut PathBuilder, cx: f32, cy: f32, r: f32) {
    const K: f32 = 0.552_284_8;
    let k = r * K;
    pb.move_to(cx + r, cy);
    pb.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
    pb.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
    pb.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
    pb.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
    pb.close();
}

fn rgb_of(value: SlotValue) -> [u8; 3] {
    match value {
        SlotValue::Color(rgb) => rgb,
        SlotValue::Width(_) => [0; 3],
    }
}

impl Brush<'_, '_, '_> {
    fn paint(&self, rgb: [u8; 3], alpha: f32) -> Paint<'static> {
        solid_paint(rgb, alpha, self.format)
    }

    fn disc(&mut self, (cx, cy): (f32, f32), r: f32, paint: &Paint<'_>) {
        self.target.fill_with(|pb| circle(pb, cx, cy, r), paint);
    }

    fn line(&mut self, (cx, cy): (f32, f32), r: f32, width: f32, paint: &Paint<'_>) {
        let stroke = Stroke {
            width: width * self.s,
            ..Stroke::default()
        };
        self.target
            .stroke_with(|pb| circle(pb, cx, cy, r), paint, &stroke);
    }

    /// A round-capped line, `width` thick (physical px).
    fn bar(&mut self, (x0, x1): (f32, f32), y: f32, width: f32, paint: &Paint<'_>) {
        let stroke = Stroke {
            width,
            line_cap: LineCap::Round,
            ..Stroke::default()
        };
        let build = |pb: &mut PathBuilder| {
            pb.move_to(x0, y);
            pb.line_to(x1, y);
        };
        self.target.stroke_with(build, paint, &stroke);
    }

    fn rounded(&mut self, r: Area, radius: f32, paint: &Paint<'_>) {
        fill_rounded(self.target, r, radius * self.s, paint);
    }

    /// A background for a pressed-looking or hovered icon button.
    fn backing(&mut self, r: Area, hot: bool, selected: bool) {
        if selected {
            let fill = self.paint(self.tokens.selected, 1.0);
            self.rounded(r, RADIUS, &fill);
        } else if hot {
            let fill = self.paint(self.tokens.hover, 1.0);
            self.rounded(r, RADIUS, &fill);
        }
    }

    fn icon(&mut self, r: Area, icon: &Icon, rgb: [u8; 3]) {
        let size = ICON * self.s;
        let at = (r.x + (r.w - size) / 2.0, r.y + (r.h - size) / 2.0, size);
        icons::draw(self.target, icon, at, &self.paint(rgb, 1.0));
    }

    /// One icon button, lit when hovered.
    fn button(&mut self, g: &Geometry, look: &Look, part: Part, icon: &Icon) {
        let Some(r) = g.rect_of(part) else { return };
        let hot = look.hover == Some(part);
        self.backing(r, hot, false);
        self.icon(r, icon, self.tokens.icon);
    }

    /// The two tabs as a segmented control.
    fn tabs(&mut self, look: &Look) {
        let g = look.geom;
        let (Some(a), Some(b)) = (
            g.rect_of(Part::Tab(Tab::Presets)),
            g.rect_of(Part::Tab(Tab::Custom)),
        ) else {
            return;
        };
        let track = Area {
            x: a.x,
            y: a.y,
            w: b.x + b.w - a.x,
            h: a.h,
        };
        let fill = self.paint(self.tokens.hover, 1.0);
        self.rounded(track, RADIUS, &fill);
        for (tab, icon) in [
            (Tab::Presets, &icons::GRID),
            (Tab::Custom, &icons::SPECTRUM),
        ] {
            let Some(r) = g.rect_of(Part::Tab(tab)) else {
                continue;
            };
            let on = g.tab == tab;
            let inner = Area {
                x: r.x + 2.0 * self.s,
                y: r.y + 2.0 * self.s,
                w: r.w - 4.0 * self.s,
                h: r.h - 4.0 * self.s,
            };
            if on {
                let fill = self.paint(self.tokens.selected, 1.0);
                self.rounded(inner, RADIUS - 2.0, &fill);
            }
            let rgb = if on {
                self.tokens.selected_icon
            } else if look.hover == Some(Part::Tab(tab)) {
                self.tokens.icon
            } else {
                self.tokens.disabled
            };
            self.icon(r, icon, rgb);
        }
    }

    /// Reset, the current color and (on Custom) "...".
    fn footer(&mut self, look: &Look) {
        let g = look.geom;
        let hairline = self.paint(self.tokens.swatch_border, HAIRLINE);
        if let Some(line) = rect(g.divider()) {
            self.target.fill_rect(line, &hairline);
        }
        self.button(&g, look, Part::Reset, &icons::RESET);
        self.button(&g, look, Part::More, &icons::MORE);
        let rgb = rgb_of(look.value);
        let at = g.chip();
        self.swatch(at, CELL / 2.0 + 1.0, rgb);
        if color::is_low(rgb, self.tokens.page) {
            self.badge((at.0 + 10.0 * self.s, at.1 + 9.0 * self.s));
        }
    }

    /// A color disc with a hairline border, `r` logical px.
    fn swatch(&mut self, at: (f32, f32), r: f32, rgb: [u8; 3]) {
        let r = r * self.s;
        let border = self.paint(self.tokens.swatch_border, 1.0);
        self.disc(at, r, &border);
        self.disc(at, r - self.s, &self.paint(rgb, 1.0));
    }

    fn cell(&mut self, g: &Geometry, part: Part, rgb: [u8; 3], look: &Look, ring: bool) {
        let Some(at) = g.center_of(part) else { return };
        if look.hover == Some(part) {
            let fill = self.paint(self.tokens.hover, 1.0);
            self.disc(at, DISC * self.s, &fill);
        }
        self.swatch(at, CELL / 2.0, rgb);
        if ring {
            let ring_paint = self.paint(self.tokens.swatch_active, 1.0);
            self.line(at, RING * self.s, 2.0, &ring_paint);
            if color::is_low(rgb, self.tokens.page) {
                self.badge((at.0 + 8.5 * self.s, at.1 + 8.5 * self.s));
            }
        }
    }

    /// The warning on a color that is hard to see on the page: a small
    /// triangle with an exclamation mark, outlined in the island's color.
    fn badge(&mut self, (cx, cy): (f32, f32)) {
        let s = self.s;
        let triangle = move |pb: &mut PathBuilder| {
            pb.move_to(cx, cy - 4.8 * s);
            pb.line_to(cx + 4.9 * s, cy + 3.7 * s);
            pb.line_to(cx - 4.9 * s, cy + 3.7 * s);
            pb.close();
        };
        let round = |width: f32| Stroke {
            width: width * s,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        let halo = self.paint(self.tokens.island, 1.0);
        self.target.stroke_with(triangle, &halo, &round(4.0));
        self.target.fill_with(triangle, &halo);
        let warn = self.paint(self.tokens.low, 1.0);
        self.target.stroke_with(triangle, &warn, &round(1.6));
        self.target.fill_with(triangle, &warn);
        let mark = self.paint([255, 255, 255], 1.0);
        let bar = move |pb: &mut PathBuilder| {
            pb.move_to(cx, cy - 1.9 * s);
            pb.line_to(cx, cy + 0.4 * s);
            pb.move_to(cx, cy + 2.2 * s);
            pb.line_to(cx, cy + 2.3 * s);
        };
        self.target.stroke_with(bar, &mark, &round(1.3));
    }

    fn presets(&mut self, look: &Look) {
        let g = look.geom;
        let current = rgb_of(look.value);
        for (i, &rgb) in NEUTRALS.iter().enumerate() {
            self.cell(&g, Part::Neutral(i), rgb, look, rgb == current);
        }
        for i in 0..PRESETS {
            let Some(rgb) = color::preset(i) else {
                continue;
            };
            // A gray the neutral row already rings is not ringed twice.
            let ring = rgb == current && !NEUTRALS.contains(&rgb);
            self.cell(&g, Part::Preset(i), rgb, look, ring);
        }
        let recent = look.recent.as_slice();
        for i in 0..CAPACITY {
            if let Some(&rgb) = recent.get(i) {
                self.cell(&g, Part::Recent(i), rgb, look, rgb == current);
            } else if let Some(at) = g.center_of(Part::Recent(i)) {
                let empty = self.paint(self.tokens.swatch_border, EMPTY);
                self.line(at, (CELL / 2.0 - 0.5) * self.s, 1.0, &empty);
            }
        }
    }

    fn custom(&mut self, buffers: &mut super::Custom, look: &Look) {
        let g = look.geom;
        let (Some(square), Some(strip)) = (g.rect_of(Part::Square), g.rect_of(Part::Hue)) else {
            return;
        };
        let size = |r: Area| (round_px(r.w), round_px(r.h));
        let s = self.s;
        let hsv = look.hsv;
        if let Some(pixels) = buffers.square(size(square), hsv.h, self.format) {
            let build = |pb: &mut PathBuilder| push_rounded(pb, square, PICTURE_RADIUS * s);
            self.target.draw_pixmap(build, pixels, (square.x, square.y));
        }
        if let Some(pixels) = buffers.strip(size(strip), self.format) {
            let build = |pb: &mut PathBuilder| push_rounded(pb, strip, STRIP_RADIUS * s);
            self.target.draw_pixmap(build, pixels, (strip.x, strip.y));
        }
        for frame in [square, strip] {
            let edge = self.paint(self.tokens.swatch_border, HAIRLINE);
            let stroke = Stroke {
                width: s,
                ..Stroke::default()
            };
            let radius = if frame.h > 20.0 * s {
                PICTURE_RADIUS
            } else {
                STRIP_RADIUS
            } * s;
            let build = |pb: &mut PathBuilder| push_rounded(pb, frame, radius);
            self.target.stroke_with(build, &edge, &stroke);
        }
        // The knobs: a white ring with a dark halo reads on any color.
        let at = (
            square.x + hsv.s * square.w,
            square.y + (1.0 - hsv.v) * square.h,
        );
        self.knob(at, 7.0, hsv.rgb());
        let half = STRIP_RADIUS * s;
        let x = (strip.x + hsv.h.rem_euclid(360.0) / 360.0 * strip.w)
            .clamp(strip.x + half, strip.x + strip.w - half);
        let pure = color::hsv_to_rgb(hsv.h, 1.0, 1.0);
        self.knob((x, strip.y + strip.h / 2.0), 9.0, pure);
    }

    /// A ring over a picture, with `rgb` inside.
    fn knob(&mut self, at: (f32, f32), r: f32, rgb: [u8; 3]) {
        let halo = self.paint([0, 0, 0], 0.35);
        self.line(at, (r + 1.0) * self.s, 1.0, &halo);
        self.disc(at, r * self.s, &self.paint(rgb, 1.0));
        let white = self.paint([255, 255, 255], 1.0);
        self.line(at, r * self.s, 2.5, &white);
    }

    fn width(&mut self, look: &Look) {
        let g = look.geom;
        let SlotValue::Width(pt) = look.value else {
            return;
        };
        let s = self.s;
        // The well is a bit of the page, so the stroke reads as it will on it.
        let well = g.preview();
        let border = self.paint(self.tokens.swatch_border, 1.0);
        self.rounded(well, RADIUS, &border);
        let inner = Area {
            x: well.x + s,
            y: well.y + s,
            w: well.w - 2.0 * s,
            h: well.h - 2.0 * s,
        };
        self.rounded(inner, RADIUS - 1.0, &self.paint(self.tokens.page, 1.0));
        let (pad, y) = (20.0 * s, well.y + well.h / 2.0);
        self.bar(
            (well.x + pad, well.x + well.w - pad),
            y,
            pt * s,
            &self.paint(look.stroke, 1.0),
        );
        self.button(&g, look, Part::Reset, &icons::RESET);
        let Some(track) = g.rect_of(Part::Track) else {
            return;
        };
        let (from, to) = width::travel(track, s);
        let cy = track.y + track.h / 2.0;
        let knob = width::knob_x(pt, track, s);
        self.bar(
            (from, to),
            cy,
            4.0 * s,
            &self.paint(self.tokens.swatch_border, 1.0),
        );
        self.bar(
            (from, knob),
            cy,
            4.0 * s,
            &self.paint(self.tokens.swatch_active, 1.0),
        );
        self.disc(
            (knob, cy + s),
            (KNOB + 1.0) * s,
            &self.paint([0, 0, 0], 0.12),
        );
        self.disc(
            (knob, cy),
            KNOB * s,
            &self.paint(self.tokens.swatch_active, 1.0),
        );
        self.disc(
            (knob, cy),
            (KNOB - 2.5) * s,
            &self.paint(self.tokens.island, 1.0),
        );
    }
}

fn rect(a: Area) -> Option<Rect> {
    Rect::from_xywh(a.x, a.y, a.w, a.h)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a positive pixel size well below 2^24"
)]
fn round_px(v: f32) -> u32 {
    v.round().max(1.0) as u32
}
