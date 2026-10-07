//! Ink and its Elements.

use crate::freehand::Scratch;

/// A position, in pixels, origin top-left. Pointer events carry physical
/// pixels; an Element's points are logical (physical / scale factor), like
/// Excalidraw's scene coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Excalidraw's `LINE_CONFIRM_THRESHOLD`: a freedraw ending this close to its
/// start is closed into a loop on finalize (actionFinalize.tsx, `isPathALoop`).
const LOOP_THRESHOLD: f32 = 8.0;

/// perfect-freehand `size` = strokeWidth * 4.25 (renderElement.ts).
const SIZE_PER_WIDTH: f64 = 4.25;

/// Defaults: Excalidraw red, medium width (Excalidraw's "bold", 2), opaque.
pub(crate) const DEFAULT_COLOR: [u8; 3] = [0xe0, 0x31, 0x31];
pub(crate) const DEFAULT_WIDTH: f32 = 2.0;
pub(crate) const DEFAULT_OPACITY: u8 = 100;

/// Axis-aligned box `[left, top, right, bottom]`.
pub(crate) type Rect = [f32; 4];

/// One freehand stroke. Same shape as an Excalidraw freedraw element:
/// `points` and the cached outline are relative to (`x`, `y`).
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    id: u64,
    x: f32,
    y: f32,
    points: Vec<Point>,
    pressures: Vec<f32>,
    simulate_pressure: bool,
    stroke_color: [u8; 3],
    stroke_width: f32,
    opacity: u8,
    seed: u32,
    version: u32,
    /// Outline polygon, recomputed per pointer move while drawing and fixed
    /// (computed with `last`) once committed. Element-local, logical pixels.
    outline: Vec<[f32; 2]>,
    /// The previous outline, kept to find what a change touched.
    previous: Vec<[f32; 2]>,
    bounds: Option<Rect>,
    /// Box around the input points (not the outline), element-local. This is
    /// what Excalidraw's selection and its `width`/`height` are based on.
    extent: Rect,
}

