//! The Selection: which Elements the Select Tool has chosen, and how it looks.
//!
//! Look (Excalidraw 0.18 `renderer/interactiveScene.ts`, MIT): every selected
//! Element gets a solid 1 px rectangle around the box of its input points,
//! 4 px outside it; two or more also get a dashed 1 px rectangle (2 on, 2 off)
//! around their common box. The drag box is filled `rgba(0, 0, 200, 0.04)` and
//! outlined in the selection colour. Deviations: no resize or rotation handles
//! (the spec excludes them), and in the dark theme the colour is the lightened
//! post-filter `#a8a5ff` because Markuli has no canvas colour filter.
//!
//! Painting is never incremental: [`Selection::flush`] marks the old and new
//! overlay areas as damage, and the renderer redraws that whole area (Ink,
//! then this, then the toolbar), so a repaint never stacks on itself.

use crate::ink::{Ink, Rect};
use crate::render::{Canvas, Format, Pending};
use crate::toolbar::Theme;
use tiny_skia::{Color, Paint, PathBuilder, Rect as SkiaRect, Stroke, StrokeDash, Transform};

/// Excalidraw's `DEFAULT_TRANSFORM_HANDLE_SPACING * 2`: the gap between an
/// Element and its selection rectangle, in logical pixels.
const PADDING: f32 = 4.0;

/// Anti-aliasing slack around painted geometry, in physical pixels.
const SLACK: f32 = 2.0;

const LIGHT: [u8; 3] = [0x69, 0x65, 0xdb];
const DARK: [u8; 3] = [0xa8, 0xa5, 0xff];

#[derive(Debug, Default)]
pub(crate) struct Selection {
    ids: Vec<u64>,
    /// The drag box in logical pixels, while one is being dragged.
    dragging: Option<Rect>,
    /// Physical area painted at the last flush: next flush's old damage.
    shown: Option<Rect>,
    /// Something visible changed since the last flush.
    stale: bool,
    theme: Theme,
    /// Reused by [`Selection::set_union`], so a box drag allocates nothing.
    spare: Vec<u64>,
    /// The dashed stroke of the common rectangle and the scale it was built
    /// for. `StrokeDash` owns its array, so it is built once per scale (in
    /// `flush`, which runs after every event) instead of once per frame.
    dashed: Option<(f32, Stroke)>,
}

