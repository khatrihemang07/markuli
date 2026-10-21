//! The Color Picker's Custom pictures and the Width Picker's slider, painted
//! with the same brush as the rest of the popover (`paint`).

use super::color;
use super::layout::{Part, KNOB};
use super::paint::{round_px, Brush, HAIRLINE};
use super::{width, Look};
use crate::palette::SlotValue;
use crate::toolbar::{push_rounded, Area, RADIUS};
use tiny_skia::{PathBuilder, Stroke};

/// The Custom pictures' corner radius, and the strip's (a pill: half of 16).
const PICTURE_RADIUS: f32 = 6.0;
const STRIP_RADIUS: f32 = 8.0;

impl Brush<'_, '_, '_> {
    pub(super) fn custom(&mut self, buffers: &mut super::Custom, look: &Look) {
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

    pub(super) fn width(&mut self, look: &Look) {
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
        self.button(&g, look, Part::Reset, &self.glyphs.reset);
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
