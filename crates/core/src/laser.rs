//! Laser: a fading pointer trail, ported from `@excalidraw/laser-pointer`.
//!
//! Sources (MIT, (c) 2023 Excalidraw): `@excalidraw/laser-pointer` 1.3.1,
//! `src/{state,math}.ts`; Excalidraw's options and decay are
//! `packages/excalidraw/laser-trails.ts` (`getTrailOptions`): size 2 (a
//! radius), streamline 0.4, simplify 0, decay 1000 ms, fade over the last 50
//! points, both eased with `easeOut(k) = 1 - (1 - k)^4`. The path hand-off is
//! `getSvgPathFromStroke` (`Q`, then smooth `T` quadratics), see [`Laser::fill`].
//!
//! Deviations, all behaviour-neutral for Excalidraw's call:
//! - Excalidraw passes `simplify: 0` and `keepHead: false`, so Douglas-Peucker
//!   and the head circle are not ported.
//! - The geometry is 2-D: the third coordinate (the timestamp) is only ever
//!   read back from the stored points, never from derived ones.
//! - The core has no clock: time is [`Event::Clock`](crate::Event::Clock),
//!   in whole milliseconds. Excalidraw uses fractional `performance.now()`.
//! - Where JS would leak a NaN into the path (a zero-length direction), the
//!   outline is dropped instead: the core is total.
//! - Math runs in `f64` like JS, and sin, cos, atan2 and sqrt come from the
//!   `libm` crate (fdlibm, like V8): a corner arc's vertex count depends on
//!   float accumulation of its turn angle, which one ulp of a different libm
//!   can change.
//!
//! The Laser is never part of Ink: it has its own buffers, its own damage and
//! its own fill, and ends by itself.

use crate::ink::Rect;
use crate::render::Pending;
use std::f64::consts::PI;
use tiny_skia::{FillRule, Paint, PathBuilder, PixmapMut, Transform};

type V = [f64; 2];
/// `[x, y, timestamp in ms]`, like laser-pointer's `[x, y, r]`.
type P = [f64; 3];

const SIZE: f64 = 2.0;
const STREAMLINE: f64 = 0.4;
const MAX_TAIL_LENGTH: f64 = 50.0;
const CORNER_MAX_ANGLE: f64 = 75.0;
const DECAY_TIME: f64 = 1000.0;
const DECAY_LENGTH: f64 = 50.0;
/// The next frame is due this long after the clock while a trail is alive.
const FRAME_MS: u64 = 16;
/// Anti-aliasing and curve slack around a trail, in physical pixels.
const DAMAGE_PAD: f32 = 3.0;

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1]]
}
fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1]]
}
fn smul(a: V, s: f64) -> V {
    [a[0] * s, a[1] * s]
}
fn rot(a: V, rad: f64) -> V {
    let (s, c) = (libm::sin(rad), libm::cos(rad));
    [c * a[0] - s * a[1], s * a[0] + c * a[1]]
}
fn mag(a: V) -> f64 {
    libm::sqrt(a[0] * a[0] + a[1] * a[1])
}
fn norm(a: V) -> V {
    let m = mag(a);
    [a[0] / m, a[1] / m]
}
fn dist(a: P, b: P) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    libm::sqrt(dx * dx + dy * dy)
}
fn angle(p: V, p1: V, p2: V) -> f64 {
    libm::atan2(p2[1] - p[1], p2[0] - p[0]) - libm::atan2(p1[1] - p[1], p1[0] - p[0])
}
fn norm_angle(a: f64) -> f64 {
    libm::atan2(libm::sin(a), libm::cos(a))
}
fn xy(p: P) -> V {
    [p[0], p[1]]
}
fn ease_out(k: f64) -> f64 {
    1.0 - (1.0 - k).powi(4)
}

/// Excalidraw's `sizeMapping`: shrinks with the age of the point and with its
/// distance, in points, from the head.
fn size_mapping(now: f64, stamp: f64, index: usize, total: usize) -> f64 {
    let t = (1.0 - (now - stamp) / DECAY_TIME).max(0.0);
    #[allow(clippy::cast_precision_loss, reason = "point counts are tiny")]
    let behind = (total - index) as f64;
    let l = (DECAY_LENGTH - behind.min(DECAY_LENGTH)) / DECAY_LENGTH;
    ease_out(l).min(ease_out(t))
}

