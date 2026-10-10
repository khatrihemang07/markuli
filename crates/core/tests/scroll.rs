//! Seam 1: the Scroll, the Hand Tool and the temporary hand (Alt), through
//! the annotator core interface.

use markuli_core::{Annotator, Button, Cursor, DisplayId, Event, Format, Key, Point, ToolKind};
use tiny_skia::Pixmap;

const W: u32 = 700;
const H: u32 = 300;
const D1: DisplayId = DisplayId::new(1);
const D2: DisplayId = DisplayId::new(2);

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// Draw Mode on a `W` x `H` Overlay, so the Toolbar exists.
fn drawing() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::ToggleDrawMode(D1));
    a
}

fn scroll(a: &mut Annotator, dy: f32) {
    a.handle(Event::Scroll { dy });
}

fn key(a: &mut Annotator, key: Key, command: bool, shift: bool) {
    a.handle(Event::Key {
        key,
        command,
        shift,
        alt: false,
    });
}

fn press(a: &mut Annotator, c: char) {
    key(a, Key::Char(c), false, false);
}

fn alt(a: &mut Annotator, held: bool) {
    a.handle(Event::Modifiers {
        shift: false,
        alt: held,
    });
}

/// A horizontal Stroke from `(x0, y)` to `(x1, y)` in screen pixels.
fn stroke(a: &mut Annotator, (x0, x1): (f32, f32), y: f32) {
    a.handle(Event::PointerDown(p(x0, y)));
    a.handle(Event::PointerMove(p(f32::midpoint(x0, x1), y)));
    a.handle(Event::PointerUp(p(x1, y)));
}

fn drag(a: &mut Annotator, from: Point, to: Point) {
    a.handle(Event::PointerDown(from));
    a.handle(Event::PointerMove(to));
    a.handle(Event::PointerUp(to));
}

fn full(a: &mut Annotator) -> Pixmap {
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.handle(Event::SurfaceReset);
    a.render(&mut pm.as_mut(), Format::Rgba);
    pm
}

fn alpha(pm: &Pixmap, x: u32, y: u32) -> u8 {
    pm.data()[((y * W + x) * 4 + 3) as usize]
}

/// Whether a pixel is Ink (opaque), not the near-transparent backdrop.
fn inked(pm: &Pixmap, x: u32, y: u32) -> bool {
    alpha(pm, x, y) == 255
}

/// The pixel rows `y0..y1` of `pm`, for comparing bands.
fn rows(pm: &Pixmap, y0: u32, y1: u32) -> &[u8] {
    &pm.data()[(y0 * W * 4) as usize..(y1 * W * 4) as usize]
}

#[test]
fn a_stroke_drawn_after_scrolling_lands_under_the_pointer() {
    let mut a = drawing();
    scroll(&mut a, 100.0);
    assert_eq!(a.view().scroll, 100.0);
    stroke(&mut a, (50.0, 150.0), 200.0);
    let element = &a.ink().elements()[0];
    assert_eq!((element.x(), element.y()), (50.0, 100.0));
    let pm = full(&mut a);
    assert!(inked(&pm, 100, 200), "ink is under the pointer");
    assert!(!inked(&pm, 100, 100), "not at the Element's own y");
}

#[test]
fn scrolling_moves_the_ink_by_the_delta_and_leaves_the_toolbar_alone() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 150.0);
    let before = full(&mut a);
    assert!(inked(&before, 100, 150));
    scroll(&mut a, 60.0);
    let after = full(&mut a);
    assert!(inked(&after, 100, 210), "moved down by exactly 60");
    assert!(!inked(&after, 100, 150));
    for x in 0..W {
        assert_eq!(inked(&before, x, 150), inked(&after, x, 210), "x {x}");
    }
    assert_eq!(rows(&before, 0, 70), rows(&after, 0, 70), "Toolbar band");
    scroll(&mut a, -60.0);
    assert_eq!(a.view().scroll, 0.0);
}

#[test]
fn scroll_is_ignored_outside_draw_mode_mid_stroke_and_when_not_finite() {
    let mut a = Annotator::new();
    scroll(&mut a, 40.0);
    assert_eq!(a.view().scroll, 0.0, "outside Draw Mode");
    let mut a = drawing();
    for dy in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        scroll(&mut a, dy);
        assert_eq!(a.view().scroll, 0.0);
    }
    a.handle(Event::PointerDown(p(50.0, 200.0)));
    a.handle(Event::PointerMove(p(80.0, 200.0)));
    scroll(&mut a, 40.0);
    assert_eq!(a.view().scroll, 0.0, "mid-Stroke");
    a.handle(Event::PointerUp(p(120.0, 200.0)));
    scroll(&mut a, 40.0);
    assert_eq!(a.view().scroll, 40.0);
}

