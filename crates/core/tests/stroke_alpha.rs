//! Seam 1, pixel output: a semi-transparent Stroke drawn live (one render per
//! pointer event) must look like the same Stroke rendered once, and its start
//! must not be darker than its middle (nothing is composited twice).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    reason = "test pixel coordinates are small and positive"
)]

use markuli_core::{Annotator, Control, DisplayId, Event, Format, Point};
use tiny_skia::Pixmap;

const W: u32 = 1100;
const H: u32 = 500;
const Y: f32 = 400.0;

fn render(a: &mut Annotator, pm: &mut Pixmap) {
    a.render(&mut pm.as_mut(), Format::Rgba);
}

fn alpha(pm: &Pixmap, x: u32, y: u32) -> u8 {
    pm.data()[((y * W + x) * 4 + 3) as usize]
}

/// Alpha at the stroke's centre line over `x`, the largest value in the
/// column (the stroke is a few pixels thick).
fn column(pm: &Pixmap, x: u32) -> u8 {
    (Y as u32 - 4..=Y as u32 + 4)
        .map(|y| alpha(pm, x, y))
        .max()
        .unwrap_or(0)
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
    let at = a.panel_center(Control::Opacity(40)).expect("panel");
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerUp(at));
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

#[test]
fn the_start_of_a_semi_transparent_stroke_is_not_darker_than_its_middle() {
    let (_, pm) = live(&xs(), true);
    let middle = column(&pm, 600);
    assert!(middle > 90 && middle < 115, "40% opacity, got {middle}");
    for x in 275..420 {
        let here = column(&pm, x);
        assert!(
            here <= middle + 8,
            "x={x}: alpha {here} is darker than the middle ({middle})"
        );
    }
}

#[test]
fn the_stroke_in_progress_matches_the_committed_one() {
    let (_, in_progress) = live(&xs(), false);
    let (_, committed) = live(&xs(), true);
    for x in 260..700 {
        let (a, b) = (column(&in_progress, x), column(&committed, x));
        assert!(a.abs_diff(b) <= 24, "x={x}: live {a} vs committed {b}");
    }
}
