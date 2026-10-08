//! Seam 1: the toolbar, its buttons, Tool keys and cursor, through events.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test pixel coordinates are small and positive"
)]

use markuli_core::{
    Annotator, Button, Cursor, DisplayId, Event, Format, Key, Point, Theme, ToolKind,
};
use tiny_skia::PixmapMut;

const W: u32 = 640;
const H: u32 = 240;
const PEN: Button = Button::Tool(1);

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

fn center(a: &Annotator, button: Button) -> Point {
    a.button_center(button).expect("toolbar is visible")
}

fn click(a: &mut Annotator, at: Point) {
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerUp(at));
}

fn stroke(a: &mut Annotator, at: f32) {
    a.handle(Event::PointerDown(p(at, 200.0)));
    a.handle(Event::PointerMove(p(at + 5.0, 210.0)));
    a.handle(Event::PointerUp(p(at + 10.0, 220.0)));
}

fn key(a: &mut Annotator, key: Key, command: bool, shift: bool) {
    a.handle(Event::Key {
        key,
        command,
        shift,
        alt: false,
    });
}

fn pixels(a: &mut Annotator) -> Vec<u8> {
    let mut buf = vec![0_u8; (W * H * 4) as usize];
    let mut target = PixmapMut::from_bytes(&mut buf, W, H).unwrap();
    a.render(&mut target, Format::Rgba);
    buf
}

fn alpha(buf: &[u8], at: Point) -> u8 {
    let (x, y) = (at.x as usize, at.y as usize);
    buf[(y * W as usize + x) * 4 + 3]
}

#[test]
fn the_toolbar_is_drawn_in_draw_mode_and_hidden_outside_it() {
    let mut a = drawing();
    let pen = center(&a, PEN);
    // The row (about 580 px) is centred at the top of the Overlay.
    let (first, last) = (center(&a, Button::Tool(0)), center(&a, Button::Clear));
    assert!((f32::midpoint(first.x, last.x) - W as f32 / 2.0).abs() < 1.0 && pen.y < 80.0);
    assert_eq!(alpha(&pixels(&mut a), pen), 255);

    stroke(&mut a, 20.0); // Ink keeps the Overlay alive after leaving Draw Mode.
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    assert_eq!(alpha(&pixels(&mut a), pen), 0);
    assert_eq!(a.button_center(PEN), None);
}

#[test]
fn pressing_on_the_toolbar_never_starts_a_stroke() {
    let mut a = drawing();
    let (pen, undo) = (center(&a, PEN), center(&a, Button::Undo));
    click(&mut a, pen);
    a.handle(Event::PointerDown(undo));
    a.handle(Event::PointerMove(p(undo.x, 150.0)));
    a.handle(Event::PointerUp(p(undo.x, 200.0)));
    // A press on the island's padding is dead space, not a stroke either.
    click(&mut a, p(pen.x - 18.0, pen.y));
    assert!(a.ink().is_empty());
}

#[test]
fn a_stroke_may_cross_the_toolbar() {
    let mut a = drawing();
    let pen = center(&a, PEN);
    a.handle(Event::PointerDown(p(pen.x, 150.0)));
    a.handle(Event::PointerMove(pen));
    a.handle(Event::PointerUp(p(pen.x, 5.0)));
    assert_eq!(a.ink().len(), 1);
    assert_eq!(a.ink().elements()[0].points().len(), 3);
}

#[test]
fn the_undo_redo_and_clear_buttons_behave_like_their_keys() {
    let mut by_button = drawing();
    let mut by_key = drawing();
    for a in [&mut by_button, &mut by_key] {
        stroke(a, 20.0);
        stroke(a, 60.0);
    }
    let steps: [(Button, Key, bool); 4] = [
        (Button::Undo, Key::Char('z'), false),
        (Button::Redo, Key::Char('z'), true),
        (Button::Undo, Key::Char('z'), false),
        (Button::Undo, Key::Char('z'), false),
    ];
    for (button, k, shift) in steps {
        let at = center(&by_button, button);
        click(&mut by_button, at);
        key(&mut by_key, k, true, shift);
        assert_eq!(by_button.ink(), by_key.ink());
    }
    assert!(by_button.ink().is_empty());
    key(&mut by_key, Key::Char('y'), true, false);
    let redo = center(&by_button, Button::Redo);
    click(&mut by_button, redo);
    assert_eq!(by_button.ink(), by_key.ink());

    // Clear: removes all Ink and stays in Draw Mode, like the hotkey.
    let clear = center(&by_button, Button::Clear);
    click(&mut by_button, clear);
    let view = by_key.handle(Event::Clear);
    assert_eq!(by_button.ink(), by_key.ink());
    assert!(by_button.ink().is_empty());
    assert_eq!(by_button.view().draw_mode, view.draw_mode);
    assert!(view.draw_mode);
}

