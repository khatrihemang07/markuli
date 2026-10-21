//! Seam 1: the Color Picker through the annotator interface only. Events in;
//! the View, the Ink and the picker's locator out.

mod support;

use markuli_core::{
    Annotator, Button, Event, Palette, PickerPart, SlotKind, SlotValue, Style, ToolKind,
    ToolbarPosition,
};
use support::picker::{
    click, click_part, colors, command, drag, escape, is_open, key, open, p, part, session, sized,
    stroke,
};

/// Preset 0 is the first cell in reading order: gray, lightest tone.
const GRAY_2: [u8; 3] = [0xe9, 0xec, 0xef];
/// Neutral 8 is black.
const BLACK: [u8; 3] = [0, 0, 0];

fn slot_color(a: &Annotator, slot: usize) -> [u8; 3] {
    a.view().palette.colors[slot]
}

#[test]
fn a_secondary_click_on_a_color_slot_opens_the_picker() {
    let mut a = session();
    assert!(!is_open(&a));
    open(&mut a, Button::Color(2));
    assert!(is_open(&a));
    // It asks the platform for nothing: only "..." does.
    assert!(a.view().edit.is_none());
}

#[test]
fn a_secondary_click_elsewhere_closes_it_or_does_nothing() {
    let mut a = session();
    a.handle(Event::SecondaryClick(p(400.0, 400.0)));
    assert!(!is_open(&a));
    open(&mut a, Button::Color(2));
    a.handle(Event::SecondaryClick(p(700.0, 500.0)));
    assert!(!is_open(&a));
    assert!(a.ink().is_empty());
}

#[test]
fn a_secondary_click_on_another_slot_moves_the_picker() {
    let mut a = session();
    open(&mut a, Button::Color(0));
    let before = part(&a, PickerPart::Preset(0));
    open(&mut a, Button::Color(4));
    assert!(is_open(&a));
    assert!(part(&a, PickerPart::Preset(0)).x > before.x);
}

#[test]
fn a_first_click_only_chooses_and_a_click_on_the_chosen_slot_opens() {
    let mut a = session();
    let at = support::picker::center(&a, Button::Color(3));
    click(&mut a, at);
    assert_eq!(a.view().style.color, 3);
    assert!(!is_open(&a));
    click(&mut a, at);
    assert!(is_open(&a));
    // The same click again is outside the popover: it closes (a natural toggle).
    click(&mut a, at);
    assert!(!is_open(&a));
}

#[test]
fn a_click_on_the_chosen_slot_of_a_dimmed_tool_only_switches_to_the_pen() {
    let mut a = session();
    key(&mut a, 'e');
    let at = support::picker::center(&a, Button::Color(0));
    click(&mut a, at);
    assert_eq!(a.view().tool, ToolKind::Pen);
    assert!(!is_open(&a));
}

#[test]
fn a_preset_sets_the_slot_chooses_it_closes_and_draws_with_it() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Preset(0));
    assert_eq!(slot_color(&a, 2), GRAY_2);
    assert_eq!(a.view().style, Style { color: 2, width: 1 });
    assert!(!is_open(&a));
    assert!(a.ink().is_empty(), "the pick never draws");
    stroke(&mut a, 100.0);
    assert_eq!(colors(&a), vec![GRAY_2]);
}

#[test]
fn the_grid_holds_the_new_defaults_and_the_neutrals() {
    // Red tone 8, then the defaults swapped in: yellow 6, green 8, blue 6.
    let expect = [
        (28, [0xe0, 0x31, 0x31]), // red 8
        (21, [0xf5, 0x9e, 0x0b]), // yellow 6
        (31, [0x16, 0xa3, 0x4a]), // green 8
        (24, [0x00, 0x7a, 0xff]), // blue 6
        (25, [0xbe, 0x4b, 0xdb]), // grape 6
    ];
    let mut a = session();
    for (cell, rgb) in expect {
        open(&mut a, Button::Color(0));
        click_part(&mut a, PickerPart::Preset(cell));
        assert_eq!(slot_color(&a, 0), rgb, "preset {cell}");
    }
    open(&mut a, Button::Color(0));
    click_part(&mut a, PickerPart::Neutral(8));
    assert_eq!(slot_color(&a, 0), BLACK);
}

