//! Seam 1, pixel output: the damage region returned by `render` covers every
//! pixel that changed, so an incrementally updated buffer always equals a
//! from-scratch render.

#![allow(clippy::cast_precision_loss, reason = "tiny test values")]

use markuli_core::{Annotator, DisplayId, Event, Format, Point};
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

fn render(a: &mut Annotator, pm: &mut Pixmap, format: Format) {
    let mut m = pm.as_mut();
    a.render(&mut m, format);
}

/// Largest per-channel difference between the incremental and a fresh render.
fn drift(a: &mut Annotator, incremental: &Pixmap, format: Format) -> u8 {
    let mut fresh = Pixmap::new(incremental.width(), incremental.height()).expect("pixmap");
    a.handle(Event::SurfaceReset);
    render(a, &mut fresh, format);
    incremental
        .data()
        .iter()
        .zip(fresh.data())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

fn run(scale: f32, format: Format, pressure: bool) {
    let mut a = Annotator::new();
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::ToggleDrawMode(DisplayId(1)));
    render(&mut a, &mut pm, format);
    let mut rng = Lcg(7);
    let (w, h) = (W as f32, H as f32);
    for stroke in 0..8 {
        let mut at = Point {
            x: rng.next() * w,
            y: rng.next() * h,
        };
        a.handle(Event::Pressure(pressure.then(|| 0.1 + rng.next() * 0.8)));
        a.handle(Event::PointerDown(at));
        let steps = 5 + stroke * 12;
        for step in 0..steps {
            // Mixed fast and slow movement, including a sharp turn now and then.
            let speed = if step % 7 == 0 { 30.0 } else { 3.0 };
            at.x = (at.x + (rng.next() - 0.5) * speed * 2.0).clamp(-10.0, w + 10.0);
            at.y = (at.y + (rng.next() - 0.5) * speed * 2.0).clamp(-10.0, h + 10.0);
            a.handle(Event::Pressure(pressure.then(|| 0.1 + rng.next() * 0.8)));
            a.handle(Event::PointerMove(at));
            render(&mut a, &mut pm, format);
            assert!(drift(&mut a, &pm, format) <= 2, "stroke {stroke} step {step}");
        }
        a.handle(Event::PointerUp(at));
        render(&mut a, &mut pm, format);
        assert!(drift(&mut a, &pm, format) <= 2, "stroke {stroke} commit");
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