#[test]
fn a_click_must_press_and_release_on_the_same_button() {
    let mut a = drawing();
    stroke(&mut a, 20.0);
    let (undo, redo) = (center(&a, Button::Undo), center(&a, Button::Redo));
    a.handle(Event::PointerDown(undo));
    a.handle(Event::PointerUp(redo));
    a.handle(Event::PointerDown(undo));
    a.handle(Event::PointerUp(p(undo.x, 150.0)));
    assert_eq!(a.ink().len(), 1);
}

#[test]
fn p_and_7_select_the_pen_and_other_keys_do_not() {
    let mut a = drawing();
    for k in ['p', '7'] {
        key(&mut a, Key::Char('v'), false, false);
        key(&mut a, Key::Char(k), false, false);
        assert_eq!(a.view().tool, ToolKind::Pen);
    }
    key(&mut a, Key::Char('p'), true, false); // Cmd+P is not a Tool key.
    key(&mut a, Key::Char('q'), false, false);
    assert_eq!(a.view().tool, ToolKind::Pen);
    // And keys do nothing outside Draw Mode.
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    key(&mut a, Key::Char('7'), false, false);
    assert_eq!(a.view().tool, ToolKind::Pen);
}

#[test]
fn letters_select_tools() {
    // V Select, P Pen, E Eraser, K Laser; Excalidraw's 7 (Pen) and 0
    // (Eraser) stay as aliases.
    let table = [
        (ToolKind::Select, ['v', 'v']),
        (ToolKind::Pen, ['p', 'p']),
        (ToolKind::Eraser, ['e', 'e']),
        (ToolKind::Laser, ['k', 'k']),
        (ToolKind::Pen, ['7', '7']),
        (ToolKind::Eraser, ['0', '0']),
    ];
    let mut a = drawing();
    for (kind, keys) in table {
        for k in keys {
            // Start from another Tool so the key has to change something.
            let other = if kind == ToolKind::Select { 'p' } else { 'v' };
            key(&mut a, Key::Char(other), false, false);
            key(&mut a, Key::Char(k), false, false);
            assert_eq!(a.view().tool, kind, "key {k}");
        }
    }
}

#[test]
fn tool_keys_work_on_the_first_key_after_entering_draw_mode() {
    let mut a = drawing();
    key(&mut a, Key::Char('k'), false, false);
    assert_eq!(a.view().tool, ToolKind::Laser);
}

#[test]
fn the_cursor_is_an_arrow_over_the_toolbar_and_the_tools_own_elsewhere() {
    let mut a = drawing();
    let pen = center(&a, PEN);
    let ring = a.view().cursor;
    assert!(matches!(ring, Cursor::Pen { .. }));
    a.handle(Event::PointerMove(pen));
    assert_eq!(a.view().cursor, Cursor::Arrow);
    a.handle(Event::PointerMove(p(10.0, 200.0)));
    assert_eq!(a.view().cursor, ring);
}

#[test]
fn eraser_laser_and_select_have_their_own_cursors() {
    let mut a = drawing();
    key(&mut a, Key::Char('e'), false, false);
    // The Eraser hits Elements within 8 logical px of its centre.
    assert_eq!(a.view().cursor, Cursor::Eraser { diameter: 16 });
    key(&mut a, Key::Char('k'), false, false);
    assert_eq!(a.view().cursor, Cursor::Laser);
    key(&mut a, Key::Char('v'), false, false);
    assert_eq!(a.view().cursor, Cursor::Arrow);
}

#[test]
fn toolbar_changes_ask_for_a_render_and_only_repaint_the_toolbar() {
    let mut a = drawing();
    pixels(&mut a);
    assert!(!a.view().needs_render);
    let undo = center(&a, Button::Undo);
    let view = a.handle(Event::PointerMove(undo));
    assert!(view.needs_render, "hover highlight");
    let mut buf = vec![0_u8; (W * H * 4) as usize];
    let mut target = PixmapMut::from_bytes(&mut buf, W, H).unwrap();
    let damage = a.render(&mut target, Format::Rgba).expect("damage");
    assert!(damage.height < 100 && damage.width < W);
    assert!(!a.view().needs_render);
    let view = a.handle(Event::Theme(Theme::Dark));
    assert!(view.needs_render);
}