#[test]
fn every_default_color_is_a_cell_of_the_grid() {
    let defaults = Palette::DEFAULT.colors;
    let mut a = session();
    let mut found = [false; 5];
    for cell in 0..36 {
        open(&mut a, Button::Color(0));
        click_part(&mut a, PickerPart::Preset(cell));
        let picked = slot_color(&a, 0);
        for (i, d) in defaults.iter().enumerate() {
            found[i] |= *d == picked;
        }
    }
    assert_eq!(found, [true; 5], "{defaults:?}");
}

#[test]
fn a_pick_restyles_the_selection_as_one_undo_step() {
    let mut a = session();
    stroke(&mut a, 200.0);
    stroke(&mut a, 260.0);
    command(&mut a, 'a');
    let before = colors(&a);
    open(&mut a, Button::Color(1));
    click_part(&mut a, PickerPart::Preset(0));
    assert_eq!(colors(&a), vec![GRAY_2; 2]);
    command(&mut a, 'z');
    assert_eq!(colors(&a), before);
}

#[test]
fn clicking_a_tab_keeps_the_picker_open() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    assert!(is_open(&a));
    assert!(a.picker_point(PickerPart::Square(0.5, 0.5)).is_some());
    assert!(a.picker_point(PickerPart::Preset(0)).is_none());
    click_part(&mut a, PickerPart::Tab(0));
    assert!(a.picker_point(PickerPart::Preset(0)).is_some());
    assert!(a.ink().is_empty());
}

#[test]
fn a_custom_drag_is_live_and_its_release_closes() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    let down = part(&a, PickerPart::Square(0.2, 0.2));
    a.handle(Event::PointerDown(down));
    let first = slot_color(&a, 2);
    assert_ne!(first, Palette::DEFAULT.colors[2], "a press already picks");
    a.handle(Event::PointerMove(part(&a, PickerPart::Square(0.9, 0.3))));
    let second = slot_color(&a, 2);
    assert_ne!(second, first, "a move updates the slot at once");
    assert!(is_open(&a), "still open while dragging");
    assert_eq!(a.view().style.color, 2);
    a.handle(Event::PointerUp(part(&a, PickerPart::Square(0.9, 0.3))));
    assert_eq!(slot_color(&a, 2), second);
    assert!(!is_open(&a), "the release closes");
    assert!(a.ink().is_empty());
}

#[test]
fn the_square_maps_saturation_right_and_value_up() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    drag(&mut a, &[PickerPart::Square(0.0, 0.0)]);
    assert_eq!(slot_color(&a, 2), [255, 255, 255]);
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    drag(&mut a, &[PickerPart::Square(0.5, 1.0)]);
    assert_eq!(slot_color(&a, 2), [0, 0, 0]);
}

#[test]
fn the_hue_strip_changes_the_hue_and_keeps_saturation_and_value() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    drag(&mut a, &[PickerPart::Square(1.0, 0.0)]);
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    drag(&mut a, &[PickerPart::Hue(0.0)]);
    assert_eq!(slot_color(&a, 2), [255, 0, 0]);
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    drag(&mut a, &[PickerPart::Hue(0.5)]);
    assert_eq!(slot_color(&a, 2), [0, 255, 255]);
}

#[test]
fn the_hue_survives_a_gray() {
    let mut a = session();
    // Slot 2 is green (hue about 142). Drag to a gray, then to full color.
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    drag(&mut a, &[PickerPart::Square(0.0, 0.5)]);
    let gray = slot_color(&a, 2);
    assert_eq!(gray[0], gray[1]);
    assert_eq!(gray[1], gray[2]);
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    drag(&mut a, &[PickerPart::Square(1.0, 0.0)]);
    let [r, g, b] = slot_color(&a, 2);
    assert_eq!((r, g), (0, 255), "still the green hue, not red");
    assert!((90..=97).contains(&b), "{b}");
}

