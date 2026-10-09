//! Seam: the remembered Style through the annotator interface only.

use markuli_core::{Annotator, Button, DisplayId, Element, Event, Key, Point, Style};

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn session() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: 800,
        height: 600,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a
}

fn key(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: false,
        shift: false,
        alt: false,
    });
}

fn stroke(a: &mut Annotator, y: f32) {
    a.handle(Event::PointerDown(p(300.0, y)));
    a.handle(Event::PointerMove(p(350.0, y)));
    a.handle(Event::PointerUp(p(400.0, y)));
}

fn pick(a: &mut Annotator, control: Button) {
    let at = a.button_center(control).expect("the toolbar is shown");
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerUp(at));
}

fn last(a: &Annotator) -> &Element {
    a.ink().elements().last().expect("a stroke")
}

const BLUE: [u8; 3] = [0x19, 0x71, 0xc2];
const GREEN: [u8; 3] = [0x2f, 0x9e, 0x44];

#[test]
fn the_view_reports_the_default_style() {
    assert_eq!(session().view().style, Style { color: 0, width: 1 });
}

#[test]
fn the_startup_style_sets_the_next_strokes_look() {
    let mut a = Annotator::new();
    a.handle(Event::Style(Style { color: 1, width: 2 }));
    a.handle(Event::Resize {
        width: 800,
        height: 600,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    stroke(&mut a, 100.0);
    assert_eq!(last(&a).stroke_color(), BLUE);
    assert!((last(&a).stroke_width() - 8.0).abs() < f32::EPSILON);
    assert_eq!(a.view().style, Style { color: 1, width: 2 });
}

#[test]
fn out_of_range_indices_are_ignored() {
    let mut a = session();
    a.handle(Event::Style(Style { color: 9, width: 1 }));
    assert_eq!(a.view().style, Style::default());
    a.handle(Event::Style(Style { color: 2, width: 3 }));
    assert_eq!(a.view().style, Style::default());
}

#[test]
fn the_view_reports_the_style_after_a_click() {
    let mut a = session();
    pick(&mut a, Button::Color(2));
    assert_eq!(a.view().style, Style { color: 2, width: 1 });
    pick(&mut a, Button::Width(0));
    assert_eq!(a.view().style, Style { color: 2, width: 0 });
    stroke(&mut a, 100.0);
    assert_eq!(last(&a).stroke_color(), GREEN);
}

#[test]
fn the_style_survives_leaving_and_reentering_draw_mode() {
    let mut a = session();
    pick(&mut a, Button::Color(3));
    pick(&mut a, Button::Width(2));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    assert_eq!(a.view().style, Style { color: 3, width: 2 });
}

#[test]
fn a_selection_restyle_does_not_change_the_reported_style() {
    let mut a = session();
    stroke(&mut a, 100.0);
    key(&mut a, 'v');
    a.handle(Event::PointerDown(p(350.0, 100.0)));
    a.handle(Event::PointerUp(p(350.0, 100.0)));
    assert!(!a.selection().is_empty(), "the stroke is selected");
    pick(&mut a, Button::Color(2));
    assert_eq!(last(&a).stroke_color(), GREEN);
    assert_eq!(a.view().style, Style::default());
}

#[test]
fn color_and_width_keys_update_the_view_style() {
    let mut a = session();
    key(&mut a, '3');
    assert_eq!(a.view().style, Style { color: 2, width: 1 });
    key(&mut a, ']');
    assert_eq!(a.view().style, Style { color: 2, width: 2 });
    key(&mut a, '[');
    key(&mut a, '[');
    assert_eq!(a.view().style, Style { color: 2, width: 0 });
}

#[test]
fn the_default_style_draws_what_a_fresh_session_draws() {
    let mut fresh = session();
    stroke(&mut fresh, 100.0);
    let mut a = session();
    a.handle(Event::Style(Style::default()));
    stroke(&mut a, 100.0);
    assert_eq!(last(&a).stroke_color(), [0xe0, 0x31, 0x31]);
    assert!((last(&a).stroke_width() - 4.0).abs() < f32::EPSILON);
    assert_eq!(last(&a).stroke_color(), last(&fresh).stroke_color());
    assert!((last(&a).stroke_width() - last(&fresh).stroke_width()).abs() < f32::EPSILON);
}
