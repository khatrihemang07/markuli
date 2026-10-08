//! Ink to pixels, incrementally.
//!
//! Each Element's outline is cached on the Element (see `ink.rs`); drawing
//! only fills it. Between full redraws, only the damaged region is redrawn
//! (standards rule 7): the region is cleared and every Element touching it is
//! filled again. Every layer of the frame (backdrop, Ink, Laser, Selection,
//! toolbar, style panel) is composed in a small reused scratch pixmap, one band
//! at a time, and each finished band is copied into the caller's buffer in one
//! pass, so the buffer never holds a half-painted frame (ADR-0003). Nothing in
//! here allocates once warm (rule 8).
//!
//! Fill follows Excalidraw (renderElement.ts, freedraw): the outline is
//! `M p0 Q p_i mid(p_i, p_i+1) ... Q p_n mid(p_n, p0) L p0 Z`, filled nonzero
//! with `strokeColor` at `globalAlpha = opacity / 100`. Deviation: Excalidraw
//! truncates path numbers to 2 decimals; we keep full f32 (at most 0.01 px).

use crate::ink::{Element, Ink, Rect};
use crate::laser::Laser;
use crate::selection::Selection;
use crate::toolbar::{Area, Chrome};
use tiny_skia::{
    Color, FillRule, Paint, Path, PathBuilder, PixmapMut, Rect as SkiaRect, Stroke, Transform,
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

/// A target seen from a band: drawing code works in target pixel coordinates,
/// the canvas shifts them into the band's own pixmap.
pub(crate) struct Canvas<'a, 'b> {
    pm: &'a mut PixmapMut<'b>,
    shift: Transform,
}

impl<'a, 'b> Canvas<'a, 'b> {
    /// `pm`'s top-left pixel is the target pixel `(x, y)`.
    fn new(pm: &'a mut PixmapMut<'b>, x: u32, y: u32) -> Self {
        Self {
            pm,
            shift: Transform::from_translate(-to_f32(x), -to_f32(y)),
        }
    }

    pub fn fill_path(&mut self, path: &Path, paint: &Paint<'_>, rule: FillRule) {
        self.pm.fill_path(path, paint, rule, self.shift, None);
    }

    /// `at` maps the path into target pixels first.
    pub fn stroke_path(&mut self, path: &Path, paint: &Paint<'_>, stroke: &Stroke, at: Transform) {
        let to_band = at.post_concat(self.shift);
        self.pm.stroke_path(path, paint, stroke, to_band, None);
    }

    pub fn fill_rect(&mut self, rect: SkiaRect, paint: &Paint<'_>) {
        self.pm.fill_rect(rect, paint, self.shift, None);
    }
}

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

    /// Whether the pending damage overlaps `rect` (physical `[l, t, r, b]`).
    pub fn touches(&self, rect: Rect) -> bool {
        self.full
            || self.dirty.is_some_and(|[l, t, r, b]| {
                l < rect[2] && rect[0] < r && t < rect[3] && rect[1] < b
            })
    }

    /// The region the next frame redraws (all of the target after a reset),
    /// clamped to the target; the pending state is cleared.
    fn take_damage(&mut self, width: u32, height: u32) -> Option<Damage> {
        if self.full {
            self.full = false;
            self.dirty = None;
            return Some(Damage {
                x: 0,
                y: 0,
                width,
                height,
            });
        }
        damage_of(self.dirty.take()?, width, height)
    }

    /// Marks a physical box as changed.
    pub fn add_physical(&mut self, rect: Rect) {
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

    /// Marks an element-local logical box as changed.
    pub fn damage_local(&mut self, local: Rect, origin: (f32, f32), scale: f32) {
        let [left, top, right, bottom] = local;
        let (ox, oy) = origin;
        let rect = [
            (left + ox) * scale - DAMAGE_PAD,
            (top + oy) * scale - DAMAGE_PAD,
            (right + ox) * scale + DAMAGE_PAD,
            (bottom + oy) * scale + DAMAGE_PAD,
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

/// Renders the Ink, then the toolbar on top of it.
///
/// The toolbar is never drawn incrementally: whenever it changed, or the Ink
/// redraw touches its region, the whole region is redrawn from the backdrop
/// and the Ink, then the toolbar is painted over it (so its shadow never
/// stacks on itself).
#[allow(
    clippy::too_many_arguments,
    reason = "one call site; each is a distinct layer"
)]
pub fn render(
    ink: &Ink,
    laser: &Laser,
    selection: &Selection,
    pending: &mut Pending,
    draw_mode: bool,
    scale: f32,
    target: &mut PixmapMut<'_>,
    format: Format,
    mut chrome: Chrome<'_>,
) -> Option<Damage> {
    let region = chrome.region();
    let dirty = chrome.is_dirty();
    // The Selection overlay is painted as a whole, so any redraw touching it
    // redraws all of it. Done again after the toolbar, whose region may bring
    // in more damage.
    grow_to_overlay(pending, selection);
    match region {
        // The toolbar vanished (Draw Mode ended): that is always a full redraw.
        None if dirty => pending.full(),
        Some(r) if dirty || pending.touches(r.to_bounds()) => {
            pending.add_physical(r.to_bounds());
        }
        _ => {}
    }
    // The style panel is repainted whole too: when it changed, the area it
    // left and the area it takes are damage.
    let panel = chrome.panel_region();
    if chrome.panel_dirty() {
        for area in [chrome.panel_shown(), panel].into_iter().flatten() {
            pending.add_physical(area.to_bounds());
        }
    }
    // Regions that overlap one another must be redrawn together, or a shadow
    // would stack on itself; two passes reach every overlap of two regions.
    for _ in 0..2 {
        for r in [region, panel].into_iter().flatten() {
            if pending.touches(r.to_bounds()) {
                pending.add_physical(r.to_bounds());
            }
        }
        grow_to_overlay(pending, selection);
    }
    let Some(damage) = pending.take_damage(target.width(), target.height()) else {
        chrome.done();
        return None;
    };
    let layers = Layers {
        ink,
        laser,
        selection,
        draw_mode,
        scale,
        format,
    };
    let rows_per_band = (BAND_BYTES / (damage.width as usize * 4).max(1)).max(1);
    let rows_per_band = u32::try_from(rows_per_band).unwrap_or(u32::MAX);
    let mut y = damage.y;
    while y < damage.y + damage.height {
        let rows = rows_per_band.min(damage.y + damage.height - y);
        let band = Damage {
            y,
            height: rows,
            ..damage
        };
        compose_band(&layers, pending, &mut chrome, target, band);
        y += rows;
    }
    chrome.done();
    Some(damage)
}

fn grow_to_overlay(pending: &mut Pending, selection: &Selection) {
    if let Some(area) = selection.shown() {
        if pending.touches(area) {
            pending.add_physical(area);
        }
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// What a frame is made of, bottom to top: backdrop, Ink and Laser, Selection
/// overlay, then (chrome, below) the toolbar and the style panel.
struct Layers<'a> {
    ink: &'a Ink,
    laser: &'a Laser,
    selection: &'a Selection,
    draw_mode: bool,
    scale: f32,
    format: Format,
}

/// Composes every layer of one band of the damaged region in the scratch
/// pixmap, then copies the finished band into the target in one pass.
///
/// The target is what the screen shows (on macOS the displayed `IOSurface`), and
/// the compositor reads it at any moment. Painting layer after layer into it
/// let a sample catch the band cleared and the toolbar or style panel not yet
/// painted: a flicker while hovering them (ADR-0003). Here each target pixel
/// is written once, with its final value, so no partial layer state is ever
/// visible.
fn compose_band(
    layers: &Layers<'_>,
    pending: &mut Pending,
    chrome: &mut Chrome<'_>,
    target: &mut PixmapMut<'_>,
    band: Damage,
) {
    let Layers {
        ink,
        laser,
        selection,
        draw_mode,
        scale,
        format,
    } = *layers;
    let row_bytes = band.width as usize * 4;
    let len = row_bytes * band.height as usize;
    let Pending {
        scratch, builder, ..
    } = pending;
    if scratch.len() < len {
        scratch.resize(len, 0);
    }
    let Some(bytes) = scratch.get_mut(..len) else {
        return;
    };
    let Some(mut region) = PixmapMut::from_bytes(bytes, band.width, band.height) else {
        return;
    };
    // In Draw Mode the Overlay needs alpha 1 everywhere, otherwise the OS
    // sends clicks on fully transparent pixels to the apps underneath.
    region.fill(Color::from_rgba8(0, 0, 0, u8::from(draw_mode)));
    draw_elements(ink, laser, builder, &mut region, band, scale, format);
    let mut canvas = Canvas::new(&mut region, band.x, band.y);
    let this = Area::from_bounds(bounds_of(band));
    if selection
        .shown()
        .is_some_and(|area| overlaps(area, bounds_of(band)))
    {
        selection.paint(ink, &mut canvas, scale, format);
    }
    if chrome.region().is_some_and(|r| r.intersects(&this)) {
        chrome.paint(&mut canvas, format);
    }
    if chrome.panel_region().is_some_and(|r| r.intersects(&this)) {
        chrome.paint_panel(&mut canvas, format);
    }
    let stride = target.width() as usize * 4;
    let data = target.data_mut();
    let Some(rendered) = scratch.get(..len) else {
        return;
    };
    for (row, src) in rendered.chunks_exact(row_bytes).enumerate() {
        let start = (band.y as usize + row) * stride + band.x as usize * 4;
        if let Some(dst) = data.get_mut(start..start + row_bytes) {
            dst.copy_from_slice(src);
        }
    }
}

/// Fills every Element touching `view` (target pixels) into `pm`, whose
/// top-left pixel is `view`'s top-left, then the Laser on top of them.
fn draw_elements(
    ink: &Ink,
    laser: &Laser,
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
            fill_element(element, builder, pm, (scale, ox, oy), format);
        }
    }
    laser.fill(builder, pm, (scale, ox, oy), format == Format::Bgra);
}

/// Fills one Element. Vertices are mapped to absolute target pixels first and
/// the integer view origin is subtracted last: that subtraction is exact, so a
/// band redraw rasterizes bit-identically to a full redraw (a combined
/// transform would round differently and flip edge-coverage samples).
fn fill_element(
    element: &Element,
    builder: &mut Option<PathBuilder>,
    pm: &mut PixmapMut<'_>,
    (scale, ox, oy): (f32, f32, f32),
    format: Format,
) {
    let outline = element.outline();
    if outline.len() < 3 {
        return;
    }
    let (ex, ey) = (element.x(), element.y());
    let absolute = |v: [f32; 2]| ((v[0] + ex) * scale, (v[1] + ey) * scale);
    let mut path = builder.take().unwrap_or_default();
    // `outline.len() >= 3` was checked above, so both indexings are in range
    // and the modulo cannot divide by zero.
    let (x0, y0) = absolute(outline[0]);
    path.move_to(x0 - ox, y0 - oy);
    for (i, &v) in outline.iter().enumerate() {
        let (x, y) = absolute(v);
        let (nx, ny) = absolute(outline[(i + 1) % outline.len()]);
        path.quad_to(
            x - ox,
            y - oy,
            f32::midpoint(x, nx) - ox,
            f32::midpoint(y, ny) - oy,
        );
    }
    path.line_to(x0 - ox, y0 - oy);
    path.close();
    if let Some(path) = path.finish() {
        pm.fill_path(
            &path,
            &ink_paint(element, format),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
        *builder = Some(path.clear());
    }
}

fn ink_paint(element: &Element, format: Format) -> Paint<'static> {
    let [r, g, b] = element.stroke_color();
    let (r, b) = if format == Format::Bgra {
        (b, r)
    } else {
        (r, b)
    };
    // Excalidraw draws Elements pending erasure at a fifth of their opacity.
    let fifth = if element.is_erasing() { 5 } else { 1 };
    let alpha = u16::from(element.opacity().min(100)) * 255 / 100 / fifth;
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, u8::try_from(alpha).unwrap_or(255));
    paint.anti_alias = true;
    paint
}

/// A damage rectangle as physical `[l, t, r, b]`.
fn bounds_of(d: Damage) -> Rect {
    [
        to_f32(d.x),
        to_f32(d.y),
        to_f32(d.x + d.width),
        to_f32(d.y + d.height),
    ]
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
