//! Freehand: a port of perfect-freehand's `getStroke`, called with the options
//! Excalidraw passes for freedraw elements.
//!
//! Sources (MIT, (c) 2021 Stephen Ruiz Ltd.): perfect-freehand 1.2.0,
//! `src/{getStroke,getStrokePoints,getStrokeOutlinePoints,getStrokeRadius,vec}.ts`.
//! Excalidraw call site: `packages/excalidraw/renderer/renderElement.ts`
//! (freedraw): thinning 0.6, smoothing 0.5, streamline 0.5, easeOutSine.
//!
//! Deviations from the library, all behaviour-neutral for Excalidraw's call:
//! - Excalidraw never passes `start`/`end` options, so tapers are always
//!   undefined (falsy) and both caps are round; that code is folded in.
//! - Math runs in `f64` (like JS) because the cap loops accumulate a float
//!   step and iterate 14/13/29 times only under IEEE double arithmetic.
//!   Callers cache the outline as `f32`.
//! - Working buffers live in [`Scratch`] and are reused, so recomputing an
//!   outline per pointer move allocates nothing once warm.

type V = [f64; 2];

const THINNING: f64 = 0.6;
const SMOOTHING: f64 = 0.5;
const STREAMLINE: f64 = 0.5;
const RATE_OF_PRESSURE_CHANGE: f64 = 0.275;
/// Browser strokes seem to be off if PI is regular (perfect-freehand comment).
const FIXED_PI: f64 = std::f64::consts::PI + 0.0001;

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: V, n: f64) -> V {
    [a[0] * n, a[1] * n]
}
fn per(a: V) -> V {
    [a[1], -a[0]]
}
fn dpr(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn dist2(a: V, b: V) -> f64 {
    let d = sub(a, b);
    d[0] * d[0] + d[1] * d[1]
}
fn uni(a: V) -> V {
    let len = a[0].hypot(a[1]);
    [a[0] / len, a[1] / len]
}
fn lrp(a: V, b: V, t: f64) -> V {
    add(a, mul(sub(b, a), t))
}
fn rot_around(a: V, c: V, r: f64) -> V {
    let (s, co) = r.sin_cos();
    let (px, py) = (a[0] - c[0], a[1] - c[1]);
    [px * co - py * s + c[0], px * s + py * co + c[1]]
}
/// Excalidraw's easing: easeOutSine.
fn easing(t: f64) -> f64 {
    (t * std::f64::consts::PI / 2.0).sin()
}
fn stroke_radius(size: f64, pressure: f64) -> f64 {
    size * easing(0.5 - THINNING * (0.5 - pressure))
}

#[derive(Clone, Copy, Debug)]
struct StrokePoint {
    point: V,
    pressure: f64,
    vector: V,
    distance: f64,
    running_length: f64,
}

/// Reusable working memory for [`Scratch::outline`].
#[derive(Debug, Default)]
pub(crate) struct Scratch {
    /// Input as `[x, y, pressure]`; NaN pressure stands for JS `undefined`.
    input: Vec<[f64; 3]>,
    pts: Vec<[f64; 3]>,
    stroke_points: Vec<StrokePoint>,
    left: Vec<V>,
    right: Vec<V>,
    start_cap: Vec<V>,
    end_cap: Vec<V>,
    out: Vec<V>,
}

impl Scratch {
    /// Loads input points. An empty `pressures` means simulated pressure,
    /// which Excalidraw feeds to the library as plain `[x, y]` pairs.
    pub(crate) fn load(
        &mut self,
        points: impl Iterator<Item = V>,
        mut pressures: impl Iterator<Item = f64>,
    ) {
        self.input.clear();
        for p in points {
            self.input
                .push([p[0], p[1], pressures.next().unwrap_or(f64::NAN)]);
        }
    }

    /// The outline polygon of the loaded input. `last` marks a committed Stroke.
    pub(crate) fn outline(&mut self, size: f64, last: bool) -> &[V] {
        self.out.clear();
        self.compute_stroke_points(size, last);
        self.compute_outline(size);
        &self.out
    }

    /// perfect-freehand `getStrokePoints`.
    fn compute_stroke_points(&mut self, size: f64, is_complete: bool) {
        // Mirrors `getStrokePoints` step by step (rule 15), for parity diffs.
        self.stroke_points.clear();
        if self.input.is_empty() {
            return;
        }
        let t = 0.15 + (1.0 - STREAMLINE) * 0.85;
        self.pts.clear();
        self.pts.extend_from_slice(&self.input);
        if self.pts.len() == 2 {
            // Extra points avoid "dash" lines. Interpolated points carry no
            // pressure: the library's `lrp` returns 2-vectors.
            // Indexing is safe: the length is exactly 2 here.
            let (first, end) = (self.pts[0], self.pts[1]);
            self.pts.truncate(1);
            for i in 1..5_u8 {
                let p = lrp([first[0], first[1]], [end[0], end[1]], f64::from(i) / 4.0);
                self.pts.push([p[0], p[1], f64::NAN]);
            }
        }
        if self.pts.len() == 1 {
            // Indexing is safe: the length is exactly 1 here.
            let p = self.pts[0];
            self.pts.push([p[0] + 1.0, p[1] + 1.0, p[2]]);
        }
        let pts = &self.pts;
        // `input` is non-empty (checked above), so `pts` has at least one point.
        let Some(&first) = pts.first() else {
            return;
        };
        let mut prev = StrokePoint {
            point: [first[0], first[1]],
            pressure: if first[2] >= 0.0 { first[2] } else { 0.25 },
            vector: [1.0, 1.0],
            distance: 0.0,
            running_length: 0.0,
        };
        self.stroke_points.push(prev);
        let mut reached_minimum = false;
        let mut running_length = 0.0;
        // `pts` is non-empty, so this cannot underflow.
        let max = pts.len().saturating_sub(1);
        for (i, p) in pts.iter().enumerate().skip(1) {
            let target = [p[0], p[1]];
            let point = if is_complete && i == max {
                target
            } else {
                lrp(prev.point, target, t)
            };
            #[allow(clippy::float_cmp, reason = "exact equality is the library's rule")]
            if prev.point == point {
                continue;
            }
            let distance = (point[1] - prev.point[1]).hypot(point[0] - prev.point[0]);
            running_length += distance;
            // Wait for a minimum length at the start of the line to avoid noise.
            if i < max && !reached_minimum {
                if running_length < size {
                    continue;
                }
                reached_minimum = true;
            }
            prev = StrokePoint {
                point,
                pressure: if p[2] >= 0.0 { p[2] } else { 0.5 },
                vector: uni(sub(prev.point, point)),
                distance,
                running_length,
            };
            self.stroke_points.push(prev);
        }
        let second_vector = self.stroke_points.get(1).map_or([0.0, 0.0], |s| s.vector);
        if let Some(head) = self.stroke_points.first_mut() {
            head.vector = second_vector;
        }
    }

    /// perfect-freehand `getStrokeOutlinePoints` (no tapers, round caps).
    #[allow(
        clippy::too_many_lines,
        reason = "1:1 port, kept linear to diff against the source"
    )]
    fn compute_outline(&mut self, size: f64) {
        // Long on purpose (rule 15): this mirrors `getStrokeOutlinePoints` line
        // for line, so a change upstream can be diffed and re-ported. Splitting
        // it would hide parity drift.
        let simulate = !self.input.iter().any(|p| p[2] >= 0.0);
        let points = &self.stroke_points;
        let (Some(&first_sp), Some(&last_sp)) = (points.first(), points.last()) else {
            return;
        };
        if size <= 0.0 {
            return;
        }
        let total_length = last_sp.running_length;
        let min_distance = (size * SMOOTHING).powi(2);
        self.left.clear();
        self.right.clear();
        self.start_cap.clear();
        self.end_cap.clear();
        let (left, right) = (&mut self.left, &mut self.right);

        let mut prev_pressure = points.iter().take(10).fold(first_sp.pressure, |acc, curr| {
            let mut pressure = curr.pressure;
            if simulate {
                let sp = (curr.distance / size).min(1.0);
                let rp = (1.0 - sp).min(1.0);
                pressure = (acc + (rp - acc) * (sp * RATE_OF_PRESSURE_CHANGE)).min(1.0);
            }
            f64::midpoint(acc, pressure)
        });
        let mut radius = stroke_radius(size, last_sp.pressure);
        let mut first_radius: Option<f64> = None;
        let mut prev_vector = first_sp.vector;
        let mut pl = first_sp.point;
        let mut pr = pl;
        let (mut tl, mut tr);
        let mut is_prev_point_sharp_corner = false;

        for (i, &sp) in points.iter().enumerate() {
            let StrokePoint {
                point,
                vector,
                distance,
                running_length,
                mut pressure,
            } = sp;
            // Removes noise from the end of the line.
            if i + 1 < points.len() && total_length - running_length < 3.0 {
                continue;
            }
            // THINNING is non-zero for Excalidraw.
            if simulate {
                let sp = (distance / size).min(1.0);
                let rp = (1.0 - sp).min(1.0);
                pressure = (prev_pressure + (rp - prev_pressure) * (sp * RATE_OF_PRESSURE_CHANGE))
                    .min(1.0);
            }
            radius = stroke_radius(size, pressure);
            if first_radius.is_none() {
                first_radius = Some(radius);
            }
            radius = radius.max(0.01);

            let next = points.get(i + 1);
            let next_vector = next.map_or(vector, |n| n.vector);
            let next_dpr = if next.is_some() {
                dpr(vector, next_vector)
            } else {
                1.0
            };
            let prev_dpr = dpr(vector, prev_vector);
            let is_point_sharp_corner = prev_dpr < 0.0 && !is_prev_point_sharp_corner;
            let is_next_point_sharp_corner = next_dpr < 0.0;

            if is_point_sharp_corner || is_next_point_sharp_corner {
                // A rounded cap at the corner. 14 iterations: f64 accumulation.
                let offset = mul(per(prev_vector), radius);
                let step = 1.0 / 13.0;
                let mut t = 0.0_f64;
                tl = pl;
                tr = pr;
                while t <= 1.0 {
                    tl = rot_around(sub(point, offset), point, FIXED_PI * t);
                    left.push(tl);
                    tr = rot_around(add(point, offset), point, FIXED_PI * -t);
                    right.push(tr);
                    t += step;
                }
                pl = tl;
                pr = tr;
                if is_next_point_sharp_corner {
                    is_prev_point_sharp_corner = true;
                }
                continue;
            }
            is_prev_point_sharp_corner = false;

            if i + 1 == points.len() {
                let offset = mul(per(vector), radius);
                left.push(sub(point, offset));
                right.push(add(point, offset));
                continue;
            }
            let offset = mul(per(lrp(next_vector, vector, next_dpr)), radius);
            tl = sub(point, offset);
            if i <= 1 || dist2(pl, tl) > min_distance {
                left.push(tl);
                pl = tl;
            }
            tr = add(point, offset);
            if i <= 1 || dist2(pr, tr) > min_distance {
                right.push(tr);
                pr = tr;
            }
            prev_pressure = pressure;
            prev_vector = vector;
        }

        let first_point = first_sp.point;
        let last_point = if points.len() > 1 {
            last_sp.point
        } else {
            add(first_sp.point, [1.0, 1.0])
        };

        if points.len() == 1 {
            // A dot: 13 points around the first point.
            let start = add(
                first_point,
                mul(
                    uni(per(sub(first_point, last_point))),
                    -first_radius.unwrap_or(radius),
                ),
            );
            let step = 1.0 / 13.0;
            let mut t = step;
            while t <= 1.0 {
                self.out
                    .push(rot_around(start, first_point, FIXED_PI * 2.0 * t));
                t += step;
            }
            return;
        }
        if let Some(&r0) = right.first() {
            let step = 1.0 / 13.0;
            let mut t = step;
            while t <= 1.0 {
                self.start_cap
                    .push(rot_around(r0, first_point, FIXED_PI * t));
                t += step;
            }
        }
        // A full turn and a half: prevents incorrect caps on sharp end turns.
        let direction = per(mul(last_sp.vector, -1.0));
        let start = add(last_point, mul(direction, radius));
        let step = 1.0 / 29.0;
        let mut t = step;
        while t < 1.0 {
            self.end_cap
                .push(rot_around(start, last_point, FIXED_PI * 3.0 * t));
            t += step;
        }
        self.out.extend_from_slice(left);
        self.out.extend_from_slice(&self.end_cap);
        self.out.extend(right.iter().rev());
        self.out.extend_from_slice(&self.start_cap);
    }
}

/// The outline polygon of a Stroke, as Excalidraw computes it: perfect-freehand
/// `getStroke` with `size = strokeWidth * 4.25` and Excalidraw's other options.
///
/// `points` are relative to the Element origin. An empty `pressures` means
/// simulated pressure; otherwise there is one pressure per point. `last` is
/// true once the Stroke is committed.
///
/// This exists for the parity tests against the JS library; the core itself
/// uses the allocation-free [`Scratch`] directly.
#[must_use]
pub fn get_stroke(points: &[[f64; 2]], pressures: &[f64], size: f64, last: bool) -> Vec<[f64; 2]> {
    let mut scratch = Scratch::default();
    scratch.load(points.iter().copied(), pressures.iter().copied());
    scratch.outline(size, last).to_vec()
}
