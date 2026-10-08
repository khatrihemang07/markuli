//! Seam 1: the style panel with a Selection: restyle, undo, copy, redraw.

use markuli_core::{Annotator, Control, DisplayId, Event, Format, Key, Point};

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn session() -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a
}

const W: u32 = 800;
const H: u32 = 600;

fn key(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: false,
        shift: false,
        alt: false,
    });
}

fn command(a: &mut Annotator, c: char, shift: bool) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: true,
        shift,
        alt: false,
    });
}

fn click(a: &mut Annotator, at: Point) {
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerUp(at));
}

fn stroke(a: &mut Annotator, y: f32) {
    key(a, '2');
    a.handle(Event::PointerDown(p(300.0, y)));
    a.handle(Event::PointerMove(p(350.0, y)));
    a.handle(Event::PointerUp(p(400.0, y)));
}

fn centre(a: &Annotator, control: Control) -> Point {
    a.panel_center(control).expect("the panel is shown")
}

fn pick(a: &mut Annotator, control: Control) {
    click(a, centre(a, control));
}

const RED: [u8; 3] = [0xe0, 0x31, 0x31];
const GREEN: [u8; 3] = [0x2f, 0x9e, 0x44];
const BLUE: [u8; 3] = [0x19, 0x71, 0xc2];

fn colors(a: &Annotator) -> Vec<[u8; 3]> {
    a.ink()
        .elements()
        .iter()
        .map(markuli_core::Element::stroke_color)
        .collect()
}

fn opacities(a: &Annotator) -> Vec<u8> {
    a.ink()
        .elements()
        .iter()
        .map(markuli_core::Element::opacity)
        .collect()
}

/// Three strokes at y = 100, 200, 300; the Select Tool with the first and
/// third selected.
fn first_and_third_selected() -> Annotator {
    let mut a = session();
    for y in [100.0, 200.0, 300.0] {
        stroke(&mut a, y);
    }
    key(&mut a, 'v');
    click(&mut a, p(350.0, 100.0));
    a.handle(Event::Modifiers { shift: true });
    click(&mut a, p(350.0, 300.0));
    a.handle(Event::Modifiers { shift: false });
    a
}

#[test]
fn with_a_selection_a_choice_restyles_exactly_the_selected_elements() {
    let mut a = first_and_third_selected();
    assert_eq!(a.selection().len(), 2);
    pick(&mut a, Control::Color(3));
    pick(&mut a, Control::Width(0));
    pick(&mut a, Control::Opacity(40));
    for (i, selected) in [true, false, true].into_iter().enumerate() {
        let e = &a.ink().elements()[i];
        assert_eq!(e.stroke_color() == BLUE, selected, "colour of {i}");
        assert_eq!((e.stroke_width() - 1.0).abs() < f32::EPSILON, selected);
        assert_eq!(e.opacity() == 40, selected, "opacity of {i}");
    }
    // The next Strokes keep their own style.
    stroke(&mut a, 400.0);
    assert_eq!(a.ink().elements()[3].stroke_color(), RED);
    assert_eq!(a.ink().elements()[3].opacity(), 100);
}

#[test]
fn a_restyle_is_one_undo_step_per_choice_and_redo_applies_it_again() {
    let mut a = first_and_third_selected();
    pick(&mut a, Control::Color(3));
    pick(&mut a, Control::Color(2));
    assert_eq!(colors(&a), [GREEN, RED, GREEN]);
    command(&mut a, 'z', false);
    assert_eq!(colors(&a), [BLUE, RED, BLUE]);
    command(&mut a, 'z', false);
    assert_eq!(colors(&a), [RED, RED, RED]);
    assert_eq!(a.ink().len(), 3, "the strokes themselves stay");
    command(&mut a, 'z', true);
    command(&mut a, 'z', true);
    assert_eq!(colors(&a), [GREEN, RED, GREEN]);
}

#[test]
fn choosing_what_is_already_set_logs_nothing() {
    let mut a = first_and_third_selected();
    pick(&mut a, Control::Color(1));
    pick(&mut a, Control::Opacity(100));
    command(&mut a, 'z', false);
    assert_eq!(
        a.ink().len(),
        2,
        "undo removed the last stroke, not a no-op"
    );
}