impl Element {
    /// A new Element whose first point is `at` (logical). A missing pressure,
    /// or exactly 0.5, means simulated (Excalidraw: `event.pressure === 0.5`).
    pub(crate) fn start(id: u64, at: Point, pressure: Option<f32>) -> Self {
        let simulate_pressure = pressure.is_none_or(|p| (p - 0.5).abs() < f32::EPSILON);
        Self {
            id,
            x: at.x,
            y: at.y,
            points: {
                // Room for a long stroke without regrowing on the hot path.
                let mut points = Vec::with_capacity(256);
                points.push(Point { x: 0.0, y: 0.0 });
                points
            },
            pressures: if simulate_pressure {
                Vec::new()
            } else {
                Vec::from([pressure.unwrap_or(0.5)])
            },
            simulate_pressure,
            stroke_color: DEFAULT_COLOR,
            stroke_width: DEFAULT_WIDTH,
            opacity: DEFAULT_OPACITY,
            #[allow(
                clippy::cast_possible_truncation,
                reason = "a seed only needs to differ"
            )]
            seed: (id as u32).wrapping_mul(2_654_435_761),
            version: 1,
            outline: Vec::new(),
            previous: Vec::new(),
            bounds: None,
            extent: [0.0; 4],
        }
    }

    /// Computes the outline of the first point (a dot).
    pub(crate) fn preview(&mut self, scratch: &mut Scratch) -> Option<Rect> {
        self.refresh(scratch, false)
    }

    /// Appends a point (`at` is logical, absolute). Exact duplicates of the
    /// last point are dropped, like Excalidraw's pointermove.
    pub(crate) fn push(
        &mut self,
        at: Point,
        pressure: Option<f32>,
        scratch: &mut Scratch,
    ) -> Option<Rect> {
        let local = Point {
            x: at.x - self.x,
            y: at.y - self.y,
        };
        if self.points.last() == Some(&local) {
            return None;
        }
        self.append(local, pressure);
        self.refresh(scratch, false)
    }

    /// Ends the Stroke at `at`: the point is always kept (a dot gets
    /// Excalidraw's 0.0001 nudge, a near-closed path is closed) and the cached
    /// outline is recomputed with `last`.
    pub(crate) fn commit(
        &mut self,
        at: Point,
        pressure: Option<f32>,
        scratch: &mut Scratch,
    ) -> Option<Rect> {
        let mut local = Point {
            x: at.x - self.x,
            y: at.y - self.y,
        };
        if self.points.first() == Some(&local) {
            local.x += 0.0001;
            local.y += 0.0001;
        }
        self.append(local, pressure);
        self.close_loop();
        self.refresh(scratch, true)
    }

    /// Ends the Stroke without a final point (Draw Mode left mid-Stroke).
    pub(crate) fn finish(&mut self, scratch: &mut Scratch) -> Option<Rect> {
        self.close_loop();
        self.refresh(scratch, true)
    }

    fn append(&mut self, local: Point, pressure: Option<f32>) {
        self.points.push(local);
        self.extent = [
            self.extent[0].min(local.x),
            self.extent[1].min(local.y),
            self.extent[2].max(local.x),
            self.extent[3].max(local.y),
        ];
        if !self.simulate_pressure {
            self.pressures.push(pressure.unwrap_or(0.5));
        }
        self.version = self.version.wrapping_add(1);
    }

    fn close_loop(&mut self) {
        let (Some(&first), Some(&last)) = (self.points.first(), self.points.last()) else {
            return;
        };
        if self.points.len() >= 3 && (last.x - first.x).hypot(last.y - first.y) <= LOOP_THRESHOLD {
            if let Some(end) = self.points.last_mut() {
                *end = first;
            }
        }
    }

    /// Recomputes the outline; returns the element-local box that changed.
    /// Reuses both outline buffers: nothing is allocated once they are warm.
    fn refresh(&mut self, scratch: &mut Scratch, last: bool) -> Option<Rect> {
        std::mem::swap(&mut self.outline, &mut self.previous);
        scratch.load(
            self.points.iter().map(|p| [f64::from(p.x), f64::from(p.y)]),
            self.pressures.iter().map(|&p| f64::from(p)),
        );
        let size = f64::from(self.stroke_width) * SIZE_PER_WIDTH;
        self.outline.clear();
        #[allow(
            clippy::cast_possible_truncation,
            reason = "the cache is f32 by design"
        )]
        self.outline.extend(
            scratch
                .outline(size, last)
                .iter()
                .map(|v| [v[0] as f32, v[1] as f32]),
        );
        self.bounds = bounds_of(&self.outline);
        changed_box(&self.previous, &self.outline)
    }

    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    #[must_use]
    pub fn x(&self) -> f32 {
        self.x
    }

    #[must_use]
    pub fn y(&self) -> f32 {
        self.y
    }

    /// Points relative to (`x`, `y`); the first is always (0, 0).
    #[must_use]
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// One pressure per point, or empty when pressure is simulated.
    #[must_use]
    pub fn pressures(&self) -> &[f32] {
        &self.pressures
    }

    #[must_use]
    pub fn simulate_pressure(&self) -> bool {
        self.simulate_pressure
    }

    #[must_use]
    pub fn stroke_color(&self) -> [u8; 3] {
        self.stroke_color
    }

    #[must_use]
    pub fn stroke_width(&self) -> f32 {
        self.stroke_width
    }

    /// 0 to 100, like Excalidraw.
    #[must_use]
    pub fn opacity(&self) -> u8 {
        self.opacity
    }

    #[must_use]
    pub fn seed(&self) -> u32 {
        self.seed
    }

    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Box around the input points, element-local.
    pub(crate) fn extent(&self) -> Rect {
        self.extent
    }

    /// Box around the input points in Overlay (logical) coordinates.
    pub(crate) fn absolute_extent(&self) -> Rect {
        let [l, t, r, b] = self.extent;
        [l + self.x, t + self.y, r + self.x, b + self.y]
    }

    /// Moves the Element. Points and the cached outline are element-local,
    /// so nothing else changes.
    pub(crate) fn set_position(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
        self.version = self.version.wrapping_add(1);
    }

    /// Whether `at` (logical, absolute) is within `tolerance` of the input
    /// polyline: Excalidraw's freedraw hit test (centreline, not the outline).
    pub(crate) fn hit(&self, at: Point, tolerance: f32) -> bool {
        let [l, t, r, b] = self.absolute_extent();
        if at.x < l - tolerance
            || at.x > r + tolerance
            || at.y < t - tolerance
            || at.y > b + tolerance
        {
            return false;
        }
        let local = Point {
            x: at.x - self.x,
            y: at.y - self.y,
        };
        let limit = tolerance * tolerance;
        match self.points.as_slice() {
            [] => false,
            [only] => distance_squared(local, *only, *only) <= limit,
            points => points
                .windows(2)
                .any(|w| distance_squared(local, w[0], w[1]) <= limit),
        }
    }

    pub(crate) fn outline(&self) -> &[[f32; 2]] {
        &self.outline
    }

    /// Element-local box of the outline.
    pub(crate) fn bounds(&self) -> Option<Rect> {
        self.bounds
    }
}