/// `LaserPointer`'s point store. Points are streamlined as they arrive.
#[derive(Debug, Default)]
struct Pointer {
    last_input: Option<V>,
    stable: Vec<P>,
    tail: Vec<P>,
}

impl Pointer {
    fn add_point(&mut self, point: P) {
        if self.last_input == Some(xy(point)) {
            return;
        }
        self.last_input = Some(xy(point));
        let Some(&last) = self.tail.last().or(self.stable.last()) else {
            self.stable.push(point);
            return;
        };
        let t = 1.0 - STREAMLINE;
        self.tail.push([
            last[0] + (point[0] - last[0]) * t,
            last[1] + (point[1] - last[1]) * t,
            last[2] + (point[2] - last[2]) * t,
        ]);
        if run_length(&self.tail) > MAX_TAIL_LENGTH {
            self.stabilize_tail();
        }
    }

    fn stabilize_tail(&mut self) {
        self.stable.append(&mut self.tail);
    }

    fn len(&self) -> usize {
        self.stable.len() + self.tail.len()
    }

    fn at(&self, i: usize) -> P {
        self.stable
            .get(i)
            .or_else(|| self.tail.get(i - self.stable.len()))
            .copied()
            .unwrap_or([0.0; 3])
    }
}

/// `runLength`, including its habit of counting the last segment twice.
fn run_length(ps: &[P]) -> f64 {
    let [.., a, b] = ps else { return 0.0 };
    ps.windows(2).map(|w| dist(w[0], w[1])).sum::<f64>() + dist(*a, *b)
}

/// Reusable working memory for outlines.
#[derive(Debug, Default)]
struct Scratch {
    forward: Vec<V>,
    backward: Vec<V>,
    start_cap: Vec<V>,
    end_cap: Vec<V>,
}

/// A circle point `c + rot([1, 0], theta) * size`.
fn on_circle(c: V, theta: f64, size: f64) -> V {
    add(c, smul(rot([1.0, 0.0], theta), size))
}

/// Pushes `f(theta)` for `theta` from `from` up to and including `to`,
/// accumulating `step` like the JS `for` loops (the vertex count depends on it).
fn sweep(from: f64, to: f64, step: f64, mut f: impl FnMut(f64)) {
    let mut theta = from;
    while theta <= to {
        f(theta);
        theta += step;
    }
}

impl Pointer {
    /// `getStrokeOutline(SIZE)` at time `now`; empty once everything faded.
    fn outline(&self, now: f64, s: &mut Scratch, out: &mut Vec<V>) {
        out.clear();
        let len = self.len();
        let size_at = |p: P, index: usize| SIZE * size_mapping(now, p[2], index, len);
        match len {
            0 => {}
            1 => {
                let c = self.at(0);
                let size = size_at(c, 0);
                if size >= 0.5 {
                    sweep(0.0, 2.0 * PI, PI / 16.0, |t| {
                        out.push(on_circle(xy(c), t, size));
                    });
                    out.push(on_circle(xy(c), 0.0, size));
                }
            }
            2 => {
                let (c, n) = (self.at(0), self.at(1));
                let (c_size, n_size) = (size_at(c, 0), size_at(n, 0));
                if c_size >= 0.5 && n_size >= 0.5 {
                    let a = angle(xy(c), [c[0], c[1] - 100.0], xy(n));
                    sweep(a, PI + a, PI / 16.0, |t| {
                        out.push(on_circle(xy(c), t, c_size));
                    });
                    sweep(PI + a, 2.0 * PI + a, PI / 16.0, |t| {
                        out.push(on_circle(xy(n), t, n_size));
                    });
                    out.push(out[0]);
                }
            }
            _ => self.outline_long(now, s, out),
        }
        if !out.iter().all(|v| v[0].is_finite() && v[1].is_finite()) {
            out.clear();
        }
    }