#[test]
fn dragging_the_slider_is_one_undo_step() {
    let mut a = first_and_third_selected();
    a.handle(Event::PointerDown(centre(&a, Control::Opacity(100))));
    for v in [80, 60, 40, 20] {
        let at = centre(&a, Control::Opacity(v));
        a.handle(Event::PointerMove(at));
    }
    a.handle(Event::PointerUp(centre(&a, Control::Opacity(20))));
    assert_eq!(opacities(&a), [20, 100, 20]);
    command(&mut a, 'z', false);
    assert_eq!(opacities(&a), [100, 100, 100]);
}

#[test]
fn the_selection_survives_a_restyle() {
    let mut a = first_and_third_selected();
    let before = a.selection().to_vec();
    pick(&mut a, Control::Width(2));
    assert_eq!(a.selection(), before);
    assert!(a.panel_center(Control::Color(0)).is_some());
}

#[test]
fn copied_excalidraw_json_reflects_the_restyled_values() {
    let mut a = first_and_third_selected();
    pick(&mut a, Control::Color(4));
    pick(&mut a, Control::Width(2));
    pick(&mut a, Control::Opacity(70));
    command(&mut a, 'c', false);
    let text = a.take_copy().expect("something is selected");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let elements = doc["elements"].as_array().expect("elements");
    assert_eq!(elements.len(), 2);
    for e in elements {
        assert_eq!(e["strokeColor"], "#f08c00");
        assert_eq!(e["strokeWidth"], 4);
        assert_eq!(e["opacity"], 70);
    }
    // Undo all three, and the copy goes back to the old values.
    for _ in 0..3 {
        command(&mut a, 'z', false);
    }
    command(&mut a, 'c', false);
    let text = a.take_copy().expect("still selected");
    assert!(text.contains(r##""strokeColor":"#e03131""##));
    assert!(text.contains(r#""strokeWidth":2,"#));
    assert!(text.contains(r#""opacity":100,"#));
}

/// The Overlay drawn into `buf`, which keeps what earlier calls drew.
fn render(a: &mut Annotator, buf: &mut [u8]) {
    let mut target = tiny_skia::PixmapMut::from_bytes(buf, W, H).expect("size");
    a.render(&mut target, Format::Rgba);
}

/// Reddish pixels in the rows around `y` of the stroke area (x 280..420),
/// clear of the panel.
fn inked(buf: &[u8], y: usize) -> usize {
    (y - 30..y + 30)
        .flat_map(|row| (280..420).map(move |x| (row * W as usize + x) * 4))
        .filter(|&i| buf[i] > 100 && buf[i + 1] < 100)
        .count()
}

#[test]
fn a_new_width_redraws_the_stroke_and_undo_gives_back_the_old_pixels() {
    let mut a = session();
    stroke(&mut a, 100.0);
    key(&mut a, 'v');
    click(&mut a, p(350.0, 100.0));
    let mut screen = vec![0_u8; (W * H * 4) as usize];
    render(&mut a, &mut screen);
    let medium = screen.clone();

    // Incremental redraws into the same buffer, like the real Overlay.
    pick(&mut a, Control::Width(2));
    render(&mut a, &mut screen);
    assert!(
        inked(&screen, 100) * 2 > inked(&medium, 100) * 3,
        "bold ink is thicker: {} vs {}",
        inked(&screen, 100),
        inked(&medium, 100)
    );
    pick(&mut a, Control::Width(0));
    render(&mut a, &mut screen);
    assert!(
        inked(&screen, 100) < inked(&medium, 100),
        "thin ink leaves no bold pixels behind"
    );
    command(&mut a, 'z', false);
    command(&mut a, 'z', false);
    render(&mut a, &mut screen);
    // The stroke's area only: the toolbar's undo and redo buttons changed.
    let worst = (60..140)
        .flat_map(|row| (270..430).map(move |x| (row * W as usize + x) * 4))
        .flat_map(|i| (i..i + 4).map(|c| screen[c].abs_diff(medium[c])))
        .max()
        .unwrap_or(0);
    assert!(worst <= 64, "undo restored the old look (worst {worst})");
}