#[test]
fn scroll_is_ignored_while_a_toolbar_button_is_pressed_but_not_when_hovering_it() {
    let mut a = drawing();
    let undo = a.button_center(Button::Undo).expect("toolbar");
    a.handle(Event::PointerMove(undo));
    scroll(&mut a, 25.0);
    assert_eq!(a.view().scroll, 25.0, "hovering the Toolbar still scrolls");
    a.handle(Event::PointerDown(undo));
    scroll(&mut a, 25.0);
    assert_eq!(a.view().scroll, 25.0, "pressing it does not");
}

#[test]
fn physical_pixels_are_divided_by_the_scale_factor() {
    let mut a = drawing();
    a.handle(Event::ScaleFactor(2.0));
    scroll(&mut a, 50.0);
    assert_eq!(a.view().scroll, 25.0);
}

#[test]
fn scroll_is_clamped_to_a_million_logical_pixels() {
    let mut a = drawing();
    scroll(&mut a, 3.0e9);
    assert_eq!(a.view().scroll, 1_000_000.0);
    scroll(&mut a, -9.0e9);
    assert_eq!(a.view().scroll, -1_000_000.0);
}

#[test]
fn scroll_works_with_any_tool() {
    let mut a = drawing();
    for c in ['v', 'e', 'k', 'p'] {
        press(&mut a, c);
        scroll(&mut a, 1.0);
    }
    assert_eq!(a.view().scroll, 4.0);
}

#[test]
fn scroll_is_kept_when_draw_mode_is_left_and_entered_on_the_same_display() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 150.0);
    scroll(&mut a, 70.0);
    a.handle(Event::ToggleDrawMode(D1));
    a.handle(Event::ToggleDrawMode(D1));
    assert_eq!(a.view().scroll, 70.0);
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn clear_and_a_display_move_reset_the_scroll() {
    for how in 0..4 {
        let mut a = drawing();
        stroke(&mut a, (50.0, 150.0), 150.0);
        scroll(&mut a, 70.0);
        match how {
            0 => press(&mut a, ' '),
            1 => {
                let clear = a.button_center(Button::Clear).expect("toolbar");
                drag(&mut a, clear, clear);
            }
            2 => {
                a.handle(Event::Clear);
            }
            _ => {
                a.handle(Event::ToggleDrawMode(D1));
                a.handle(Event::ToggleDrawMode(D2));
            }
        }
        assert_eq!(a.view().scroll, 0.0, "case {how}");
    }
}

#[test]
fn undoing_a_clear_restores_the_ink_but_leaves_the_scroll_at_zero() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 150.0);
    scroll(&mut a, 70.0);
    a.handle(Event::Clear);
    key(&mut a, Key::Char('z'), true, false);
    assert_eq!(a.ink().len(), 1);
    assert_eq!(a.view().scroll, 0.0);
}

#[test]
fn undo_and_redo_never_change_the_scroll() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 150.0);
    scroll(&mut a, 70.0);
    key(&mut a, Key::Char('z'), true, false);
    assert!(a.ink().is_empty());
    assert_eq!(a.view().scroll, 70.0);
    key(&mut a, Key::Char('z'), true, true);
    assert_eq!(a.ink().len(), 1);
    assert_eq!(a.view().scroll, 70.0);
}

#[test]
fn scrolling_is_not_an_undo_step() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 150.0);
    scroll(&mut a, 70.0);
    key(&mut a, Key::Char('z'), true, false);
    assert!(
        a.ink().is_empty(),
        "Undo removed the Stroke, not the scroll"
    );
}

#[test]
fn select_click_and_box_select_hit_the_right_element_after_scrolling() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 100.0);
    stroke(&mut a, (50.0, 150.0), 200.0);
    let [first, second] = [0, 1].map(|i| a.ink().elements()[i].id());
    scroll(&mut a, 30.0);
    press(&mut a, 'v');
    drag(&mut a, p(100.0, 230.0), p(100.0, 230.0));
    assert_eq!(a.selection(), [second], "click at the shifted position");
    drag(&mut a, p(20.0, 110.0), p(200.0, 160.0));
    assert_eq!(a.selection(), [first], "box around the shifted first");
}