    /// The `len >= 3` branch: a corner-aware ribbon with round caps.
    #[allow(
        clippy::too_many_lines,
        clippy::similar_names,
        clippy::many_single_char_names,
        reason = "1:1 port, kept linear and with the library's names to diff against the source"
    )]
    fn outline_long(&self, now: f64, s: &mut Scratch, out: &mut Vec<V>) {
        let len = self.len();
        let size_at = |p: P, index: usize| SIZE * size_mapping(now, p[2], index, len);
        s.forward.clear();
        s.backward.clear();
        let mut prev_speed = 0.0;
        let mut visible_start = 0;
        for i in 1..len - 1 {
            let (p, c, n) = (self.at(i - 1), self.at(i), self.at(i + 1));
            let d = dist(p, c);
            let speed = prev_speed + (d - prev_speed) * 0.2;
            let c_size = size_at(c, i);
            if c_size == 0.0 {
                visible_start = i + 1;
                continue;
            }
            let (cv, pv, nv) = (xy(c), xy(p), xy(n));
            let dir_pc = norm(sub(pv, cv));
            let dir_nc = norm(sub(nv, cv));
            let p1_dir_pc = rot(dir_pc, PI / 2.0);
            let p2_dir_pc = rot(dir_pc, -PI / 2.0);
            let p1_dir_nc = rot(dir_nc, PI / 2.0);
            let p2_dir_nc = rot(dir_nc, -PI / 2.0);
            let p1_nc = add(cv, smul(p1_dir_nc, c_size));
            let p2_pc = add(cv, smul(p2_dir_pc, c_size));
            let p1_pc = add(cv, smul(p1_dir_pc, c_size));
            let p2_nc = add(cv, smul(p2_dir_nc, c_size));
            let ftdir = add(p1_dir_pc, p2_dir_nc);
            let btdir = add(p2_dir_pc, p1_dir_nc);
            let pa_pc = add(
                cv,
                smul(
                    if mag(ftdir) == 0.0 {
                        dir_pc
                    } else {
                        norm(ftdir)
                    },
                    c_size,
                ),
            );
            let pa_nc = add(
                cv,
                smul(
                    if mag(btdir) == 0.0 {
                        dir_nc
                    } else {
                        norm(btdir)
                    },
                    c_size,
                ),
            );
            let c_angle = norm_angle(angle(cv, pv, nv));
            let variance = if speed > 35.0 { 0.5 } else { 1.0 };
            let d_angle = CORNER_MAX_ANGLE / 180.0 * PI * variance;
            if c_angle.abs() < d_angle {
                let t_angle = norm_angle(PI - c_angle).abs();
                #[allow(clippy::float_cmp, reason = "the library's own exact test")]
                if t_angle == 0.0 {
                    continue;
                }
                let step = t_angle / 4.0;
                if c_angle < 0.0 {
                    s.backward.extend([p2_pc, pa_nc]);
                    let arm = smul(p1_dir_pc, c_size);
                    sweep(0.0, t_angle, step, |t| s.forward.push(add(cv, rot(arm, t))));
                    descend(t_angle, step, |t| s.backward.push(add(cv, rot(arm, t))));
                    s.backward.extend([pa_nc, p1_nc]);
                } else {
                    s.forward.extend([p1_pc, pa_pc]);
                    let arm = smul(p1_dir_pc, -c_size);
                    sweep(0.0, t_angle, step, |t| {
                        s.backward.push(add(cv, rot(arm, -t)));
                    });
                    descend(t_angle, step, |t| s.forward.push(add(cv, rot(arm, -t))));
                    s.forward.extend([pa_pc, p2_nc]);
                }
            } else {
                s.forward.push(pa_pc);
                s.backward.push(pa_nc);
            }
            prev_speed = speed;
        }
        if visible_start >= len - 2 {
            return; // keepHead is false: a fully faded trail has no outline
        }
        let (first, second) = (self.at(visible_start), self.at(visible_start + 1));
        let (penultimate, ultimate) = (self.at(len - 2), self.at(len - 1));
        let dir_fs = norm(sub(xy(second), xy(first)));
        let dir_pu = norm(sub(xy(penultimate), xy(ultimate)));
        let pp_fs = rot(dir_fs, -PI / 2.0);
        let pp_pu = rot(dir_pu, PI / 2.0);
        let start_size = size_at(first, 0);
        let end_size = size_at(penultimate, len - 2);
        s.start_cap.clear();
        s.end_cap.clear();
        if start_size > 1.0 {
            // JS unshifts each point: the cap comes out reversed.
            sweep(0.0, PI, PI / 16.0, |t| {
                s.start_cap
                    .push(add(xy(first), rot(smul(pp_fs, start_size), -t)));
            });
            s.start_cap.push(add(xy(first), smul(pp_fs, -start_size)));
            s.start_cap.reverse();
        } else {
            s.start_cap.push(xy(first));
        }
        sweep(0.0, 3.0 * PI, PI / 16.0, |t| {
            s.end_cap
                .push(add(xy(ultimate), rot(smul(pp_pu, -end_size), -t)));
        });
        out.extend_from_slice(&s.start_cap);
        out.extend_from_slice(&s.forward);
        out.extend(s.end_cap.iter().rev());
        out.extend(s.backward.iter().rev());
        out.push(s.start_cap[0]);
    }
}

