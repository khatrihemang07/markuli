//! Ink to pixels, incrementally.
//!
//! Each Element's outline is cached on the Element (see `ink.rs`); drawing
//! only fills it. Between full redraws, only the damaged region is redrawn
//! (standards rule 7): the region is cleared and every Element touching it is
//! filled again, in bands drawn into a small reused scratch pixmap and copied
//! into the caller's buffer. Nothing in here allocates once warm (rule 8).
//!
//! Fill follows Excalidraw (renderElement.ts, freedraw): the outline is
//! `M p0 Q p_i mid(p_i, p_i+1) ... Q p_n mid(p_n, p0) L p0 Z`, filled nonzero
//! with `strokeColor` at `globalAlpha = opacity / 100`. Deviation: Excalidraw
//! truncates path numbers to 2 decimals; we keep full f32 (at most 0.01 px).

use crate::ink::{Element, Ink, Rect};
use tiny_skia::{
    Color, FillRule, Paint, PathBuilder, PixmapMut, Transform,
};

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

/// The scratch pixmap holds at most this many bytes, so a huge damaged
/// region is drawn in bands instead of needing a screen-sized second buffer.
const BAND_BYTES: usize = 1 << 20;

/// Anti-aliasing and curve slack around changed geometry, in pixels.
const DAMAGE_PAD: f32 = 2.0;

/// What the next `render` must redraw, plus the buffers it reuses.
#[derive(Debug, Default)]
pub struct Pending {
    full: bool,
    /// Union of changed rectangles, physical `[l, t, r, b]`.
    dirty: Option<Rect>,
    scratch: Vec<u8>,
    builder: Option<PathBuilder>,
}

impl Pending {
    pub fn full(&mut self) {
        self.full = true;
    }

    pub fn is_pending(&self) -> bool {
        self.full || self.dirty.is_some()
    }

    /// Marks an element-local logical box as changed.
    pub fn damage_local(&mut self, local: Rect, origin: (f32, f32), scale: f32) {
        let [l, t, r, b] = local;
        let (x, y) = origin;
        let rect = [
            (l + x) * scale - DAMAGE_PAD,
            (t + y) * scale - DAMAGE_PAD,
            (r + x) * scale + DAMAGE_PAD,
            (b + y) * scale + DAMAGE_PAD,
        ];
        if !rect.iter().all(|v| v.is_finite()) {
            return;
        }
        self.dirty = Some(match self.dirty {
            None => rect,
            Some([ol, ot, or, ob]) => [
                rect[0].min(ol),
                rect[1].min(ot),
                rect[2].max(or),
                rect[3].max(ob),
            ],
        });
    }
}