#[test]
fn the_eraser_hits_the_element_under_the_pointer_after_scrolling() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 100.0);
    stroke(&mut a, (50.0, 150.0), 200.0);
    let first = a.ink().elements()[0].id();
    scroll(&mut a, 30.0);
    press(&mut a, 'e');
    drag(&mut a, p(60.0, 230.0), p(140.0, 230.0));
    assert_eq!(a.ink().len(), 1);
    assert_eq!(a.ink().elements()[0].id(), first);
}

#[test]
fn copied_json_is_the_same_before_and_after_scrolling() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 100.0);
    key(&mut a, Key::Char('c'), true, false);
    let before = a.take_copy().expect("copied");
    scroll(&mut a, 55.0);
    key(&mut a, Key::Char('c'), true, false);
    let after = a.take_copy().expect("copied");
    assert_eq!(before, after);
}

#[test]
fn the_laser_trail_stays_on_screen_when_the_scroll_changes() {
    let mut a = drawing();
    a.handle(Event::Clock(0));
    press(&mut a, 'k');
    a.handle(Event::PointerDown(p(50.0, 220.0)));
    a.handle(Event::PointerMove(p(200.0, 230.0)));
    let before = full(&mut a);
    scroll(&mut a, 0.0);
    let same = full(&mut a);
    assert_eq!(rows(&before, 0, H), rows(&same, 0, H));
    // Scroll with the Laser trail fading out after the pointer is released.
    a.handle(Event::PointerUp(p(250.0, 230.0)));
    let held = full(&mut a);
    scroll(&mut a, 80.0);
    let moved = full(&mut a);
    assert_eq!(rows(&held, 0, H), rows(&moved, 0, H));
}

/// Draws incrementally and compares with a full redraw of the same state.
fn same_as_full(a: &mut Annotator, shown: &mut Pixmap, what: &str) {
    a.render(&mut shown.as_mut(), Format::Rgba);
    let fresh = full(a);
    assert_eq!(shown.data(), fresh.data(), "{what}");
}

#[test]
fn incremental_redraws_after_scrolling_equal_a_full_redraw() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 100.0);
    let mut shown = full(&mut a);
    scroll(&mut a, 80.0);
    same_as_full(&mut a, &mut shown, "after the scroll");
    // A Stroke drawn after scrolling, move by move.
    a.handle(Event::PointerDown(p(450.0, 235.0)));
    for i in 1..6_u8 {
        a.handle(Event::PointerMove(p(
            450.0 + f32::from(i) * 15.0,
            235.0 + f32::from(i) * 9.0,
        )));
        same_as_full(&mut a, &mut shown, "drawing");
    }
    a.handle(Event::PointerUp(p(540.0, 285.0)));
    same_as_full(&mut a, &mut shown, "stroke done");
    // Moving a Selection.
    press(&mut a, 'v');
    a.handle(Event::PointerDown(p(100.0, 180.0)));
    for i in 1..6_u8 {
        a.handle(Event::PointerMove(p(
            100.0 + f32::from(i) * 8.0,
            180.0 + f32::from(i) * 5.0,
        )));
        same_as_full(&mut a, &mut shown, "moving");
    }
    a.handle(Event::PointerUp(p(140.0, 205.0)));
    same_as_full(&mut a, &mut shown, "move done");
    // A box select.
    drag(&mut a, p(10.0, 90.0), p(390.0, 290.0));
    same_as_full(&mut a, &mut shown, "box select");
    // Erasing.
    press(&mut a, 'e');
    a.handle(Event::PointerDown(p(160.0, 210.0)));
    a.handle(Event::PointerMove(p(260.0, 215.0)));
    same_as_full(&mut a, &mut shown, "erasing");
    a.handle(Event::PointerUp(p(300.0, 215.0)));
    same_as_full(&mut a, &mut shown, "erased");
}

#[test]
fn h_selects_the_hand_which_styles_nothing() {
    let mut a = drawing();
    press(&mut a, 'h');
    assert_eq!(a.view().tool, ToolKind::Hand);
}

