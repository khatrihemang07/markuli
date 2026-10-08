//! Seam 1, pixel output: a Stroke drawn live (one render per pointer event) must
//! look like the same Stroke rendered once.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    reason = "test pixel coordinates are small and positive"
)]

use markuli_core::{Annotator, DisplayId, Event, Format, Point};
use tiny_skia::Pixmap;

const W: u32 = 1100;
const H: u32 = 500;
const Y: f32 = 400.0;

fn render(a: &mut Annotator, pm: &mut Pixmap) {
    a.render(&mut pm.as_mut(), Format::Rgba);
}

fn scene() -> (Annotator, Pixmap) {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    render(&mut a, &mut pm);
    (a, pm)
}

/// Draws the Stroke point by point, rendering after every event.
fn live(steps: &[f32], commit: bool) -> (Annotator, Pixmap) {
    let (mut a, mut pm) = scene();
    let p = |x: f32| Point { x, y: Y };
    a.handle(Event::PointerDown(p(steps[0])));
    render(&mut a, &mut pm);
    for &x in &steps[1..] {
        a.handle(Event::PointerMove(p(x)));
        render(&mut a, &mut pm);
    }
    if commit {
        a.handle(Event::PointerUp(p(steps[steps.len() - 1])));
        render(&mut a, &mut pm);
    }
    (a, pm)
}

/// A real drag: events dropped while the Overlay was starting, so the third
/// point is far from the second, then even steps.
fn xs() -> Vec<f32> {
    let mut v = vec![250.0, 269.0, 326.0];
    v.extend((1..38).map(|i| 326.0 + 19.0 * i as f32));
    v
}

#[test]
fn live_drawn_stroke_equals_one_full_render() {
    let (mut a, pm) = live(&xs(), true);
    a.handle(Event::SurfaceReset);
    let mut full = Pixmap::new(W, H).expect("pixmap");
    render(&mut a, &mut full);
    let worst = pm
        .data()
        .iter()
        .zip(full.data())
        .map(|(p, q)| p.abs_diff(*q))
        .max()
        .unwrap_or(0);
    assert!(
        worst <= 24,
        "live render differs from a full one by {worst}"
    );
}