pub fn render(
    ink: &Ink,
    pending: &mut Pending,
    draw_mode: bool,
    scale: f32,
    target: &mut PixmapMut<'_>,
    format: Format,
) -> Option<Damage> {
    let (w, h) = (target.width(), target.height());
    // In Draw Mode the Overlay needs alpha 1 everywhere, otherwise the OS
    // sends clicks on fully transparent pixels to the apps underneath.
    let backdrop = Color::from_rgba8(0, 0, 0, u8::from(draw_mode));
    let Pending {
        full,
        dirty,
        scratch,
        builder,
    } = pending;
    if *full {
        *full = false;
        *dirty = None;
        target.fill(backdrop);
        let view = Damage {
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        draw_elements(ink, builder, target, view, scale, format);
        return Some(view);
    }
    let damage = damage_of(dirty.take()?, w, h)?;
    let rows_per_band = (BAND_BYTES / (damage.width as usize * 4)).max(1);
    let mut y = damage.y;
    while y < damage.y + damage.height {
        let rows = u32::try_from(rows_per_band)
            .unwrap_or(u32::MAX)
            .min(damage.y + damage.height - y);
        let band = Damage {
            y,
            height: rows,
            ..damage
        };
        draw_band(ink, builder, scratch, target, band, backdrop, scale, format);
        y += rows;
    }
    Some(damage)
}

/// Redraws one band of the damaged region through the scratch pixmap.
#[allow(clippy::too_many_arguments, reason = "internal helper over disjoint buffers")]
fn draw_band(
    ink: &Ink,
    builder: &mut Option<PathBuilder>,
    scratch: &mut Vec<u8>,
    target: &mut PixmapMut<'_>,
    band: Damage,
    backdrop: Color,
    scale: f32,
    format: Format,
) {
    let row_bytes = band.width as usize * 4;
    let len = row_bytes * band.height as usize;
    if scratch.len() < len {
        scratch.resize(len, 0);
    }
    let Some(mut region) = PixmapMut::from_bytes(&mut scratch[..len], band.width, band.height)
    else {
        return;
    };
    region.fill(backdrop);
    draw_elements(ink, builder, &mut region, band, scale, format);
    let stride = target.width() as usize * 4;
    let data = target.data_mut();
    for (row, src) in scratch[..len].chunks_exact(row_bytes).enumerate() {
        let start = (band.y as usize + row) * stride + band.x as usize * 4;
        if let Some(dst) = data.get_mut(start..start + row_bytes) {
            dst.copy_from_slice(src);
        }
    }
}

/// Fills every Element touching `view` (target pixels) into `pm`, whose
/// top-left pixel is `view`'s top-left.
fn draw_elements(
    ink: &Ink,
    builder: &mut Option<PathBuilder>,
    pm: &mut PixmapMut<'_>,
    view: Damage,
    scale: f32,
    format: Format,
) {
    let (ox, oy) = (to_f32(view.x), to_f32(view.y));
    let (vr, vb) = (ox + to_f32(view.width), oy + to_f32(view.height));
    for element in ink.elements() {
        let Some([l, t, r, b]) = element.bounds() else {
            continue;
        };
        let (ex, ey) = (element.x(), element.y());
        let touches = (l + ex) * scale - DAMAGE_PAD < vr
            && (r + ex) * scale + DAMAGE_PAD > ox
            && (t + ey) * scale - DAMAGE_PAD < vb
            && (b + ey) * scale + DAMAGE_PAD > oy;
        if touches {
            let to_pixels = Transform::from_row(scale, 0.0, 0.0, scale, ex * scale - ox, ey * scale - oy);
            fill_element(element, builder, pm, to_pixels, format);
        }
    }
}

fn fill_element(
    element: &Element,
    builder: &mut Option<PathBuilder>,
    pm: &mut PixmapMut<'_>,
    transform: Transform,
    format: Format,
) {
    let outline = element.outline();
    if outline.len() < 3 {
        return;
    }
    let mut path = builder.take().unwrap_or_default();
    let mid = |a: [f32; 2], b: [f32; 2]| ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0);
    path.move_to(outline[0][0], outline[0][1]);
    for (i, &p) in outline.iter().enumerate() {
        let next = outline[(i + 1) % outline.len()];
        let (mx, my) = mid(p, next);
        path.quad_to(p[0], p[1], mx, my);
    }
    path.line_to(outline[0][0], outline[0][1]);
    path.close();
    if let Some(path) = path.finish() {
        pm.fill_path(
            &path,
            &ink_paint(element, format),
            FillRule::Winding,
            transform,
            None,
        );
        *builder = Some(path.clear());
    }
}

fn ink_paint(element: &Element, format: Format) -> Paint<'static> {
    let [r, g, b] = element.stroke_color();
    let (r, b) = if format == Format::Bgra { (b, r) } else { (r, b) };
    let alpha = u16::from(element.opacity().min(100)) * 255 / 100;
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, u8::try_from(alpha).unwrap_or(255));
    paint.anti_alias = true;
    paint
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "values are clamped to the target size first"
)]
fn damage_of([left, top, right, bottom]: Rect, width: u32, height: u32) -> Option<Damage> {
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

#[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
fn to_f32(v: u32) -> f32 {
    v as f32
}