#[test]
fn a_hand_drag_changes_only_the_scroll_by_the_vertical_movement() {
    let mut a = drawing();
    stroke(&mut a, (50.0, 150.0), 150.0);
    key(&mut a, Key::Char('c'), true, false);
    let json = a.take_copy().expect("copied");
    let ink = a.ink().clone();
    press(&mut a, 'h');
    a.handle(Event::PointerDown(p(100.0, 200.0)));
    assert_eq!(a.view().cursor, Cursor::Hand { grabbing: true });
    a.handle(Event::PointerMove(p(180.0, 230.0)));
    assert_eq!(a.view().scroll, 30.0, "x is ignored");
    a.handle(Event::PointerMove(p(10.0, 210.0)));
    assert_eq!(a.view().scroll, 10.0);
    a.handle(Event::PointerUp(p(10.0, 210.0)));
    assert_eq!(a.view().cursor, Cursor::Hand { grabbing: false });
    assert_eq!(*a.ink(), ink);
    assert_eq!(a.selection(), []);
    key(&mut a, Key::Char('c'), true, false);
    assert_eq!(a.take_copy().expect("copied"), json);
    key(&mut a, Key::Char('z'), true, false);
    assert!(a.ink().is_empty(), "Undo removed the Stroke, not the drag");
    assert_eq!(a.view().scroll, 10.0);
}

#[test]
fn a_hand_drag_divides_by_the_scale_factor() {
    let mut a = drawing();
    a.handle(Event::ScaleFactor(2.0));
    press(&mut a, 'h');
    drag(&mut a, p(200.0, 200.0), p(200.0, 260.0));
    assert_eq!(a.view().scroll, 30.0);
}

#[test]
fn alt_at_the_press_makes_the_drag_a_hand_drag_in_any_tool() {
    let mut a = drawing();
    alt(&mut a, true);
    a.handle(Event::PointerDown(p(100.0, 200.0)));
    alt(&mut a, false);
    a.handle(Event::PointerMove(p(100.0, 240.0)));
    assert_eq!(
        a.view().scroll,
        40.0,
        "keeps scrolling after Alt is released"
    );
    a.handle(Event::PointerUp(p(100.0, 250.0)));
    assert_eq!(a.view().scroll, 50.0);
    assert!(a.ink().is_empty(), "no Element was created");
    assert_eq!(a.view().tool, ToolKind::Pen, "the Tool did not change");
    drag(&mut a, p(100.0, 100.0), p(100.0, 120.0));
    assert_eq!(a.view().scroll, 50.0, "the next drag is the Pen's again");
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn alt_pressed_during_a_stroke_changes_nothing() {
    let mut a = drawing();
    a.handle(Event::PointerDown(p(50.0, 200.0)));
    alt(&mut a, true);
    a.handle(Event::PointerMove(p(100.0, 200.0)));
    a.handle(Event::PointerUp(p(150.0, 200.0)));
    assert_eq!(a.view().scroll, 0.0);
    assert_eq!(a.ink().len(), 1);
    assert!(a.ink().elements()[0].points().len() >= 3);
}

#[test]
fn alt_and_a_click_on_a_toolbar_button_still_presses_it() {
    let mut a = drawing();
    alt(&mut a, true);
    let eraser = a.button_center(Button::Tool(2)).expect("toolbar");
    drag(&mut a, eraser, eraser);
    assert_eq!(a.view().tool, ToolKind::Eraser);
    assert_eq!(a.view().scroll, 0.0);
}

#[test]
fn the_cursor_is_the_hand_while_alt_is_held() {
    let mut a = drawing();
    a.handle(Event::PointerMove(p(200.0, 200.0)));
    assert!(matches!(a.view().cursor, Cursor::Pen { .. }));
    alt(&mut a, true);
    assert_eq!(a.view().cursor, Cursor::Hand { grabbing: false });
    a.handle(Event::PointerDown(p(200.0, 200.0)));
    alt(&mut a, false);
    assert_eq!(a.view().cursor, Cursor::Hand { grabbing: true });
    a.handle(Event::PointerUp(p(200.0, 200.0)));
    assert!(matches!(a.view().cursor, Cursor::Pen { .. }));
}

#[test]
fn a_zero_scroll_step_redraws_nothing() {
    let mut a = drawing();
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    a.render(&mut pm.as_mut(), Format::Rgba);
    assert!(!a.view().needs_render);
    assert!(!a.handle(Event::Scroll { dy: 0.0 }).needs_render);
    assert!(a.handle(Event::Scroll { dy: 3.0 }).needs_render);
}
