//! Test-only helpers for the Color Picker and Width Picker tests.

#![allow(dead_code, reason = "each test binary uses a subset")]

use markuli_core::{
    Annotator, Button, DisplayId, Element, Event, Key, PickerPart, Point, ToolbarPosition,
};

pub fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

pub fn session() -> Annotator {
    sized(800, 600, ToolbarPosition::Top)
}

pub fn sized(width: u32, height: u32, position: ToolbarPosition) -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Resize { width, height });
    a.handle(Event::ToolbarPosition(position));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a
}

pub fn center(a: &Annotator, b: Button) -> Point {
    a.button_center(b).expect("the toolbar is shown")
}

/// A secondary click on a Toolbar button.
pub fn open(a: &mut Annotator, b: Button) {
    let at = center(a, b);
    a.handle(Event::SecondaryClick(at));
}

pub fn part(a: &Annotator, part: PickerPart) -> Point {
    a.picker_point(part).expect("the picker shows that part")
}

pub fn is_open(a: &Annotator) -> bool {
    a.picker_point(PickerPart::Reset).is_some()
}

/// A press and release on the same point.
pub fn click(a: &mut Annotator, at: Point) {
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerUp(at));
}

pub fn click_part(a: &mut Annotator, which: PickerPart) {
    let at = part(a, which);
    click(a, at);
}

/// Press at the first part, move through the rest, release on the last.
pub fn drag(a: &mut Annotator, parts: &[PickerPart]) {
    let points: Vec<Point> = parts.iter().map(|&w| part(a, w)).collect();
    let (first, rest) = points.split_first().expect("a start");
    a.handle(Event::PointerDown(*first));
    for &at in rest {
        a.handle(Event::PointerMove(at));
    }
    a.handle(Event::PointerUp(*points.last().expect("an end")));
}

pub fn key(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: false,
        shift: false,
        alt: false,
    });
}

pub fn command(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: true,
        shift: false,
        alt: false,
    });
}

pub fn escape(a: &mut Annotator) {
    a.handle(Event::Key {
        key: Key::Escape,
        command: false,
        shift: false,
        alt: false,
    });
}

pub fn stroke(a: &mut Annotator, y: f32) {
    a.handle(Event::PointerDown(p(300.0, y)));
    a.handle(Event::PointerMove(p(350.0, y)));
    a.handle(Event::PointerUp(p(400.0, y)));
}

pub fn last(a: &Annotator) -> &Element {
    a.ink().elements().last().expect("a stroke")
}

pub fn colors(a: &Annotator) -> Vec<[u8; 3]> {
    a.ink()
        .elements()
        .iter()
        .map(Element::stroke_color)
        .collect()
}
