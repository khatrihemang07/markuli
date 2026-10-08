//! Seam 1: the colour and width buttons of the one Toolbar row, through events.

use markuli_core::{Annotator, Button, Cursor, DisplayId, Event, Format, Key, Point, ToolKind};
use tiny_skia::Pixmap;

const W: u32 = 800;
const H: u32 = 400;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn drawing() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: W,
        height: H,
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

fn center(a: &Annotator, button: Button) -> Point {
    a.button_center(button).expect("toolbar is visible")
}

fn click(a: &mut Annotator, at: Point) {
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerUp(at));
}

fn press(a: &mut Annotator, button: Button) {
    let at = center(a, button);
    click(a, at);
}

fn stroke(a: &mut Annotator, y: f32) {
    a.handle(Event::PointerDown(p(300.0, y)));
    a.handle(Event::PointerMove(p(350.0, y)));
    a.handle(Event::PointerUp(p(400.0, y)));
}

/// Tool keys that reach each Tool, and a Select with nothing selected.
const TOOLS: [(char, ToolKind); 4] = [
    ('p', ToolKind::Pen),
    ('e', ToolKind::Eraser),
    ('k', ToolKind::Laser),
    ('v', ToolKind::Select),
];

#[test]
fn a_click_on_a_colour_or_width_does_what_its_key_does_for_every_tool() {
    let choices = [
        (Button::Color(3), '4'),
        (Button::Color(0), '1'),
        (Button::Width(0), '['),
        (Button::Width(2), ']'),
    ];
    for (tool_key, kind) in TOOLS {
        for (button, choice_key) in choices {
            let mut by_click = drawing();
            let mut by_key = drawing();
            for a in [&mut by_click, &mut by_key] {
                key(a, 'v'); // Draw Mode starts on the Pen, whose key toggles.
                key(a, tool_key);
                assert_eq!(a.view().tool, kind);
            }
            press(&mut by_click, button);
            key(&mut by_key, choice_key);
            assert_eq!(by_click.view().tool, by_key.view().tool, "{kind:?}");
            assert_eq!(by_click.view().style, by_key.view().style, "{kind:?}");
            // Eraser, Laser and an empty Select hand over to the Pen.
            assert_eq!(by_click.view().tool, ToolKind::Pen, "{kind:?}");
            for a in [&mut by_click, &mut by_key] {
                stroke(a, 100.0);
            }
            assert_eq!(by_click.ink(), by_key.ink(), "{kind:?} {button:?}");
        }
    }
}

#[test]
fn a_click_restyles_the_selection_like_the_key_and_stays_on_select() {
    let mut by_click = drawing();
    let mut by_key = drawing();
    for a in [&mut by_click, &mut by_key] {
        stroke(a, 100.0);
        key(a, 'v');
        click(a, p(350.0, 100.0));
        assert_eq!(a.selection().len(), 1);
    }
    press(&mut by_click, Button::Color(3));
    key(&mut by_key, '4');
    press(&mut by_click, Button::Width(2));
    key(&mut by_key, ']');
    assert_eq!(by_click.ink(), by_key.ink());
    assert_eq!(by_click.view().tool, ToolKind::Select);
    // The Pen's own Style was not touched.
    assert_eq!(by_click.view().style, markuli_core::Style::default());
    assert_eq!(
        by_click.ink().elements()[0].stroke_color(),
        [0x19, 0x71, 0xc2]
    );
}

#[test]
fn a_press_on_any_toolbar_button_or_gap_never_starts_a_stroke() {
    let mut a = drawing();
    let buttons = [
        Button::Tool(0),
        Button::Color(0),
        Button::Color(4),
        Button::Width(0),
        Button::Width(2),
        Button::Undo,
        Button::Clear,
    ];
    for b in buttons {
        let at = center(&a, b);
        a.handle(Event::PointerDown(at));
        a.handle(Event::PointerMove(p(at.x + 3.0, at.y + 100.0)));
        a.handle(Event::PointerUp(p(at.x + 3.0, at.y + 100.0)));
    }
    // The gap between two buttons of one group is padding: dead, not canvas.
    let (c0, c1) = (center(&a, Button::Color(0)), center(&a, Button::Color(1)));
    click(&mut a, p(f32::midpoint(c0.x, c1.x), c0.y));
    assert!(a.ink().is_empty());
}

fn paint(a: &mut Annotator, pm: &mut Pixmap) -> Option<markuli_core::Damage> {
    a.render(&mut pm.as_mut(), Format::Rgba)
}

#[test]
fn moving_within_one_button_causes_no_damage() {
    let mut a = drawing();
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    for button in [Button::Color(2), Button::Width(1)] {
        let c = center(&a, button);
        a.handle(Event::PointerMove(c));
        assert!(paint(&mut a, &mut pm).is_some(), "entering repaints");
        for dx in [1.0, 2.0, -1.0, 0.5] {
            let view = a.handle(Event::PointerMove(p(c.x + dx, c.y + 1.0)));
            assert!(!view.needs_render, "{button:?}: nothing to render");
            assert_eq!(paint(&mut a, &mut pm), None);
        }
    }
    // Another button does change what is shown.
    let view = a.handle(Event::PointerMove(center(&a, Button::Color(3))));
    assert!(view.needs_render);
}

#[test]
fn the_cursor_is_an_arrow_over_colour_and_width_buttons() {
    let mut a = drawing();
    for b in [Button::Color(1), Button::Width(2)] {
        a.handle(Event::PointerMove(p(10.0, 300.0)));
        assert!(matches!(a.view().cursor, Cursor::Pen { .. }));
        a.handle(Event::PointerMove(center(&a, b)));
        assert_eq!(a.view().cursor, Cursor::Arrow, "{b:?}");
    }
}

fn row() -> impl Iterator<Item = Button> {
    (0..4)
        .map(Button::Tool)
        .chain((0..5).map(Button::Color))
        .chain((0..3).map(Button::Width))
        .chain([Button::Undo, Button::Redo, Button::Clear])
}

#[test]
fn the_row_is_tools_colours_widths_then_undo_redo_clear() {
    let a = drawing();
    let order: Vec<Point> = row().map(|b| center(&a, b)).collect();
    assert_eq!(order.len(), 15);
    for pair in order.windows(2) {
        assert!(pair[0].x < pair[1].x, "left to right");
        assert!((pair[0].y - pair[1].y).abs() < f32::EPSILON, "one row");
    }
    assert_eq!(a.button_center(Button::Color(5)), None);
    assert_eq!(a.button_center(Button::Width(3)), None);
}

#[test]
fn the_buttons_never_move_when_the_tool_or_selection_changes() {
    let mut a = drawing();
    let before: Vec<Point> = row().map(|b| center(&a, b)).collect();
    stroke(&mut a, 100.0);
    for (k, _) in TOOLS {
        key(&mut a, k);
        click(&mut a, p(350.0, 100.0));
        let now: Vec<Point> = row().map(|b| center(&a, b)).collect();
        assert_eq!(now, before, "after tool key {k}");
    }
}