#[test]
fn a_drag_outside_the_square_clamps_to_its_edge() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    let inside = part(&a, PickerPart::Square(0.5, 0.5));
    a.handle(Event::PointerDown(inside));
    a.handle(Event::PointerMove(p(-500.0, -500.0)));
    assert_eq!(slot_color(&a, 2), [255, 255, 255]);
    a.handle(Event::PointerUp(p(-500.0, -500.0)));
    assert!(!is_open(&a));
}

#[test]
fn reset_restores_the_slot_default_and_closes_without_a_recent_color() {
    let mut a = session();
    open(&mut a, Button::Color(3));
    click_part(&mut a, PickerPart::Preset(0));
    let recent = a.view().recent;
    open(&mut a, Button::Color(3));
    click_part(&mut a, PickerPart::Reset);
    assert_eq!(slot_color(&a, 3), Palette::DEFAULT.colors[3]);
    assert_eq!(a.view().style.color, 3);
    assert!(!is_open(&a));
    assert_eq!(a.view().recent, recent);
}

#[test]
fn more_asks_for_the_system_panel_and_closes() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    let at = part(&a, PickerPart::More);
    // Like the other buttons it acts on the press.
    let request = a.handle(Event::PointerDown(at)).edit;
    a.handle(Event::PointerUp(at));
    assert!(!is_open(&a));
    let request = request.expect("a system panel request");
    assert_eq!(request.kind, SlotKind::Color);
    assert_eq!(request.index, 2);
    assert_eq!(request.value, SlotValue::Color(Palette::DEFAULT.colors[2]));
    // The request lasts until the next event; nothing was drawn.
    assert!(a.handle(Event::PointerMove(at)).edit.is_none());
    assert!(a.ink().is_empty());
}

#[test]
fn a_lost_release_does_not_swallow_the_next_stroke() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    // The press on "..." opened the system panel, which took the release.
    a.handle(Event::PointerDown(part(&a, PickerPart::More)));
    stroke(&mut a, 300.0);
    assert_eq!(a.ink().elements().len(), 1);
}

#[test]
fn escape_closes_the_picker_and_stays_in_draw_mode() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    escape(&mut a);
    assert!(!is_open(&a));
    assert!(a.view().draw_mode);
    escape(&mut a);
    assert!(!a.view().draw_mode, "with nothing to close, Esc leaves");
}

#[test]
fn a_click_outside_closes_and_never_draws() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    a.handle(Event::PointerDown(p(300.0, 500.0)));
    assert!(!is_open(&a));
    a.handle(Event::PointerMove(p(350.0, 520.0)));
    a.handle(Event::PointerUp(p(400.0, 540.0)));
    assert!(a.ink().is_empty());
    stroke(&mut a, 300.0);
    assert_eq!(a.ink().elements().len(), 1);
}

#[test]
fn a_dismissing_click_on_a_tool_button_does_not_switch_tools() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    let at = support::picker::center(&a, Button::Tool(1));
    click(&mut a, at);
    assert!(!is_open(&a));
    assert_eq!(a.view().tool, ToolKind::Pen);
}

#[test]
fn a_press_inside_on_nothing_keeps_it_open_and_never_draws() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    let inside = part(&a, PickerPart::Tab(0));
    // Just left of the segmented control is padding of the popover.
    click(&mut a, p(inside.x - 30.0, inside.y));
    assert!(is_open(&a));
    assert!(a.ink().is_empty());
}

#[test]
fn tool_keys_keep_working_while_it_is_open() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    key(&mut a, 'e');
    assert_eq!(a.view().tool, ToolKind::Eraser);
    assert!(is_open(&a));
    key(&mut a, '4');
    assert_eq!(a.view().style.color, 3);
    assert!(is_open(&a));
}

