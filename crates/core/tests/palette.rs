//! Seam 1: the editable Palette through the annotator interface only.

use markuli_core::{
    Annotator, Button, DisplayId, EditRequest, Element, Event, Key, Palette, Point, SlotKind,
    SlotValue, Style, ToolKind,
};

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

fn command(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: true,
        shift: false,
        alt: false,
    });
}

fn stroke(a: &mut Annotator, y: f32) {
    a.handle(Event::PointerDown(p(300.0, y)));
    a.handle(Event::PointerMove(p(350.0, y)));
    a.handle(Event::PointerUp(p(400.0, y)));
}

fn center(a: &Annotator, b: Button) -> Point {
    a.button_center(b).expect("the toolbar is shown")
}

fn last(a: &Annotator) -> &Element {
    a.ink().elements().last().expect("a stroke")
}

fn colors(a: &Annotator) -> Vec<[u8; 3]> {
    a.ink()
        .elements()
        .iter()
        .map(Element::stroke_color)
        .collect()
}

const TEAL: [u8; 3] = [0x12, 0xb8, 0xa6];
const PINK: [u8; 3] = [0xff, 0x66, 0xcc];
const RED: [u8; 3] = [0xe0, 0x31, 0x31];
const BLUE: [u8; 3] = [0x19, 0x71, 0xc2];
const GREEN: [u8; 3] = [0x2f, 0x9e, 0x44];

fn edit_color(a: &mut Annotator, slot: usize, rgb: [u8; 3]) {
    a.handle(Event::EditColor { slot, rgb });
}

fn edit_width(a: &mut Annotator, slot: usize, width: f32) {
    a.handle(Event::EditWidth { slot, width });
}

#[test]
fn the_view_reports_the_default_palette() {
    let palette = session().view().palette;
    assert_eq!(palette, Palette::default());
    assert_eq!(palette.colors[1], RED);
    assert_eq!(palette.widths, [1.0, 2.0, 4.0]);
}

