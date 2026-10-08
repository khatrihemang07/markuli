//! Seam 1, pixels: how a Pen Stroke looks, at a fixed scale factor.
//!
//! `fixtures/pen_golden.png` is a golden image (blessed with
//! `MARKULI_BLESS=1 cargo test -p markuli-core --test pen_render`). Set
//! `MARKULI_DUMP_DIR=/some/dir` to also write what the core rendered there.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "tiny test values"
)]

mod support;

use markuli_core::{Annotator, DisplayId, Event, Format, Point};
use tiny_skia::Pixmap;

const SCALE: f32 = 2.0;
const W: u32 = 480;
const H: u32 = 300;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn scene() -> (Annotator, Pixmap) {
    let mut a = Annotator::new();
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.handle(Event::ScaleFactor(SCALE));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a.render(&mut pm.as_mut(), Format::Rgba);
    (a, pm)
}

/// Draws a Stroke through `at` (logical px), `pressure` per point if given.
fn stroke(
    a: &mut Annotator,
    pm: &mut Pixmap,
    at: &[(f32, f32)],
    pressure: impl Fn(usize) -> Option<f32>,
) {
    let phys = |i: usize| p(at[i].0 * SCALE, at[i].1 * SCALE);
    a.handle(Event::Pressure(pressure(0)));
    a.handle(Event::PointerDown(phys(0)));
    for i in 1..at.len() - 1 {
        a.handle(Event::Pressure(pressure(i)));
        a.handle(Event::PointerMove(phys(i)));
        a.render(&mut pm.as_mut(), Format::Rgba);
    }
    a.handle(Event::Pressure(pressure(at.len() - 1)));
    a.handle(Event::PointerUp(phys(at.len() - 1)));
    a.render(&mut pm.as_mut(), Format::Rgba);
}

/// A horizontal line from x=10 to x=150 with a constant step between samples.
fn line(y: f32, step: f32) -> Vec<(f32, f32)> {
    let n = (140.0 / step) as usize;
    (0..=n).map(|i| (10.0 + i as f32 * step, y)).collect()
}

/// Ink thickness in logical px of the column at logical `x`, within the rows
/// `y0..y1` (logical): summed coverage.
fn thickness(pm: &Pixmap, x: f32, y0: f32, y1: f32) -> f32 {
    let col = (x * SCALE) as u32;
    let rows = (y0 * SCALE) as u32..(y1 * SCALE) as u32;
    let alpha: u32 = rows
        .map(|y| u32::from(pm.data()[((y * W + col) * 4 + 3) as usize]))
        // The Draw Mode backdrop is alpha 1; ignore it.
        .map(|a| if a <= 1 { 0 } else { a })
        .sum();
    alpha as f32 / 255.0 / SCALE
}

#[test]
fn a_mouse_stroke_gets_thinner_the_faster_it_moves() {
    let (mut a, mut pm) = scene();
    stroke(&mut a, &mut pm, &line(30.0, 1.5), |_| None);
    stroke(&mut a, &mut pm, &line(70.0, 20.0), |_| None);
    let slow = thickness(&pm, 100.0, 0.0, 50.0);
    let fast = thickness(&pm, 100.0, 50.0, 100.0);
    assert!(slow > fast + 1.0, "slow {slow} vs fast {fast}");
    // Medium width is perfect-freehand size 8.5; its radius is size * easeOutSine(..), so a slow stroke is up to ~16 px across.
    assert!(slow > 8.0 && slow < 18.0, "slow {slow}");
}

#[test]
fn stylus_pressure_sets_thickness() {
    let (mut a, mut pm) = scene();
    stroke(&mut a, &mut pm, &line(30.0, 4.0), |_| Some(0.15));
    stroke(&mut a, &mut pm, &line(70.0, 4.0), |_| Some(0.95));
    let light = thickness(&pm, 80.0, 0.0, 50.0);
    let heavy = thickness(&pm, 80.0, 50.0, 100.0);
    assert!(heavy > light * 1.8, "light {light} heavy {heavy}");
}

#[test]
fn opacity_and_color_come_from_the_element_defaults() {
    let (mut a, mut pm) = scene();
    stroke(&mut a, &mut pm, &line(50.0, 2.0), |_| None);
    let i = ((50.0 * SCALE) as u32 * W + (80.0 * SCALE) as u32) as usize * 4;
    assert_eq!(&pm.data()[i..i + 4], &[0xe0, 0x31, 0x31, 0xff]);
}

fn golden_scene() -> Pixmap {
    let (mut a, mut pm) = scene();
    // A wave drawn fast, then slow: the speed change shows in the width.
    let mut x = 10.0;
    let wave: Vec<(f32, f32)> = (0..70)
        .map(|i| {
            x += if i < 25 { 5.0 } else { 1.6 };
            (x, 30.0 + 18.0 * (i as f32 / 8.0).sin())
        })
        .collect();
    stroke(&mut a, &mut pm, &wave, |_| None);
    // A stylus stroke swelling from light to heavy pressure.
    let swell: Vec<(f32, f32)> = (0..40)
        .map(|i| (10.0 + i as f32 * 3.5, 80.0 - (i as f32 / 6.0).sin() * 8.0))
        .collect();
    stroke(&mut a, &mut pm, &swell, |i| {
        Some(0.1 + 0.85 * i as f32 / 39.0)
    });
    // A click makes a dot, and a nearly closed ring gets closed.
    stroke(&mut a, &mut pm, &[(20.0, 120.0), (20.0, 120.0)], |_| None);
    let ring: Vec<(f32, f32)> = (0..=24)
        .map(|i| {
            let t = i as f32 / 24.0 * std::f32::consts::TAU;
            (110.0 + 30.0 * t.cos(), 118.0 + 18.0 * t.sin())
        })
        .collect();
    stroke(&mut a, &mut pm, &ring, |_| None);
    pm
}

#[test]
fn a_rendered_pen_scene_matches_the_golden_image() {
    let pm = golden_scene();
    support::dump("pen_scene", &pm, [255, 255, 255]);
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pen_golden.png");
    if std::env::var("MARKULI_BLESS").is_ok() {
        std::fs::write(path, support::encode(&pm, [255, 255, 255])).expect("bless golden");
    }
    let golden = std::fs::read(path).expect("golden image exists (bless with MARKULI_BLESS=1)");
    let (w, h, rgb) = support::decode_rgb(&golden);
    assert_eq!((w, h), (W, H));
    let ours = support::decode_rgb(&support::encode(&pm, [255, 255, 255])).2;
    let off = rgb
        .iter()
        .zip(&ours)
        .filter(|(g, o)| g.abs_diff(**o) > 24)
        .count();
    // A coverage-sample flip on a few edge pixels is fine; a changed shape is not.
    assert!(
        off <= 30,
        "{off} channel values differ from the golden image"
    );
}
