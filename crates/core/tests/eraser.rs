//! Seam 1: the Eraser Tool through the annotator core interface.

use markuli_core::{Annotator, DisplayId, Event, Format, Key, Point};
use tiny_skia::PixmapMut;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn key(a: &mut Annotator, c: char, command: bool, shift: bool) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command,
        shift,
        alt: false,
    });
}

/// Three horizontal strokes at y = 50, 100 and 150, drawn with the Pen; the
/// Eraser is selected afterwards.
fn three_strokes() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    for y in [50.0, 100.0, 150.0] {
        a.handle(Event::PointerDown(p(100.0, y)));
        a.handle(Event::PointerMove(p(200.0, y)));
        a.handle(Event::PointerMove(p(300.0, y)));
        a.handle(Event::PointerUp(p(400.0, y)));
    }
    key(&mut a, 'e', false, false);
    a
}

fn ids(a: &Annotator) -> Vec<u64> {
    a.ink()
        .elements()
        .iter()
        .map(markuli_core::Element::id)
        .collect()
}

fn drag(a: &mut Annotator, path: &[Point]) {
    a.handle(Event::PointerDown(path[0]));
    for q in &path[1..path.len() - 1] {
        a.handle(Event::PointerMove(*q));
    }
    a.handle(Event::PointerUp(path[path.len() - 1]));
}

#[test]
fn a_drag_removes_exactly_the_elements_it_crosses() {
    let mut a = three_strokes();
    let all = ids(&a);
    // Vertical drag at x = 250 from y = 80 to y = 170 crosses the strokes at
    // y = 100 and y = 150 but not the one at y = 50.
    drag(&mut a, &[p(250.0, 80.0), p(250.0, 120.0), p(250.0, 170.0)]);
    assert_eq!(ids(&a), vec![all[0]]);
}

#[test]
fn a_drag_ending_within_eight_pixels_of_a_stroke_hits_it_and_nine_misses() {
    let mut a = three_strokes();
    drag(&mut a, &[p(250.0, 20.0), p(250.0, 41.0)]); // 9 px above y = 50
    assert_eq!(a.ink().len(), 3);
    drag(&mut a, &[p(250.0, 20.0), p(250.0, 43.0)]); // 7 px above y = 50
    assert_eq!(a.ink().len(), 2);
}

#[test]
fn a_click_without_moving_erases_the_element_under_it() {
    let mut a = three_strokes();
    let all = ids(&a);
    a.handle(Event::PointerDown(p(150.0, 102.0)));
    a.handle(Event::PointerUp(p(150.0, 102.0)));
    assert_eq!(ids(&a), vec![all[0], all[2]]);
}

#[test]
fn elements_are_removed_only_when_the_pointer_goes_up() {
    let mut a = three_strokes();
    a.handle(Event::PointerDown(p(250.0, 80.0)));
    a.handle(Event::PointerMove(p(250.0, 120.0)));
    assert_eq!(a.ink().len(), 3, "still there, drawn faded, while dragging");
    a.handle(Event::PointerUp(p(250.0, 130.0)));
    assert_eq!(a.ink().len(), 2);
}

#[test]
fn undo_restores_the_erased_elements_in_their_old_order_and_redo_erases_again() {
    let mut a = three_strokes();
    let all = ids(&a);
    drag(&mut a, &[p(250.0, 20.0), p(250.0, 110.0)]); // erases y = 50 and 100
    assert_eq!(ids(&a), vec![all[2]]);
    key(&mut a, 'z', true, false);
    assert_eq!(ids(&a), all);
    key(&mut a, 'z', true, true);
    assert_eq!(ids(&a), vec![all[2]]);
    key(&mut a, 'z', true, false);
    key(&mut a, 'z', true, false);
    assert_eq!(
        ids(&a),
        vec![all[0], all[1]],
        "the third stroke is undone next"
    );
}

#[test]
fn a_drag_that_crosses_nothing_logs_nothing() {
    let mut a = three_strokes();
    drag(&mut a, &[p(10.0, 10.0), p(20.0, 20.0)]);
    key(&mut a, 'z', true, false);
    assert_eq!(a.ink().len(), 2, "undo reached the last Pen stroke");
}

#[test]
fn escape_cancels_the_erase_and_keeps_everything() {
    let mut a = three_strokes();
    a.handle(Event::PointerDown(p(250.0, 80.0)));
    a.handle(Event::PointerMove(p(250.0, 120.0)));
    a.handle(Event::Key {
        key: Key::Escape,
        command: false,
        shift: false,
        alt: false,
    });
    a.handle(Event::PointerUp(p(250.0, 130.0)));
    assert_eq!(a.ink().len(), 3);
}

#[test]
fn elements_pending_erasure_are_drawn_faded_then_gone() {
    let mut a = three_strokes();
    let mut buf = vec![0_u8; 500 * 200 * 4];
    let alpha_at = |a: &mut Annotator, buf: &mut Vec<u8>| {
        let mut target = PixmapMut::from_bytes(buf, 500, 200).expect("pixmap");
        a.render(&mut target, Format::Rgba);
        // The stroke at y = 100, x = 250: premultiplied red channel.
        buf[(100 * 500 + 250) * 4]
    };
    let solid = alpha_at(&mut a, &mut buf);
    a.handle(Event::PointerDown(p(250.0, 80.0)));
    a.handle(Event::PointerMove(p(250.0, 120.0)));
    let faded = alpha_at(&mut a, &mut buf);
    assert!(solid > 150 && faded < 80, "solid {solid}, faded {faded}");
    a.handle(Event::PointerUp(p(250.0, 130.0)));
    assert_eq!(alpha_at(&mut a, &mut buf), 0, "erased pixels are backdrop");
}
