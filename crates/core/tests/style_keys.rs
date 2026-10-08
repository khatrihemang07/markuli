//! Seam 1: colour and width keys (1-5, `[`, `]`) through the annotator interface.

use markuli_core::{Annotator, Button, DisplayId, Element, Event, Key, Point, ToolKind};

const PALETTE: [[u8; 3]; 5] = [
    [0x1e, 0x1e, 0x1e],
    [0xe0, 0x31, 0x31],
    [0x2f, 0x9e, 0x44],
    [0x19, 0x71, 0xc2],
    [0xf0, 0x8c, 0x00],
];
const RED: [u8; 3] = PALETTE[1];

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

fn press(a: &mut Annotator, c: char, command: bool, alt: bool) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command,
        shift: false,
        alt,
    });
}

fn key(a: &mut Annotator, c: char) {
    press(a, c, false, false);
}

fn stroke(a: &mut Annotator, y: f32) {
    a.handle(Event::PointerDown(p(300.0, y)));
    a.handle(Event::PointerMove(p(350.0, y)));
    a.handle(Event::PointerUp(p(400.0, y)));
}

fn last(a: &Annotator) -> &Element {
    a.ink().elements().last().expect("a stroke")
}

fn tool(a: &Annotator) -> ToolKind {
    a.view().tool
}

fn widths(a: &Annotator) -> Vec<f32> {
    a.ink()
        .elements()
        .iter()
        .map(Element::stroke_width)
        .collect()
}

#[test]
fn each_digit_sets_the_colour_of_the_next_stroke() {
    let mut a = session();
    for (i, color) in PALETTE.iter().enumerate() {
        key(&mut a, char::from(b'1' + u8::try_from(i).expect("small")));
        stroke(&mut a, 200.0);
        assert_eq!(last(&a).stroke_color(), *color, "digit {}", i + 1);
    }
}

#[test]
fn six_eight_and_nine_do_nothing() {
    let mut a = session();
    key(&mut a, '3');
    for c in ['6', '8', '9'] {
        key(&mut a, c);
        assert_eq!(tool(&a), ToolKind::Pen, "{c}");
    }
    stroke(&mut a, 200.0);
    assert_eq!(last(&a).stroke_color(), PALETTE[2]);
    key(&mut a, 'v');
    for c in ['6', '8', '9'] {
        key(&mut a, c);
        assert_eq!(tool(&a), ToolKind::Select, "{c}");
    }
}

#[test]
fn brackets_step_the_width_and_stop_at_the_ends() {
    let mut a = session();
    let mut seen = Vec::new();
    for c in ['[', '[', ']', ']', ']', ']'] {
        key(&mut a, c);
        stroke(&mut a, 200.0);
        seen.push(last(&a).stroke_width());
    }
    assert_eq!(seen, [1.0, 1.0, 2.0, 4.0, 4.0, 4.0]);
}

#[test]
fn a_colour_key_from_another_tool_switches_to_the_pen() {
    for from in ['e', 'k', 'v'] {
        let mut a = session();
        key(&mut a, from);
        assert_ne!(tool(&a), ToolKind::Pen, "{from}");
        key(&mut a, '4');
        assert_eq!(tool(&a), ToolKind::Pen, "{from}");
        stroke(&mut a, 200.0);
        assert_eq!(last(&a).stroke_color(), PALETTE[3], "{from}");
    }
}

#[test]
fn a_width_key_from_another_tool_switches_to_the_pen_and_steps() {
    for from in ['e', 'k', 'v'] {
        let mut a = session();
        key(&mut a, from);
        key(&mut a, ']');
        assert_eq!(tool(&a), ToolKind::Pen, "{from}");
        stroke(&mut a, 200.0);
        assert_eq!(last(&a).stroke_width(), 4.0, "{from}");
    }
}

#[test]
fn a_selection_is_restyled_as_one_undo_step_and_the_style_stays() {
    let mut a = session();
    stroke(&mut a, 200.0);
    stroke(&mut a, 300.0);
    press(&mut a, 'a', true, false);
    assert_eq!(tool(&a), ToolKind::Select);
    key(&mut a, '3');
    assert_eq!(tool(&a), ToolKind::Select);
    assert!(a
        .ink()
        .elements()
        .iter()
        .all(|e| e.stroke_color() == PALETTE[2]));
    // The Style did not change: the next stroke is still red.
    key(&mut a, 'p');
    stroke(&mut a, 400.0);
    assert_eq!(last(&a).stroke_color(), RED);
    // One undo removes that stroke; one more restores both restyled ones.
    press(&mut a, 'z', true, false);
    press(&mut a, 'z', true, false);
    assert_eq!(a.ink().len(), 2);
    assert!(a.ink().elements().iter().all(|e| e.stroke_color() == RED));
}