impl Selection {
    pub fn ids(&self) -> &[u64] {
        &self.ids
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn contains(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }

    pub fn set(&mut self, ids: Vec<u64>) {
        if self.ids != ids {
            self.ids = ids;
            self.stale = true;
        }
    }

    /// Selects `base` followed by `more`, in place and without allocating once
    /// warm (the box drag calls this on every pointer move).
    pub fn set_union(&mut self, base: &[u64], more: impl Iterator<Item = u64>) {
        self.spare.clear();
        self.spare.extend_from_slice(base);
        self.spare.extend(more);
        if self.spare != self.ids {
            std::mem::swap(&mut self.ids, &mut self.spare);
            self.stale = true;
        }
    }

    /// Selects exactly `id`.
    pub fn set_one(&mut self, id: u64) {
        if self.ids != [id] {
            self.ids.clear();
            self.ids.push(id);
            self.stale = true;
        }
    }

    pub fn clear(&mut self) {
        self.set(Vec::new());
        self.set_box(None);
    }

    /// Adds `id`, or removes it when it is already selected.
    pub fn toggle(&mut self, id: u64) {
        match self.ids.iter().position(|&i| i == id) {
            Some(at) => {
                self.ids.remove(at);
            }
            None => self.ids.push(id),
        }
        self.stale = true;
    }

    pub fn select_all(&mut self, ink: &Ink) {
        self.set(ink.elements().iter().map(crate::Element::id).collect());
    }

    pub fn set_box(&mut self, rect: Option<Rect>) {
        if self.dragging != rect {
            self.dragging = rect;
            self.stale = true;
        }
    }

    /// Selected Elements moved or changed: repaint the overlay.
    pub fn touch(&mut self) {
        self.stale = true;
    }

    pub fn set_theme(&mut self, theme: Theme) {
        if self.theme != theme {
            self.theme = theme;
            self.stale = true;
        }
    }

    /// Drops ids that are no longer in the Ink (after undo, redo or Clear).
    pub fn retain_existing(&mut self, ink: &Ink) {
        let before = self.ids.len();
        self.ids
            .retain(|id| ink.elements().iter().any(|e| e.id() == *id));
        if self.ids.len() != before {
            self.stale = true;
        }
    }

    /// Marks the old and new overlay areas as damage, once per change.
    pub fn flush(&mut self, ink: &Ink, paint: &mut Pending, scale: f32) {
        if self.dashed.as_ref().map(|d| d.0) != Some(scale) {
            self.dashed = Some((scale, dashed_stroke(scale)));
        }
        if !self.stale {
            return;
        }
        self.stale = false;
        let now = self.area(ink, scale);
        for area in [self.shown, now].into_iter().flatten() {
            paint.add_physical(area);
        }
        self.shown = now;
    }

    /// The physical area the overlay covers (what to redraw before painting).
    pub fn shown(&self) -> Option<Rect> {
        self.shown
    }

    /// Selected Elements' padded rectangles, in logical pixels.
    fn rects<'a>(&'a self, ink: &'a Ink) -> impl Iterator<Item = Rect> + 'a {
        ink.elements()
            .iter()
            .filter(|e| self.contains(e.id()))
            .map(|e| grow(e.absolute_extent(), PADDING))
    }

    fn common(&self, ink: &Ink) -> Option<Rect> {
        self.rects(ink).reduce(union)
    }

    fn area(&self, ink: &Ink, scale: f32) -> Option<Rect> {
        let logical = [self.common(ink), self.dragging.map(|b| grow(b, 1.0))]
            .into_iter()
            .flatten()
            .reduce(union)?;
        let [l, t, r, b] = grow(logical, SLACK / scale);
        let area = [l * scale, t * scale, r * scale, b * scale];
        area.iter().all(|v| v.is_finite()).then_some(area)
    }

    /// Paints the overlay. The caller has already redrawn the whole of
    /// [`Selection::shown`] from the Ink.
    pub fn paint(&self, ink: &Ink, target: &mut Canvas<'_, '_>, scale: f32, format: Format) {
        let colour = match self.theme {
            Theme::Light => LIGHT,
            Theme::Dark => DARK,
        };
        let line = stroke_paint(colour, 1.0, format);
        let width = scale.round().max(1.0);
        let solid = Stroke {
            width,
            ..Stroke::default()
        };
        let count = self.ids.len();
        for rect in self.rects(ink) {
            outline(target, rect, scale, &line, &solid);
        }
        if count >= 2 {
            if let (Some(common), Some((_, dashed))) = (self.common(ink), &self.dashed) {
                outline(target, common, scale, &line, dashed);
            }
        }
        if let Some(rect) = self.dragging {
            let [l, t, r, b] = snap(rect, scale);
            if let Some(fill) = SkiaRect::from_ltrb(l, t, r, b) {
                let tint = stroke_paint([0, 0, 200], 0.04, format);
                target.fill_rect(fill, &tint);
            }
            outline(target, rect, scale, &line, &solid);
        }
    }
}

/// 1 px lines, 2 on and 2 off (logical), for the common rectangle.
fn dashed_stroke(scale: f32) -> Stroke {
    Stroke {
        width: scale.round().max(1.0),
        dash: StrokeDash::new(vec![2.0 * scale, 2.0 * scale], 0.0),
        ..Stroke::default()
    }
}

/// Strokes `rect` (logical) with a line that sits exactly on pixel centres.
fn outline(
    target: &mut Canvas<'_, '_>,
    rect: Rect,
    scale: f32,
    paint: &Paint<'_>,
    stroke: &Stroke,
) {
    let [l, t, r, b] = snap(rect, scale);
    let half = stroke.width / 2.0;
    if let Some(path) =
        SkiaRect::from_ltrb(l + half, t + half, r + half, b + half).map(PathBuilder::from_rect)
    {
        target.stroke_path(&path, paint, stroke, Transform::identity());
    }
}

/// Physical, rounded to whole pixels so 1 px lines are not blurred.
fn snap([l, t, r, b]: Rect, scale: f32) -> Rect {
    [
        (l * scale).round(),
        (t * scale).round(),
        (r * scale).round(),
        (b * scale).round(),
    ]
}

fn stroke_paint(rgb: [u8; 3], alpha: f32, format: Format) -> Paint<'static> {
    let [r, g, b] = rgb;
    let (r, b) = if format == Format::Bgra {
        (b, r)
    } else {
        (r, b)
    };
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(r, g, b, alpha_byte(alpha)));
    paint.anti_alias = true;
    paint
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255"
)]
fn alpha_byte(alpha: f32) -> u8 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn grow([l, t, r, b]: Rect, by: f32) -> Rect {
    [l - by, t - by, r + by, b + by]
}

fn union(a: Rect, b: Rect) -> Rect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// The box between two drag corners, whichever way it was dragged.
pub(crate) fn normalized(a: (f32, f32), b: (f32, f32)) -> Rect {
    [a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1)]
}

/// Whether `inner` lies fully inside `outer`.
pub(crate) fn encloses(outer: Rect, inner: Rect) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}
