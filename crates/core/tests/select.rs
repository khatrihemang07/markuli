//! Seam 1: the Select Tool through the annotator interface only.

use markuli_core::{Annotator, DisplayId, Event, Key, Point};

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn session() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: 800,
        height: 600,
    });
    a.handle(Event::ToggleDrawMode(DisplayId(1)));
    a
}

fn key(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: false,
        shift: false,
    });
}

fn command(a: &mut Annotator, c: char, shift: bool) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: true,
        shift,
    });
}

fn press(a: &mut Annotator, key: Key) {
    a.handle(Event::Key {
        key,
        command: false,
        shift: false,
    });
}

/// A Pen stroke through `points`; leaves the Pen active.
fn stroke(a: &mut Annotator, points: &[(f32, f32)]) {
    key(a, 'p');
    a.handle(Event::PointerDown(p(points[0].0, points[0].1)));
    for &(x, y) in &points[1..] {
        a.handle(Event::PointerMove(p(x, y)));
    }
    let (x, y) = points[points.len() - 1];
    a.handle(Event::PointerUp(p(x, y)));
}

fn drag(a: &mut Annotator, from: (f32, f32), to: (f32, f32)) {
    a.handle(Event::PointerDown(p(from.0, from.1)));
    let steps = 5_u8;
    for i in 1..=steps {
        let t = f32::from(i) / f32::from(steps);
        a.handle(Event::PointerMove(p(
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
        )));
    }
    a.handle(Event::PointerUp(p(to.0, to.1)));
}

fn click(a: &mut Annotator, at: (f32, f32)) {
    a.handle(Event::PointerDown(p(at.0, at.1)));
    a.handle(Event::PointerUp(p(at.0, at.1)));
}

fn shift(a: &mut Annotator, held: bool) {
    a.handle(Event::Modifiers { shift: held });
}

fn ids(a: &Annotator) -> Vec<u64> {
    a.ink().elements().iter().map(|e| e.id()).collect()
}

/// Two horizontal strokes, A at y=100 and B at y=300, and the Select Tool.
fn two_strokes() -> (Annotator, u64, u64) {
    let mut a = session();
    stroke(&mut a, &[(100.0, 100.0), (150.0, 100.0), (200.0, 100.0)]);
    stroke(&mut a, &[(100.0, 300.0), (150.0, 300.0), (200.0, 300.0)]);
    key(&mut a, 'v');
    let ids = ids(&a);
    (a, ids[0], ids[1])
}

#[test]
fn v_and_1_select_the_select_tool_and_p_returns_to_the_pen() {
    let mut a = session();
    let pen = a.active_tool();
    key(&mut a, 'v');
    let select = a.active_tool();
    assert_ne!(select, pen);
    key(&mut a, 'p');
    assert_eq!(a.active_tool(), pen);
    key(&mut a, '1');
    assert_eq!(a.active_tool(), select);
}

#[test]
fn click_selects_the_element_under_the_pointer_and_empty_space_deselects() {
    let (mut a, first, second) = two_strokes();
    click(&mut a, (150.0, 103.0));
    assert_eq!(a.selection(), [first]);
    click(&mut a, (150.0, 302.0));
    assert_eq!(a.selection(), [second]);
    click(&mut a, (500.0, 450.0));
    assert!(a.selection().is_empty());
}

#[test]
fn click_hits_within_eight_pixels_of_the_stroke_and_no_further() {
    let (mut a, first, _) = two_strokes();
    click(&mut a, (150.0, 108.0));
    assert_eq!(a.selection(), [first]);
    click(&mut a, (500.0, 450.0));
    click(&mut a, (150.0, 110.0));
    assert!(a.selection().is_empty());
}

#[test]
fn click_picks_the_topmost_of_overlapping_elements() {
    let mut a = session();
    stroke(&mut a, &[(100.0, 200.0), (200.0, 200.0)]);
    stroke(&mut a, &[(150.0, 150.0), (150.0, 250.0)]);
    key(&mut a, 'v');
    click(&mut a, (150.0, 200.0));
    assert_eq!(a.selection(), [ids(&a)[1]]);
}

#[test]
fn shift_click_adds_and_removes_from_the_selection() {
    let (mut a, first, second) = two_strokes();
    click(&mut a, (150.0, 100.0));
    shift(&mut a, true);
    click(&mut a, (150.0, 300.0));
    assert_eq!(a.selection(), [first, second]);
    click(&mut a, (150.0, 100.0));
    assert_eq!(a.selection(), [second]);
    shift(&mut a, false);
}

#[test]
fn dragging_on_empty_space_selects_the_elements_inside_the_box() {
    let (mut a, first, _) = two_strokes();
    drag(&mut a, (50.0, 50.0), (250.0, 200.0));
    assert_eq!(a.selection(), [first]);
    drag(&mut a, (50.0, 50.0), (250.0, 400.0));
    assert_eq!(a.selection().len(), 2);
}

#[test]
fn a_box_only_selects_elements_it_fully_encloses() {
    let (mut a, _, _) = two_strokes();
    drag(&mut a, (150.0, 50.0), (250.0, 200.0));
    assert!(a.selection().is_empty());
}

#[test]
fn shift_box_keeps_the_existing_selection() {
    let (mut a, first, second) = two_strokes();
    click(&mut a, (150.0, 100.0));
    shift(&mut a, true);
    drag(&mut a, (50.0, 250.0), (250.0, 400.0));
    shift(&mut a, false);
    assert_eq!(a.selection(), [first, second]);
}

#[test]
fn cmd_a_selects_all_ink_even_from_the_pen() {
    let (mut a, first, second) = two_strokes();
    key(&mut a, 'p');
    let pen = a.active_tool();
    command(&mut a, 'a', false);
    assert_eq!(a.selection(), [first, second]);
    assert_ne!(a.active_tool(), pen, "Cmd+A leaves the Pen for the Select Tool");
}

#[test]
fn escape_deselects_but_keeps_the_ink() {
    let (mut a, _, _) = two_strokes();
    command(&mut a, 'a', false);
    press(&mut a, Key::Escape);
    assert!(a.selection().is_empty());
    assert_eq!(a.ink().len(), 2);
}

#[test]
fn switching_to_another_tool_drops_the_selection() {
    let (mut a, _, _) = two_strokes();
    command(&mut a, 'a', false);
    key(&mut a, 'p');
    assert!(a.selection().is_empty());
}

#[test]
fn leaving_draw_mode_drops_the_selection_but_keeps_the_ink() {
    let (mut a, _, _) = two_strokes();
    command(&mut a, 'a', false);
    a.handle(Event::ToggleDrawMode(DisplayId(1)));
    assert!(a.selection().is_empty());
    assert_eq!(a.ink().len(), 2);
}