#[test]
fn a_selection_steps_each_elements_own_width() {
    let mut a = session();
    key(&mut a, '[');
    stroke(&mut a, 200.0); // thin
    key(&mut a, ']');
    key(&mut a, ']');
    stroke(&mut a, 300.0); // bold
    press(&mut a, 'a', true, false);
    key(&mut a, ']');
    assert_eq!(widths(&a), [2.0, 4.0]);
    key(&mut a, '[');
    assert_eq!(widths(&a), [1.0, 2.0]);
    key(&mut a, '[');
    assert_eq!(widths(&a), [1.0, 1.0]);
    // Each press is its own undo step.
    press(&mut a, 'z', true, false);
    assert_eq!(widths(&a), [1.0, 2.0]);
}

#[test]
fn an_empty_selection_switches_to_the_pen() {
    let mut a = session();
    stroke(&mut a, 200.0);
    key(&mut a, 'v');
    key(&mut a, '5');
    assert_eq!(tool(&a), ToolKind::Pen);
    assert_eq!(last(&a).stroke_color(), RED, "existing ink is untouched");
    stroke(&mut a, 300.0);
    assert_eq!(last(&a).stroke_color(), PALETTE[4]);
}

#[test]
fn digits_no_longer_choose_tools_but_seven_and_zero_still_toggle() {
    let mut a = session();
    key(&mut a, 'k');
    key(&mut a, '4');
    assert_eq!(
        tool(&a),
        ToolKind::Pen,
        "4 is a colour, it does not pick the Laser"
    );
    key(&mut a, '7');
    assert_eq!(tool(&a), ToolKind::Eraser);
    key(&mut a, '0');
    assert_eq!(tool(&a), ToolKind::Pen);
    for (c, kind) in [
        ('v', ToolKind::Select),
        ('p', ToolKind::Pen),
        ('e', ToolKind::Eraser),
        ('k', ToolKind::Laser),
    ] {
        key(&mut a, c);
        assert_eq!(tool(&a), kind, "{c}");
    }
}

#[test]
fn keys_are_ignored_mid_stroke() {
    let mut a = session();
    a.handle(Event::PointerDown(p(300.0, 200.0)));
    a.handle(Event::PointerMove(p(350.0, 200.0)));
    key(&mut a, '3');
    key(&mut a, ']');
    a.handle(Event::PointerUp(p(400.0, 200.0)));
    assert_eq!(last(&a).stroke_color(), RED);
    assert_eq!(last(&a).stroke_width(), 2.0);
    stroke(&mut a, 300.0);
    assert_eq!(last(&a).stroke_color(), RED);
    assert_eq!(last(&a).stroke_width(), 2.0);
}

#[test]
fn keys_are_ignored_while_the_toolbar_is_pressed() {
    let mut a = session();
    key(&mut a, 'e');
    let at = a.button_center(Button::Undo).expect("toolbar");
    a.handle(Event::PointerDown(at));
    key(&mut a, '3');
    key(&mut a, ']');
    a.handle(Event::PointerUp(at));
    assert_eq!(tool(&a), ToolKind::Eraser);
    key(&mut a, 'p');
    stroke(&mut a, 200.0);
    assert_eq!(last(&a).stroke_color(), RED);
    assert_eq!(last(&a).stroke_width(), 2.0);
}

#[test]
fn cmd_blocks_the_style_keys() {
    let mut a = session();
    key(&mut a, 'e');
    for c in ['3', '[', ']'] {
        press(&mut a, c, true, false);
    }
    assert_eq!(tool(&a), ToolKind::Eraser);
    key(&mut a, 'p');
    stroke(&mut a, 200.0);
    assert_eq!(last(&a).stroke_color(), RED);
    assert_eq!(last(&a).stroke_width(), 2.0);
}

#[test]
fn alt_blocks_digits_but_not_brackets() {
    let mut a = session();
    key(&mut a, 'e');
    press(&mut a, '3', false, true);
    assert_eq!(tool(&a), ToolKind::Eraser);
    press(&mut a, ']', false, true);
    assert_eq!(tool(&a), ToolKind::Pen);
    stroke(&mut a, 200.0);
    assert_eq!(last(&a).stroke_color(), RED);
    assert_eq!(last(&a).stroke_width(), 4.0);
}

#[test]
fn shift_plus_a_digit_is_ignored() {
    // On AZERTY the digits are shifted; they must not pick a color.
    let mut a = session();
    key(&mut a, 'e');
    a.handle(Event::Key {
        key: Key::Char('4'),
        command: false,
        shift: true,
        alt: false,
    });
    assert_eq!(tool(&a), ToolKind::Eraser, "no hand-over to the Pen");
    key(&mut a, 'p');
    stroke(&mut a, 200.0);
    assert_eq!(last(&a).stroke_color(), RED, "the Style is unchanged");
}