/// Squared distance from `p` to the segment `a`-`b`.
fn distance_squared(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    let t = if length > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (ex, ey) = (p.x - (a.x + t * dx), p.y - (a.y + t * dy));
    ex * ex + ey * ey
}

fn bounds_of(vertices: &[[f32; 2]]) -> Option<Rect> {
    union_of(vertices.iter().copied())
}

fn union_of(vertices: impl Iterator<Item = [f32; 2]>) -> Option<Rect> {
    vertices.fold(None, |acc, [x, y]| {
        Some(match acc {
            None => [x, y, x, y],
            Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x), b.max(y)],
        })
    })
}

/// The vertices around a changed run, with two neighbours on each side.
fn span(v: &[[f32; 2]], head: usize, tail: usize) -> impl Iterator<Item = [f32; 2]> + '_ {
    let from = head.saturating_sub(2);
    let to = (v.len() - tail + 2).min(v.len());
    v.get(from..to).unwrap_or(&[]).iter().copied()
}

/// Box covering everything that differs between two outlines. Outlines are
/// deterministic, so the vertices only change in one run: the common head and
/// tail are skipped, and two neighbours are kept on each side because the
/// filled shape is quadratic curves between vertex midpoints.
#[allow(
    clippy::float_cmp,
    reason = "outlines are deterministic: identical input gives bit-identical vertices"
)]
fn changed_box(old: &[[f32; 2]], new: &[[f32; 2]]) -> Option<Rect> {
    let shortest = old.len().min(new.len());
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(shortest - head)
        .take_while(|(a, b)| a == b)
        .count();
    if head == old.len() && head == new.len() {
        return None;
    }
    union_of(span(old, head, tail).chain(span(new, head, tail)))
}

/// The set of Elements on the Overlay. Memory only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ink {
    elements: Vec<Element>,
}

impl Ink {
    #[must_use]
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    pub(crate) fn add(&mut self, element: Element) {
        self.elements.push(element);
    }

    pub(crate) fn pop(&mut self) -> Option<Element> {
        self.elements.pop()
    }

    /// Exchanges the whole Element list with `other` (Clear and its undo).
    pub(crate) fn swap_elements(&mut self, other: &mut Vec<Element>) {
        std::mem::swap(&mut self.elements, other);
    }

    /// Removes every Element and hands them back, for the operation log.
    pub(crate) fn take(&mut self) -> Vec<Element> {
        std::mem::take(&mut self.elements)
    }

    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut Element> {
        self.elements.get_mut(index)
    }

    pub(crate) fn remove(&mut self, index: usize) -> Option<Element> {
        (index < self.elements.len()).then(|| self.elements.remove(index))
    }

    /// Puts an Element back at `index` (clamped), keeping z-order.
    pub(crate) fn insert(&mut self, index: usize, element: Element) {
        self.elements
            .insert(index.min(self.elements.len()), element);
    }

    pub(crate) fn last_mut(&mut self) -> Option<&mut Element> {
        self.elements.last_mut()
    }
}