#[test]
fn scrolling_during_a_drag_is_ignored() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    a.handle(Event::PointerDown(part(&a, PickerPart::Square(0.5, 0.5))));
    a.handle(Event::Scroll { dy: 50.0 });
    assert!(a.view().scroll.abs() < f32::EPSILON);
    a.handle(Event::PointerUp(part(&a, PickerPart::Square(0.5, 0.5))));
    a.handle(Event::Scroll { dy: 50.0 });
    assert!(a.view().scroll > 0.0, "a closed picker does not block it");
}

#[test]
fn leaving_draw_mode_closes_the_picker() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    let display = a.view().display.expect("a display");
    a.handle(Event::ToggleDrawMode(display));
    a.handle(Event::ToggleDrawMode(display));
    assert!(!is_open(&a));
    // Nor does a stale dismissing click swallow the first Stroke.
    stroke(&mut a, 200.0);
    assert_eq!(a.ink().elements().len(), 1);
}

#[test]
fn escape_in_a_custom_drag_keeps_the_color_and_a_second_undo_restores_it() {
    let mut a = session();
    stroke(&mut a, 200.0);
    command(&mut a, 'a');
    let before = colors(&a);
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    a.handle(Event::PointerDown(part(&a, PickerPart::Square(0.3, 0.3))));
    a.handle(Event::PointerMove(part(&a, PickerPart::Square(0.6, 0.4))));
    let dragged = slot_color(&a, 2);
    escape(&mut a);
    assert!(!is_open(&a));
    assert_eq!(slot_color(&a, 2), dragged);
    assert_eq!(colors(&a), vec![dragged]);
    // The release that follows is swallowed.
    a.handle(Event::PointerUp(p(100.0, 400.0)));
    assert_eq!(a.ink().elements().len(), 1);
    command(&mut a, 'z');
    assert_eq!(colors(&a), before);
}

fn bounds_ok(a: &Annotator, width: f32, height: f32) {
    let corners = [
        PickerPart::Tab(0),
        PickerPart::Reset,
        PickerPart::Neutral(0),
        PickerPart::Neutral(8),
        PickerPart::Preset(0),
        PickerPart::Preset(35),
        PickerPart::Recent(0),
    ];
    for c in corners {
        let at = part(a, c);
        assert!(
            at.x > 0.0 && at.y > 0.0 && at.x < width && at.y < height,
            "{c:?} at {at:?} is off the {width}x{height} Overlay"
        );
    }
}

#[test]
fn the_picker_opens_on_the_side_facing_the_interior() {
    for (position, button) in [
        (ToolbarPosition::Top, Button::Color(2)),
        (ToolbarPosition::Bottom, Button::Color(2)),
        (ToolbarPosition::Left, Button::Color(2)),
        (ToolbarPosition::Right, Button::Color(2)),
    ] {
        let mut a = sized(1200, 800, position);
        let slot = support::picker::center(&a, button);
        open(&mut a, button);
        let first = part(&a, PickerPart::Preset(0));
        match position {
            ToolbarPosition::Top => assert!(first.y > slot.y),
            ToolbarPosition::Bottom => assert!(first.y < slot.y),
            ToolbarPosition::Left => assert!(first.x > slot.x),
            ToolbarPosition::Right => assert!(first.x < slot.x),
        }
        bounds_ok(&a, 1200.0, 800.0);
    }
}

#[test]
fn the_picker_is_clamped_inside_the_overlay_at_every_slot_and_size() {
    for (w, h) in [(800, 600), (420, 700), (300, 400)] {
        for position in ToolbarPosition::ALL {
            for slot in 0..5 {
                let mut a = sized(w, h, position);
                open(&mut a, Button::Color(slot));
                #[allow(clippy::cast_precision_loss, reason = "tiny test sizes")]
                bounds_ok(&a, w as f32, h as f32);
            }
        }
    }
}

#[test]
fn the_cursor_is_the_arrow_over_the_picker() {
    let mut a = session();
    open(&mut a, Button::Color(2));
    let at = part(&a, PickerPart::Preset(5));
    let view = a.handle(Event::PointerMove(at));
    assert_eq!(view.cursor, markuli_core::Cursor::Arrow);
}