/// `for (theta = from; theta >= 0; theta -= step)`.
fn descend(from: f64, step: f64, mut f: impl FnMut(f64)) {
    let mut theta = from;
    while theta >= 0.0 {
        f(theta);
        theta -= step;
    }
}

/// The outline of a trail of `[x, y, ms]` points at time `now`, `closed` once
/// the pointer went up. This exists for the parity test against the JS
/// library (standards rule 17); the app goes through [`Event`](crate::Event).
#[must_use]
pub fn get_trail(points: &[[f64; 3]], now: f64, closed: bool) -> Vec<[f64; 2]> {
    let mut pointer = Pointer::default();
    for &p in points {
        pointer.add_point(p);
    }
    if closed {
        pointer.stabilize_tail();
    }
    let mut out = Vec::new();
    pointer.outline(now, &mut Scratch::default(), &mut out);
    out
}

/// One trail: the pointer's points and the outline cached for drawing.
#[derive(Debug, Default)]
struct Trail {
    pointer: Pointer,
    /// The pointer is still down.
    live: bool,
    /// Outline in logical pixels, and its box.
    outline: Vec<[f32; 2]>,
    bounds: Option<Rect>,
}

/// All trails on the Overlay: the one being drawn and those still fading.
#[derive(Debug, Default)]
pub(crate) struct Laser {
    trails: Vec<Trail>,
    now: f64,
    scratch: Scratch,
    work: Vec<V>,
}

impl Laser {
    /// The pointer went down at `at` (logical).
    pub fn start(&mut self, at: crate::Point, paint: &mut Pending, scale: f32) {
        self.end(paint, scale);
        let mut trail = Trail {
            live: true,
            ..Trail::default()
        };
        trail
            .pointer
            .add_point([f64::from(at.x), f64::from(at.y), self.now]);
        self.trails.push(trail);
        self.refresh(paint, scale);
    }

    pub fn extend(&mut self, at: crate::Point, paint: &mut Pending, scale: f32) {
        let now = self.now;
        if let Some(trail) = self.trails.iter_mut().find(|t| t.live) {
            trail
                .pointer
                .add_point([f64::from(at.x), f64::from(at.y), now]);
            self.refresh(paint, scale);
        }
    }

    /// The pointer went up: the trail stops growing and fades out.
    pub fn end(&mut self, paint: &mut Pending, scale: f32) {
        if let Some(trail) = self.trails.iter_mut().find(|t| t.live) {
            trail.live = false;
            trail.pointer.stabilize_tail();
            self.refresh(paint, scale);
        }
    }

    /// Time moved on (milliseconds, any origin, never backwards).
    pub fn set_now(&mut self, now: u64, paint: &mut Pending, scale: f32) {
        #[allow(clippy::cast_precision_loss, reason = "milliseconds fit f64 exactly")]
        let now = now as f64;
        if now > self.now {
            self.now = now;
            if !self.trails.is_empty() {
                self.refresh(paint, scale);
            }
        }
    }

    /// Drops every trail (Draw Mode ended); the caller repaints in full.
    pub fn clear(&mut self) {
        self.trails.clear();
    }

