//! Seam 1: the editable Palette through the annotator interface only.

mod support;

use markuli_core::{
    Annotator, Button, DisplayId, Element, Event, Key, Palette, PickerPart, Point, SlotKind,
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
const AMBER: [u8; 3] = [0xf5, 0x9e, 0x0b];
const GREEN: [u8; 3] = [0x16, 0xa3, 0x4a];

fn edit_color(a: &mut Annotator, slot: usize, rgb: [u8; 3]) {
    a.handle(Event::EditColor { slot, rgb });
}

/// Sets a width slot's value (a startup Palette) and chooses the slot. The
/// Width Picker's own edits are in `width_picker.rs`.
fn edit_width(a: &mut Annotator, slot: usize, width: f32) {
    let mut palette = a.view().palette;
    palette.widths[slot] = width;
    a.handle(Event::Palette(palette));
    let color = a.view().style.color;
    a.handle(Event::Style(Style { color, width: slot }));
}

#[test]
fn the_view_reports_the_default_palette() {
    let palette = session().view().palette;
    assert_eq!(palette, Palette::default());
    assert_eq!(palette.colors[0], RED);
    assert_eq!(palette.widths, [2.0, 4.0, 8.0]);
}

#[test]
fn a_startup_palette_is_reported_and_drawn_with() {
    let mut a = Annotator::new();
    let mut palette = Palette::default();
    palette.colors[0] = TEAL;
    palette.widths[1] = 7.5;
    a.handle(Event::Palette(palette));
    a.handle(Event::Resize {
        width: 800,
        height: 600,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    assert_eq!(a.view().palette, palette);
    // The default Style (slots 0 and 1) resolves to the edited values.
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
    assert_eq!(a.view().palette.widths, [0.5, 30.0, 8.0]);
}

#[test]
fn an_edited_color_is_drawn_by_the_next_stroke_and_by_key_n() {
    let mut a = session();
    edit_color(&mut a, 2, TEAL);
    assert_eq!(a.view().style, Style { color: 2, width: 1 });
    assert_eq!(a.view().palette.colors[2], TEAL);
    stroke(&mut a, 100.0);
    assert_eq!(last(&a).stroke_color(), TEAL);
    key(&mut a, '2');
    stroke(&mut a, 200.0);
    assert_eq!(last(&a).stroke_color(), AMBER);
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
        support::picker::open(&mut a, Button::Width(0));
        support::picker::drag(&mut a, &[PickerPart::Track(0.4)]);
        let v = a.view();
        assert_eq!(v.tool, ToolKind::Pen, "width edit from {kind:?}");
        assert_eq!(v.style.width, 0);
        assert!(v.palette.widths[0] > 8.0, "{}", v.palette.widths[0]);
        stroke(&mut a, 100.0);
        assert!((last(&a).stroke_width() - v.palette.widths[0]).abs() < f32::EPSILON);
    }
}

#[test]
fn an_out_of_range_slot_is_ignored() {
    let mut a = session();
    let before = a.view();
    edit_color(&mut a, 5, TEAL);
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
    assert!((first.stroke_width() - 4.0).abs() < f32::EPSILON);
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
        0,
        "a Selection restyle leaves the Pen"
    );
    command(&mut a, 'z');
    assert_eq!(colors(&a), vec![RED; 2], "one undo restores the original");
}

#[test]
fn a_width_edit_with_a_selection_restyles_and_undoes_in_one_step() {
    let mut a = two_selected();
    support::picker::open(&mut a, Button::Width(0));
    support::picker::drag(
        &mut a,
        &[
            PickerPart::Track(0.1),
            PickerPart::Track(0.2),
            PickerPart::Track(0.3),
        ],
    );
    let dragged = a.view().palette.widths[0];
    assert!(dragged > 8.0);
    for e in a.ink().elements() {
        assert!((e.stroke_width() - dragged).abs() < f32::EPSILON);
    }
    command(&mut a, 'z');
    for e in a.ink().elements() {
        assert!((e.stroke_width() - 4.0).abs() < f32::EPSILON);
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
    key(&mut a, '2');
    edit_color(&mut a, 2, PINK);
    a.handle(Event::EditEnd);
    command(&mut a, 'z');
    assert_eq!(colors(&a), vec![AMBER; 2]);
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
    // 7.5 pt in Excalidraw's unit: pt / (4.25 * sqrt(2)).
    let at = json.find("\"strokeWidth\":").expect("strokeWidth") + 14;
    let end = at + json[at..].find(',').expect("comma");
    let sw: f32 = json[at..end].parse().expect("a number");
    assert!((sw - 7.5 / 6.01).abs() < 0.005, "{json}");
}

#[test]
fn the_default_of_every_slot_is_what_reset_restores() {
    let mut a = session();
    for slot in 0..5 {
        edit_color(&mut a, slot, TEAL);
        let Some(SlotValue::Color(rgb)) = Palette::default_slot(SlotKind::Color, slot) else {
            panic!("a default color");
        };
        edit_color(&mut a, slot, rgb);
    }
    for slot in 0..3 {
        edit_width(&mut a, slot, 9.0);
        let Some(SlotValue::Width(width)) = Palette::default_slot(SlotKind::Width, slot) else {
            panic!("a default width");
        };
        edit_width(&mut a, slot, width);
    }
    assert_eq!(a.view().palette, Palette::default());
    assert_eq!(Palette::default_slot(SlotKind::Color, 5), None);
    assert_eq!(Palette::default_slot(SlotKind::Width, 3), None);
}