#[test]
fn a_startup_palette_is_reported_and_drawn_with() {
    let mut a = Annotator::new();
    let mut palette = Palette::default();
    palette.colors[1] = TEAL;
    palette.widths[1] = 7.5;
    a.handle(Event::Palette(palette));
    a.handle(Event::Resize {
        width: 800,
        height: 600,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    assert_eq!(a.view().palette, palette);
    // The default Style (slots 1 and 1) resolves to the edited values.
    stroke(&mut a, 100.0);
    assert_eq!(last(&a).stroke_color(), TEAL);
    assert!((last(&a).stroke_width() - 7.5).abs() < f32::EPSILON);
}

#[test]
fn the_palette_event_sanitizes_widths() {
    let mut a = session();
    let palette = Palette {
        widths: [0.1, 99.0, f32::NAN],
        ..Palette::default()
    };
    a.handle(Event::Palette(palette));
    assert_eq!(a.view().palette.widths, [0.5, 20.0, 4.0]);
}

#[test]
fn an_edited_color_is_drawn_by_the_next_stroke_and_by_key_n() {
    let mut a = session();
    edit_color(&mut a, 2, TEAL);
    assert_eq!(a.view().style, Style { color: 2, width: 1 });
    assert_eq!(a.view().palette.colors[2], TEAL);
    stroke(&mut a, 100.0);
    assert_eq!(last(&a).stroke_color(), TEAL);
    key(&mut a, '4');
    stroke(&mut a, 200.0);
    assert_eq!(last(&a).stroke_color(), BLUE);
    key(&mut a, '3');
    stroke(&mut a, 300.0);
    assert_eq!(last(&a).stroke_color(), TEAL);
}

#[test]
fn an_edited_color_equal_to_another_slot_still_chooses_its_own_slot() {
    let mut a = session();
    edit_color(&mut a, 4, GREEN);
    assert_eq!(a.view().style.color, 4);
}

#[test]
fn editing_chooses_the_slot_for_every_tool_and_hands_over_to_the_pen() {
    for (tool, kind) in [
        (None, ToolKind::Pen),
        (Some('e'), ToolKind::Eraser),
        (Some('k'), ToolKind::Laser),
        (Some('v'), ToolKind::Select),
    ] {
        let mut a = session();
        if let Some(tool) = tool {
            key(&mut a, tool);
        }
        assert_eq!(a.view().tool, kind);
        edit_color(&mut a, 3, TEAL);
        let v = a.view();
        assert_eq!(v.tool, ToolKind::Pen, "color edit from {kind:?}");
        assert_eq!(v.style.color, 3);

        let mut a = session();
        if let Some(tool) = tool {
            key(&mut a, tool);
        }
        edit_width(&mut a, 0, 12.0);
        let v = a.view();
        assert_eq!(v.tool, ToolKind::Pen, "width edit from {kind:?}");
        assert_eq!(v.style.width, 0);
        stroke(&mut a, 100.0);
        assert!((last(&a).stroke_width() - 12.0).abs() < f32::EPSILON);
    }
}

#[test]
fn width_edits_are_clamped_and_snapped_to_half_steps() {
    let mut a = session();
    for (given, expected) in [
        (0.1, 0.5),
        (0.74, 0.5),
        (0.76, 1.0),
        (3.3, 3.5),
        (19.9, 20.0),
        (500.0, 20.0),
        (-3.0, 0.5),
    ] {
        edit_width(&mut a, 2, given);
        assert!(
            (a.view().palette.widths[2] - expected).abs() < f32::EPSILON,
            "{given} -> {expected}"
        );
    }
    edit_width(&mut a, 2, 6.0);
    edit_width(&mut a, 2, f32::NAN);
    edit_width(&mut a, 2, f32::INFINITY);
    assert!((a.view().palette.widths[2] - 6.0).abs() < f32::EPSILON);
}

#[test]
fn an_out_of_range_slot_is_ignored() {
    let mut a = session();
    let before = a.view();
    edit_color(&mut a, 5, TEAL);
    edit_width(&mut a, 3, 9.0);
    assert_eq!(a.view().palette, before.palette);
    assert_eq!(a.view().style, before.style);
}

#[test]
fn brackets_follow_slot_order_with_custom_widths() {
    let mut a = session();
    let palette = Palette {
        widths: [8.0, 2.0, 2.0],
        ..Palette::default()
    };
    a.handle(Event::Palette(palette));
    a.handle(Event::Style(Style { color: 1, width: 0 }));
    key(&mut a, ']');
    assert_eq!(a.view().style.width, 1);
    key(&mut a, ']');
    assert_eq!(a.view().style.width, 2);
    key(&mut a, ']');
    assert_eq!(a.view().style.width, 2);
    key(&mut a, '[');
    key(&mut a, '[');
    key(&mut a, '[');
    assert_eq!(a.view().style.width, 0);
    stroke(&mut a, 100.0);
    assert!((last(&a).stroke_width() - 8.0).abs() < f32::EPSILON);
}

#[test]
fn old_strokes_keep_their_look_after_an_edit() {
    let mut a = session();
    stroke(&mut a, 100.0);
    edit_color(&mut a, 1, TEAL);
    edit_width(&mut a, 1, 9.0);
    stroke(&mut a, 200.0);
    let first = &a.ink().elements()[0];
    assert_eq!(first.stroke_color(), RED);
    assert!((first.stroke_width() - 2.0).abs() < f32::EPSILON);
    assert_eq!(last(&a).stroke_color(), TEAL);
}

fn two_selected() -> Annotator {
    let mut a = session();
    stroke(&mut a, 100.0);
    stroke(&mut a, 200.0);
    command(&mut a, 'a');
    a
}

#[test]
fn editing_with_a_selection_restyles_it_in_one_undo_step() {
    let mut a = two_selected();
    assert_eq!(a.view().tool, ToolKind::Select);
    // A color picker dragged around: many edits of one slot, then it closes.
    for rgb in [TEAL, PINK, [1, 2, 3]] {
        edit_color(&mut a, 3, rgb);
    }
    a.handle(Event::EditEnd);
    assert_eq!(colors(&a), vec![[1, 2, 3]; 2]);
    assert_eq!(a.view().tool, ToolKind::Select);
    assert_eq!(
        a.view().style.color,
        1,
        "a Selection restyle leaves the Pen"
    );
    command(&mut a, 'z');
    assert_eq!(colors(&a), vec![RED; 2], "one undo restores the original");
}

#[test]
fn a_width_edit_with_a_selection_restyles_and_undoes_in_one_step() {
    let mut a = two_selected();
    for w in [3.0, 5.0, 6.5] {
        edit_width(&mut a, 0, w);
    }
    a.handle(Event::EditEnd);
    for e in a.ink().elements() {
        assert!((e.stroke_width() - 6.5).abs() < f32::EPSILON);
    }
    command(&mut a, 'z');
    for e in a.ink().elements() {
        assert!((e.stroke_width() - 2.0).abs() < f32::EPSILON);
    }
}

#[test]
fn edits_of_different_slots_are_separate_undo_steps() {
    let mut a = two_selected();
    edit_color(&mut a, 2, TEAL);
    edit_color(&mut a, 3, PINK);
    a.handle(Event::EditEnd);
    assert_eq!(colors(&a), vec![PINK; 2]);
    command(&mut a, 'z');
    assert_eq!(colors(&a), vec![TEAL; 2]);
    command(&mut a, 'z');
    assert_eq!(colors(&a), vec![RED; 2]);
}

#[test]
fn a_key_after_an_edit_closes_the_coalescing() {
    let mut a = two_selected();
    edit_color(&mut a, 2, TEAL);
    key(&mut a, '4');
    edit_color(&mut a, 2, PINK);
    a.handle(Event::EditEnd);
    command(&mut a, 'z');
    assert_eq!(colors(&a), vec![BLUE; 2]);
}

#[test]
fn reset_is_an_edit_with_the_default() {
    let mut a = session();
    edit_color(&mut a, 2, TEAL);
    edit_width(&mut a, 2, 9.0);
    let defaults = Palette::default();
    edit_color(&mut a, 2, defaults.colors[2]);
    edit_width(&mut a, 2, defaults.widths[2]);
    assert_eq!(a.view().palette, defaults);
}

#[test]
fn copied_excalidraw_json_carries_the_custom_values() {
    let mut a = session();
    edit_color(&mut a, 0, TEAL);
    edit_width(&mut a, 0, 7.5);
    stroke(&mut a, 100.0);
    command(&mut a, 'c');
    let json = a.take_copy().expect("a copy");
    assert!(json.contains("#12b8a6"), "{json}");
    assert!(json.contains("\"strokeWidth\":7.5"), "{json}");
}

fn secondary(a: &mut Annotator, at: Point) -> Option<EditRequest> {
    a.handle(Event::SecondaryClick(at)).edit
}

#[test]
fn a_secondary_click_on_a_color_or_width_button_requests_an_edit() {
    let mut a = session();
    edit_color(&mut a, 2, TEAL);
    let at = center(&a, Button::Color(2));
    let request = secondary(&mut a, at).expect("a color request");
    assert_eq!(request.kind, SlotKind::Color);
    assert_eq!(request.index, 2);
    assert_eq!(request.value, SlotValue::Color(TEAL));
    let anchor = request.anchor;
    assert!(anchor.w > 0.0 && anchor.h > 0.0);
    assert!(at.x >= anchor.x && at.x <= anchor.x + anchor.w);
    assert!(at.y >= anchor.y && at.y <= anchor.y + anchor.h);

    edit_width(&mut a, 1, 7.0);
    let at = center(&a, Button::Width(1));
    let request = secondary(&mut a, at).expect("a width request");
    assert_eq!(request.kind, SlotKind::Width);
    assert_eq!(request.index, 1);
    assert_eq!(request.value, SlotValue::Width(7.0));
}

#[test]
fn the_edit_request_lasts_until_the_next_event() {
    let mut a = session();
    let at = center(&a, Button::Color(0));
    assert!(secondary(&mut a, at).is_some());
    assert!(a.handle(Event::PointerMove(at)).edit.is_none());
}

#[test]
fn a_secondary_click_elsewhere_does_nothing() {
    let mut a = session();
    let before = a.view();
    let targets = [
        center(&a, Button::Tool(0)),
        center(&a, Button::Undo),
        center(&a, Button::Redo),
        center(&a, Button::Clear),
        p(400.0, 300.0),
        p(5.0, 5.0),
    ];
    for at in targets {
        assert!(secondary(&mut a, at).is_none());
    }
    assert!(a.ink().is_empty());
    assert_eq!(a.view().tool, before.tool);
    assert_eq!(a.view().style, before.style);
    assert_eq!(a.view().palette, before.palette);
}

#[test]
fn a_secondary_click_never_draws_or_changes_the_tool() {
    let mut a = session();
    key(&mut a, 'e');
    let at = center(&a, Button::Color(3));
    secondary(&mut a, at);
    secondary(&mut a, p(300.0, 300.0));
    assert_eq!(a.view().tool, ToolKind::Eraser);
    assert_eq!(a.view().style, Style::default());
    key(&mut a, 'p');
    assert!(a.ink().is_empty());
    // Nor does one in the middle of a Stroke end or add to it.
    a.handle(Event::PointerDown(p(300.0, 100.0)));
    secondary(&mut a, p(320.0, 100.0));
    a.handle(Event::PointerUp(p(340.0, 100.0)));
    assert_eq!(a.ink().elements().len(), 1);
}

#[test]
fn a_secondary_click_outside_draw_mode_does_nothing() {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: 800,
        height: 600,
    });
    assert!(secondary(&mut a, p(10.0, 10.0)).is_none());
}