    /// When the next frame is due: only while a trail is still visible, so the
    /// loop sleeps once the last one faded.
    pub fn next_frame(&self) -> Option<u64> {
        let alive = self.trails.iter().any(|t| !t.outline.is_empty());
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "now >= 0"
        )]
        alive.then(|| self.now as u64 + FRAME_MS)
    }

    /// Recomputes every outline, marks what changed and forgets the trails
    /// that are over (a trail whose pointer is still down is kept).
    fn refresh(&mut self, paint: &mut Pending, scale: f32) {
        let Self {
            trails,
            now,
            scratch,
            work,
        } = self;
        for trail in trails.iter_mut() {
            trail.pointer.outline(*now, scratch, work);
            let old = trail.bounds;
            trail.outline.clear();
            #[allow(
                clippy::cast_possible_truncation,
                reason = "the cache is f32 by design"
            )]
            trail
                .outline
                .extend(work.iter().map(|v| [v[0] as f32, v[1] as f32]));
            trail.bounds = trail.outline.iter().fold(None, |acc, &[x, y]| {
                Some(match acc {
                    None => [x, y, x, y],
                    Some([l, t, r, b]) => [l.min(x), t.min(y), r.max(x), b.max(y)],
                })
            });
            for [l, t, r, b] in old.into_iter().chain(trail.bounds) {
                let rect = [
                    l * scale - DAMAGE_PAD,
                    t * scale - DAMAGE_PAD,
                    r * scale + DAMAGE_PAD,
                    b * scale + DAMAGE_PAD,
                ];
                if rect.iter().all(|v| v.is_finite()) {
                    paint.add_physical(rect);
                }
            }
        }
        trails.retain(|t| t.live || !t.outline.is_empty());
    }

    /// Fills the trails into `pm`, whose top-left pixel is `(ox, oy)` of the
    /// target. All trails go into one path and one fill, like Excalidraw, so
    /// overlaps do not darken.
    ///
    /// The path is `getSvgPathFromStroke`: `M p0 Q p1 mid(p1, p2)` then `T`
    /// to the midpoint of each following pair, closed. `T` reflects the last
    /// control point about the current point.
    pub fn fill(
        &self,
        builder: &mut Option<PathBuilder>,
        pm: &mut PixmapMut<'_>,
        (scale, ox, oy): (f32, f32, f32),
        bgra: bool,
    ) {
        let (vr, vb) = (ox + pixels(pm.width()), oy + pixels(pm.height()));
        let mut path = builder.take().unwrap_or_default();
        let mut any = false;
        for trail in &self.trails {
            let Some([l, t, r, b]) = trail.bounds else {
                continue;
            };
            let inside = l * scale - DAMAGE_PAD < vr
                && r * scale + DAMAGE_PAD > ox
                && t * scale - DAMAGE_PAD < vb
                && b * scale + DAMAGE_PAD > oy;
            if inside && trail.outline.len() >= 4 {
                any |= trail_path(&mut path, &trail.outline, (scale, ox, oy));
            }
        }
        if !any {
            *builder = Some(path);
            return;
        }
        if let Some(path) = path.finish() {
            let mut paint = Paint::default();
            // Excalidraw's DEFAULT_LASER_COLOR is "red".
            let (r, b) = if bgra { (0, 255) } else { (255, 0) };
            paint.set_color_rgba8(r, 0, b, 255);
            paint.anti_alias = true;
            pm.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
            *builder = Some(path.clear());
        }
    }
}

#[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
fn pixels(v: u32) -> f32 {
    v as f32
}

/// Appends one closed trail to `path`; false if it has too few points.
fn trail_path(
    path: &mut PathBuilder,
    outline: &[[f32; 2]],
    (scale, ox, oy): (f32, f32, f32),
) -> bool {
    let at = |i: usize| (outline[i][0] * scale - ox, outline[i][1] * scale - oy);
    let (x0, y0) = at(0);
    let (x1, y1) = at(1);
    let (x2, y2) = at(2);
    let mut end = (f32::midpoint(x1, x2), f32::midpoint(y1, y2));
    path.move_to(x0, y0);
    path.quad_to(x1, y1, end.0, end.1);
    let mut control = (x1, y1);
    for i in 2..outline.len() - 1 {
        let (ax, ay) = at(i);
        let (bx, by) = at(i + 1);
        control = (2.0 * end.0 - control.0, 2.0 * end.1 - control.1);
        end = (f32::midpoint(ax, bx), f32::midpoint(ay, by));
        path.quad_to(control.0, control.1, end.0, end.1);
    }
    path.close();
    true
}