#[test]
fn ink_drawn_under_the_toolbar_does_not_erase_it() {
    let mut a = drawing();
    let pen = center(&a, PEN);
    let mut screen = vec![0_u8; (W * H * 4) as usize];
    let mut paint = |a: &mut Annotator| {
        let mut target = PixmapMut::from_bytes(&mut screen, W, H).unwrap();
        a.render(&mut target, Format::Rgba);
        screen.clone()
    };
    let before = paint(&mut a);
    a.handle(Event::PointerDown(p(pen.x, pen.y + 60.0)));
    a.handle(Event::PointerMove(p(pen.x, pen.y)));
    a.handle(Event::PointerMove(p(pen.x + 40.0, pen.y)));
    a.handle(Event::PointerUp(p(pen.x + 40.0, pen.y)));
    let after = paint(&mut a);
    assert_eq!(alpha(&after, pen), 255);
    assert_eq!(a.ink().len(), 1);
    // Away from the stroke the pixels are untouched by the repaint.
    let edge = (H as usize - 1) * W as usize * 4;
    assert_eq!(before[edge..edge + 64], after[edge..edge + 64]);
    assert_ne!(before, after);
}

#[test]
fn incremental_rendering_with_the_toolbar_matches_a_full_redraw() {
    let mut a = drawing();
    let (pen, undo) = (center(&a, PEN), center(&a, Button::Undo));
    let mut screen = vec![0_u8; (W * H * 4) as usize];
    let paint = |a: &mut Annotator, screen: &mut Vec<u8>| {
        let mut target = PixmapMut::from_bytes(screen, W, H).unwrap();
        a.render(&mut target, Format::Rgba);
    };
    paint(&mut a, &mut screen);
    // A stroke that crosses the toolbar, hover changes, undo and redo.
    let path = [p(pen.x - 60.0, 150.0), pen, undo, p(undo.x + 70.0, 5.0)];
    a.handle(Event::PointerDown(path[0]));
    paint(&mut a, &mut screen);
    for at in &path[1..] {
        a.handle(Event::PointerMove(*at));
        paint(&mut a, &mut screen);
    }
    a.handle(Event::PointerUp(path[3]));
    paint(&mut a, &mut screen);
    for button in [Button::Undo, Button::Redo, Button::Redo] {
        let at = center(&a, button);
        click(&mut a, at);
        paint(&mut a, &mut screen);
    }
    let mut fresh = vec![0_u8; (W * H * 4) as usize];
    a.handle(Event::SurfaceReset);
    paint(&mut a, &mut fresh);
    assert!(
        screen.iter().zip(&fresh).all(|(x, y)| x.abs_diff(*y) <= 1),
        "incremental pixels differ from a full redraw"
    );
}

#[test]
fn e_and_p_toggle_between_the_eraser_and_the_pen() {
    // P and E (and Excalidraw's 7 and 0) swap Pen and Eraser: the key of the
    // active Tool switches to the other one, any other key selects its Tool.
    for (pen_key, eraser_key) in [('p', 'e'), ('7', '0'), ('p', '0'), ('7', 'e')] {
        let mut a = drawing();
        assert_eq!(a.view().tool, ToolKind::Pen, "Draw Mode starts on the Pen");
        key(&mut a, Key::Char(pen_key), false, false);
        assert_eq!(a.view().tool, ToolKind::Eraser, "{pen_key} on the Pen");
        key(&mut a, Key::Char(pen_key), false, false);
        assert_eq!(a.view().tool, ToolKind::Pen, "{pen_key} again");
        key(&mut a, Key::Char(eraser_key), false, false);
        assert_eq!(a.view().tool, ToolKind::Eraser, "{eraser_key} on the Pen");
        key(&mut a, Key::Char(eraser_key), false, false);
        assert_eq!(a.view().tool, ToolKind::Pen, "{eraser_key} on the Eraser");
        key(&mut a, Key::Char(eraser_key), false, false);
        key(&mut a, Key::Char(eraser_key), false, false);
        key(&mut a, Key::Char(eraser_key), false, false);
        assert_eq!(a.view().tool, ToolKind::Eraser, "{eraser_key} three times");
    }
}

#[test]
fn e_and_p_select_their_tool_from_select_and_laser() {
    for from in ['v', 'k'] {
        for (k, tool) in [('p', ToolKind::Pen), ('e', ToolKind::Eraser)] {
            let mut a = drawing();
            key(&mut a, Key::Char(from), false, false);
            key(&mut a, Key::Char(k), false, false);
            assert_eq!(a.view().tool, tool, "{k} after {from}");
        }
    }
}

#[test]
fn the_other_tool_keys_do_not_toggle() {
    for (keys, tool) in [
        (['v', 'v'], ToolKind::Select),
        (['k', 'k'], ToolKind::Laser),
    ] {
        for k in keys {
            let mut a = drawing();
            key(&mut a, Key::Char('v'), false, false);
            for _ in 0..3 {
                key(&mut a, Key::Char(k), false, false);
                assert_eq!(a.view().tool, tool, "key {k}");
            }
        }
    }
}

#[test]
fn clicking_the_active_toolbar_button_does_not_toggle() {
    let mut a = drawing();
    let at = center(&a, Button::Tool(1));
    click(&mut a, at);
    click(&mut a, at);
    assert_eq!(a.view().tool, ToolKind::Pen);
}
