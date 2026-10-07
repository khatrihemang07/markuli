//! Ink to pixels, incrementally.
//!
//! Plain round-capped polylines for now. Only segments added since the last
//! call are drawn (standards rule 7); a full redraw happens on request.

use crate::ink::{Element, Ink, Point};
use tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Stroke, Transform};

/// Channel order of the platform's buffer. tiny-skia writes RGBA; Windows
/// layered-window DIBs are BGRA, so colours are swapped at paint time and
/// the buffer needs no conversion pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Rgba,
    Bgra,
}

/// A changed pixel region of the target, clamped to its bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Damage {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Fixed until the Excalidraw look and width choices arrive.
const INK_RGB: [u8; 3] = [0xe0, 0x31, 0x31];
const INK_WIDTH: f32 = 4.0;

/// What the next `render` must redraw.
#[derive(Debug, Default)]
pub struct Pending {
    full: bool,
}

impl Pending {
    pub fn full(&mut self) {
        self.full = true;
    }

    pub fn is_pending(&self) -> bool {
        self.full
    }
}

pub fn render(
    ink: &mut Ink,
    pending: &mut Pending,
    draw_mode: bool,
    target: &mut PixmapMut<'_>,
    format: Format,
) -> Option<Damage> {
    let (w, h) = (target.width(), target.height());
    let mut bounds = Bounds::empty();
    if pending.full {
        pending.full = false;
        // In Draw Mode the Overlay needs alpha 1 everywhere, otherwise the OS
        // sends clicks on fully transparent pixels to the apps underneath.
        target.fill(Color::from_rgba8(0, 0, 0, u8::from(draw_mode)));
        for element in ink.elements_mut() {
            element.rendered = 0;
        }
        bounds.add_rect(0.0, 0.0, to_f32(w), to_f32(h));
    }
    let paint = ink_paint(format);
    for element in ink.elements_mut() {
        draw_new_segments(element, target, &paint, &mut bounds);
    }
    bounds.damage(w, h)
}

fn ink_paint(format: Format) -> Paint<'static> {
    let [r, g, b] = INK_RGB;
    let (r, b) = if format == Format::Bgra {
        (b, r)
    } else {
        (r, b)
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, 0xff);
    paint.anti_alias = true;
    paint
}

fn draw_new_segments(
    element: &mut Element,
    target: &mut PixmapMut<'_>,
    paint: &Paint<'_>,
    bounds: &mut Bounds,
) {
    let len = element.points().len();
    let from = element.rendered;
    element.rendered = len;
    if from >= len {
        return;
    }
    let stroke = Stroke {
        width: INK_WIDTH,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    let points = element.points();
    if len == 1 {
        let at = points[0];
        if let Some(dot) = PathBuilder::from_circle(at.x, at.y, INK_WIDTH / 2.0) {
            target.fill_path(
                &dot,
                paint,
                tiny_skia::FillRule::Winding,
                Transform::identity(),
                None,
            );
            bounds.add_segment(at, at);
        }
        return;
    }
    for pair in points[from.saturating_sub(1)..].windows(2) {
        let mut pb = PathBuilder::new();
        pb.move_to(pair[0].x, pair[0].y);
        pb.line_to(pair[1].x, pair[1].y);
        if let Some(path) = pb.finish() {
            target.stroke_path(&path, paint, &stroke, Transform::identity(), None);
            bounds.add_segment(pair[0], pair[1]);
        }
    }
}

/// Union of changed rectangles, in f32 target coordinates.
struct Bounds(Option<[f32; 4]>);

impl Bounds {
    fn empty() -> Self {
        Self(None)
    }

    fn add_rect(&mut self, l: f32, t: f32, r: f32, b: f32) {
        if ![l, t, r, b].iter().all(|v| v.is_finite()) {
            return;
        }
        self.0 = Some(match self.0 {
            None => [l, t, r, b],
            Some([ol, ot, or, ob]) => [l.min(ol), t.min(ot), r.max(or), b.max(ob)],
        });
    }

    fn add_segment(&mut self, a: Point, b: Point) {
        let pad = INK_WIDTH / 2.0 + 1.0;
        self.add_rect(
            a.x.min(b.x) - pad,
            a.y.min(b.y) - pad,
            a.x.max(b.x) + pad,
            a.y.max(b.y) + pad,
        );
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "values are clamped to the target size first"
    )]
    fn damage(&self, width: u32, height: u32) -> Option<Damage> {
        let [left, top, right, bottom] = self.0?;
        let clamp = |v: f32, max: u32| v.clamp(0.0, to_f32(max)) as u32;
        let (x0, y0) = (clamp(left.floor(), width), clamp(top.floor(), height));
        let (x1, y1) = (clamp(right.ceil(), width), clamp(bottom.ceil(), height));
        (x1 > x0 && y1 > y0).then(|| Damage {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }
}

#[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
fn to_f32(v: u32) -> f32 {
    v as f32
}
