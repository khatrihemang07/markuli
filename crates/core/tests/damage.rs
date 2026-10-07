//! Seam 1, pixel output: the damage region returned by `render` covers every
//! pixel that changed, so an incrementally updated buffer stays equal to a
//! from-scratch render.

#![allow(clippy::cast_precision_loss, reason = "tiny test values")]

use markuli_core::{Annotator, Damage, DisplayId, Event, Format, Point};
use tiny_skia::Pixmap;

const W: u32 = 240;
const H: u32 = 180;

struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / 16_777_216.0
    }
}

fn render(a: &mut Annotator, pm: &mut Pixmap, format: Format) -> Option<Damage> {
    let mut m = pm.as_mut();
    a.render(&mut m, format)
}

/// What a from-scratch render of the current Ink looks like.
fn fresh(a: &mut Annotator, format: Format) -> Pixmap {
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.handle(Event::SurfaceReset);
    render(a, &mut pm, format);
    pm
}

fn pixel(pm: &Pixmap, x: u32, y: u32) -> &[u8] {
    let i = ((y * W + x) * 4) as usize;
    &pm.data()[i..i + 4]
}

/// The rasterizer's edge coverage can flip by a sample (16/255) on edges whose
/// chopping differs between a band and the full surface, even far from the
/// change. A missed damage region is a whole stroke fringe, off by ~255.
fn far(a: &[u8], b: &[u8]) -> bool {
    a.iter().zip(b).any(|(p, q)| p.abs_diff(*q) > 64)
}

/// Renders incrementally and checks against from-scratch renders: every pixel
/// that changed since the previous frame lies inside the returned damage, and
/// the incrementally updated buffer matches a fresh one.
fn check(a: &mut Annotator, pm: &mut Pixmap, before: &mut Pixmap, format: Format, what: &str) {
    let damage = render(a, pm, format);
    let now = fresh(a, format);
    for y in 0..H {
        for x in 0..W {
            let inside = damage
                .is_some_and(|d| x >= d.x && x < d.x + d.width && y >= d.y && y < d.y + d.height);
            assert!(
                inside || !far(pixel(before, x, y), pixel(&now, x, y)),
                "{what}: pixel {x},{y} changed outside {damage:?}"
            );
            assert!(
                !far(pixel(pm, x, y), pixel(&now, x, y)),
                "{what}: pixel {x},{y} differs from a fresh render"
            );
        }
    }
    *before = now;
}

fn run(scale: f32, format: Format, pressure: bool) {
    let mut a = Annotator::new();
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    render(&mut a, &mut pm, format);
    let mut before = fresh(&mut a, format);
    let mut rng = Lcg(7);
    let (w, h) = (W as f32, H as f32);
    for stroke in 0..5 {
        let mut at = Point {
            x: rng.next() * w,
            y: rng.next() * h,
        };
        a.handle(Event::Pressure(pressure.then(|| 0.1 + rng.next() * 0.8)));
        a.handle(Event::PointerDown(at));
        check(
            &mut a,
            &mut pm,
            &mut before,
            format,
            &format!("stroke {stroke} down"),
        );
        for step in 0..5 + stroke * 12 {
            // Mixed fast and slow movement, including a sharp turn now and then.
            let speed = if step % 7 == 0 { 30.0 } else { 3.0 };
            at.x = (at.x + (rng.next() - 0.5) * speed * 2.0).clamp(-10.0, w + 10.0);
            at.y = (at.y + (rng.next() - 0.5) * speed * 2.0).clamp(-10.0, h + 10.0);
            a.handle(Event::Pressure(pressure.then(|| 0.1 + rng.next() * 0.8)));
            a.handle(Event::PointerMove(at));
            let what = format!("stroke {stroke} step {step}");
            check(&mut a, &mut pm, &mut before, format, &what);
        }
        a.handle(Event::PointerUp(at));
        let what = format!("stroke {stroke} commit");
        check(&mut a, &mut pm, &mut before, format, &what);
    }
}

#[test]
fn incremental_damage_covers_all_changed_pixels_with_simulated_pressure() {
    run(1.0, Format::Rgba, false);
}

#[test]
fn incremental_damage_covers_all_changed_pixels_with_stylus_pressure_on_bgra() {
    run(1.0, Format::Bgra, true);
}

#[test]
fn incremental_damage_covers_all_changed_pixels_on_a_retina_scale() {
    run(2.0, Format::Rgba, false);
}
