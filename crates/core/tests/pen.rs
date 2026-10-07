//! Seam 1: the Pen through the annotator core interface.

use markuli_core::{Annotator, DisplayId, Event, Point};

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn drawing() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(DisplayId(1)));
    a
}

fn stroke(a: &mut Annotator, at: &[Point]) {
    a.handle(Event::PointerDown(at[0]));
    for q in &at[1..at.len() - 1] {
        a.handle(Event::PointerMove(*q));
    }
    a.handle(Event::PointerUp(at[at.len() - 1]));
}

#[test]
fn pointer_events_without_pressure_simulate_it_and_store_no_pressures() {
    let mut a = drawing();
    stroke(&mut a, &[p(10.0, 10.0), p(20.0, 15.0), p(30.0, 40.0)]);
    let e = &a.ink().elements()[0];
    assert!(e.simulate_pressure());
    assert!(e.pressures().is_empty());
}

#[test]
fn pointer_events_with_pressure_store_one_pressure_per_point() {
    let mut a = drawing();
    a.handle(Event::Pressure(Some(0.2)));
    a.handle(Event::PointerDown(p(10.0, 10.0)));
    a.handle(Event::Pressure(Some(0.6)));
    a.handle(Event::PointerMove(p(20.0, 15.0)));
    a.handle(Event::Pressure(Some(0.9)));
    a.handle(Event::PointerMove(p(30.0, 40.0)));
    a.handle(Event::Pressure(Some(0.4)));
    a.handle(Event::PointerUp(p(35.0, 45.0)));
    let e = &a.ink().elements()[0];
    assert!(!e.simulate_pressure());
    assert_eq!(e.points().len(), 4);
    assert_eq!(e.pressures(), &[0.2, 0.6, 0.9, 0.4]);
}

#[test]
fn a_pressure_of_exactly_one_half_is_treated_as_simulated_like_excalidraw() {
    let mut a = drawing();
    a.handle(Event::Pressure(Some(0.5)));
    stroke(&mut a, &[p(10.0, 10.0), p(30.0, 40.0)]);
    let e = &a.ink().elements()[0];
    assert!(e.simulate_pressure());
    assert!(e.pressures().is_empty());
}

#[test]
fn pressure_applies_per_stroke_and_mouse_events_after_a_pen_simulate_again() {
    let mut a = drawing();
    a.handle(Event::Pressure(Some(0.8)));
    stroke(&mut a, &[p(10.0, 10.0), p(30.0, 40.0)]);
    a.handle(Event::Pressure(None));
    stroke(&mut a, &[p(10.0, 10.0), p(30.0, 40.0)]);
    assert!(!a.ink().elements()[0].simulate_pressure());
    assert!(a.ink().elements()[1].simulate_pressure());
}

#[test]
fn elements_have_excalidraw_defaults_and_relative_points() {
    let mut a = drawing();
    stroke(&mut a, &[p(10.0, 20.0), p(20.0, 25.0), p(30.0, 60.0)]);
    let e = &a.ink().elements()[0];
    assert_eq!((e.x(), e.y()), (10.0, 20.0));
    assert_eq!(e.points(), &[p(0.0, 0.0), p(10.0, 5.0), p(20.0, 40.0)]);
    assert_eq!(e.stroke_color(), [0xe0, 0x31, 0x31]);
    assert!((e.stroke_width() - 2.0).abs() < f32::EPSILON);
    assert_eq!(e.opacity(), 100);
}

#[test]
fn a_click_without_movement_becomes_a_dot_with_excalidraws_nudge() {
    let mut a = drawing();
    stroke(&mut a, &[p(50.0, 50.0), p(50.0, 50.0)]);
    let e = &a.ink().elements()[0];
    assert_eq!(e.points().len(), 2);
    assert!(e.points()[1].x > 0.0 && e.points()[1].x < 0.001);
}

#[test]
fn a_stroke_ending_within_eight_pixels_of_its_start_closes_the_loop() {
    let mut a = drawing();
    stroke(
        &mut a,
        &[
            p(10.0, 10.0),
            p(60.0, 10.0),
            p(60.0, 60.0),
            p(10.0, 60.0),
            p(13.0, 14.0),
        ],
    );
    let e = &a.ink().elements()[0];
    assert_eq!(e.points().last(), Some(&p(0.0, 0.0)));
}

#[test]
fn pointer_positions_are_stored_in_logical_pixels_using_the_scale_factor() {
    let mut a = drawing();
    a.handle(Event::ScaleFactor(2.0));
    stroke(&mut a, &[p(20.0, 40.0), p(60.0, 40.0), p(100.0, 80.0)]);
    let e = &a.ink().elements()[0];
    assert_eq!((e.x(), e.y()), (10.0, 20.0));
    assert_eq!(e.points()[2], p(40.0, 20.0));
}

#[test]
fn invalid_pressure_and_positions_are_ignored_not_fatal() {
    let mut a = drawing();
    a.handle(Event::Pressure(Some(f32::NAN)));
    a.handle(Event::ScaleFactor(0.0));
    a.handle(Event::ScaleFactor(f32::NAN));
    a.handle(Event::PointerDown(p(f32::NAN, 1.0)));
    stroke(&mut a, &[p(1.0, 1.0), p(9.0, 9.0)]);
    assert_eq!(a.ink().len(), 1);
}
