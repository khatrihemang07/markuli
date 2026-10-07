//! Seam 1: scripted events in, view state / Ink / pixels out.

use markuli_core::{Annotator, DisplayId, Event, Key, Point, ToolKind};

const D1: DisplayId = DisplayId::new(1);
const D2: DisplayId = DisplayId::new(2);

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn stroke(a: &mut Annotator, points: &[Point]) {
    a.handle(Event::PointerDown(points[0]));
    for &pt in &points[1..] {
        a.handle(Event::PointerMove(pt));
    }
    a.handle(Event::PointerUp(points[points.len() - 1]));
}

#[test]
fn starts_idle_with_no_overlay_and_leaves_nothing_to_draw() {
    let a = Annotator::new();
    let v = a.view();
    assert!(!v.draw_mode);
    assert!(!v.overlay_needed);
    assert!(a.ink().is_empty());
}

#[test]
fn toggling_enters_draw_mode_on_that_display_and_captures_input() {
    let mut a = Annotator::new();
    let v = a.handle(Event::ToggleDrawMode(D1));
    assert!(v.draw_mode);
    assert!(v.overlay_needed);
    assert!(!v.click_through());
    assert_eq!(v.display, Some(D1));
}

#[test]
fn toggling_again_with_empty_ink_releases_the_overlay() {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    let v = a.handle(Event::ToggleDrawMode(D1));
    assert!(!v.draw_mode);
    assert!(!v.overlay_needed);
}

#[test]
fn a_stroke_in_draw_mode_adds_one_element_with_its_points() {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    stroke(&mut a, &[p(10.0, 10.0), p(20.0, 15.0), p(30.0, 40.0)]);
    let ink = a.ink();
    assert_eq!(ink.len(), 1);
    assert_eq!(
        ink.elements()[0].points(),
        // Pointer up always appends its position, like Excalidraw.
        &[p(0.0, 0.0), p(10.0, 5.0), p(20.0, 30.0), p(20.0, 30.0)]
    );
    assert_eq!((ink.elements()[0].x(), ink.elements()[0].y()), (10.0, 10.0));
}

#[test]
fn pointer_events_outside_draw_mode_never_reach_ink() {
    let mut a = Annotator::new();
    stroke(&mut a, &[p(1.0, 1.0), p(5.0, 5.0)]);
    assert!(a.ink().is_empty());
}

#[test]
fn ink_stays_and_overlay_becomes_click_through_after_leaving_draw_mode() {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    stroke(&mut a, &[p(10.0, 10.0), p(20.0, 20.0)]);
    let v = a.handle(Event::ToggleDrawMode(D1));
    assert!(!v.draw_mode);
    assert!(v.click_through());
    assert!(v.overlay_needed);
    assert_eq!(a.ink().len(), 1);
    // Strokes are ignored again.
    stroke(&mut a, &[p(50.0, 50.0), p(60.0, 60.0)]);
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn leaving_draw_mode_mid_stroke_keeps_the_stroke_and_ignores_the_rest() {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    a.handle(Event::PointerDown(p(10.0, 10.0)));
    a.handle(Event::PointerMove(p(20.0, 20.0)));
    a.handle(Event::ToggleDrawMode(D1));
    a.handle(Event::PointerMove(p(90.0, 90.0)));
    a.handle(Event::PointerUp(p(90.0, 90.0)));
    assert_eq!(a.ink().len(), 1);
    assert_eq!(a.ink().elements()[0].points().len(), 2);
}

#[test]
fn toggling_on_another_display_moves_the_overlay_and_clears_ink() {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    stroke(&mut a, &[p(10.0, 10.0), p(20.0, 20.0)]);
    a.handle(Event::ToggleDrawMode(D1));
    let v = a.handle(Event::ToggleDrawMode(D2));
    assert!(v.draw_mode);
    assert_eq!(v.display, Some(D2));
    assert!(a.ink().is_empty());
}

#[test]
fn toggling_on_the_same_display_keeps_ink() {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    stroke(&mut a, &[p(10.0, 10.0), p(20.0, 20.0)]);
    a.handle(Event::ToggleDrawMode(D1));
    a.handle(Event::ToggleDrawMode(D1));
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn entering_draw_mode_always_starts_with_the_pen() {
    // Story 14: the Pen is the default Tool when Draw Mode starts.
    let mut a = Annotator::new();
    assert_eq!(a.handle(Event::ToggleDrawMode(D1)).tool, ToolKind::Pen);
    for (c, kind) in [
        ('e', ToolKind::Eraser),
        ('k', ToolKind::Laser),
        ('v', ToolKind::Select),
    ] {
        a.handle(Event::Key {
            key: Key::Char(c),
            command: false,
            shift: false,
            alt: false,
        });
        assert_eq!(a.view().tool, kind);
        a.handle(Event::ToggleDrawMode(D1));
        let v = a.handle(Event::ToggleDrawMode(D1));
        assert_eq!(v.tool, ToolKind::Pen, "re-entered after {kind:?}");
    }
}

#[test]
fn tool_keys_do_nothing_while_alt_is_held() {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    let key = |a: &mut Annotator, c, alt| {
        a.handle(Event::Key {
            key: Key::Char(c),
            command: false,
            shift: false,
            alt,
        })
        .tool
    };
    assert_eq!(key(&mut a, 'e', true), ToolKind::Pen);
    assert_eq!(key(&mut a, 'e', false), ToolKind::Eraser);
}
