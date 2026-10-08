//! Seam 1: the style panel through the annotator interface only.

use markuli_core::{Annotator, Control, DisplayId, Event, Key, Point};

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

/// A horizontal Pen stroke at height `y`.
fn stroke(a: &mut Annotator, y: f32) {
    key(a, 'v');
    key(a, 'p');
    a.handle(Event::PointerDown(p(300.0, y)));
    a.handle(Event::PointerMove(p(350.0, y)));
    a.handle(Event::PointerUp(p(400.0, y)));
}

fn centre(a: &Annotator, control: Control) -> Point {
    a.panel_center(control).expect("the panel is shown")
}

fn pick(a: &mut Annotator, control: Control) {
    let at = centre(a, control);
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerUp(at));
}

fn small(i: usize) -> f32 {
    f32::from(u8::try_from(i).expect("small"))
}

const RED: [u8; 3] = [0xe0, 0x31, 0x31];
const GREEN: [u8; 3] = [0x2f, 0x9e, 0x44];

#[test]
fn without_a_selection_a_choice_styles_the_next_strokes_only() {
    let mut a = session();
    stroke(&mut a, 100.0);
    pick(&mut a, Control::Color(2));
    pick(&mut a, Control::Width(2));
    stroke(&mut a, 200.0);
    let [first, second] = a.ink().elements() else {
        panic!("two strokes")
    };
    assert_eq!(first.stroke_color(), RED);
    assert!((first.stroke_width() - 2.0).abs() < f32::EPSILON);
    assert_eq!(first.opacity(), 100);
    assert_eq!(second.stroke_color(), GREEN);
    assert!((second.stroke_width() - 4.0).abs() < f32::EPSILON);
    assert_eq!(second.opacity(), 100);
}

#[test]
fn the_palette_is_excalidraws_quick_picks_and_widths_are_thin_medium_bold() {
    let mut a = session();
    let colors = [
        [0x1e, 0x1e, 0x1e],
        RED,
        GREEN,
        [0x19, 0x71, 0xc2],
        [0xf0, 0x8c, 0x00],
    ];
    for (i, color) in colors.into_iter().enumerate() {
        pick(&mut a, Control::Color(i));
        stroke(&mut a, 100.0 + 20.0 * small(i));
        assert_eq!(
            a.ink()
                .elements()
                .last()
                .map(markuli_core::Element::stroke_color),
            Some(color)
        );
    }
    for (i, width) in [1.0_f32, 2.0, 4.0].into_iter().enumerate() {
        pick(&mut a, Control::Width(i));
        stroke(&mut a, 300.0 + 20.0 * small(i));
        let drawn = a
            .ink()
            .elements()
            .last()
            .map(markuli_core::Element::stroke_width);
        assert_eq!(drawn, Some(width));
    }
}

#[test]
fn pointer_presses_on_the_panel_never_start_a_stroke() {
    let mut a = session();
    let swatch = centre(&a, Control::Color(0));
    // The padding around the controls is panel too.
    let padding = p(swatch.x - 14.0, swatch.y - 14.0);
    for at in [swatch, padding, centre(&a, Control::Width(1))] {
        a.handle(Event::PointerDown(at));
        a.handle(Event::PointerMove(p(at.x + 3.0, at.y + 2.0)));
        a.handle(Event::PointerUp(p(at.x + 3.0, at.y + 2.0)));
    }
    assert!(a.ink().is_empty());
}

#[test]
fn the_panel_is_shown_for_the_pen_only_in_draw_mode() {
    let mut a = Annotator::new();
    a.handle(Event::Resize {
        width: 800,
        height: 600,
    });
    assert_eq!(a.panel_center(Control::Color(0)), None);
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    assert!(a.panel_center(Control::Color(0)).is_some());
    for hidden in ['e', 'k', 'v'] {
        key(&mut a, hidden);
        assert_eq!(a.panel_center(Control::Color(0)), None, "tool {hidden}");
    }
    key(&mut a, 'p');
    assert!(a.panel_center(Control::Color(0)).is_some());
}

fn paint(a: &mut Annotator, pm: &mut tiny_skia::Pixmap) -> Option<markuli_core::Damage> {
    a.render(&mut pm.as_mut(), markuli_core::Format::Rgba)
}

#[test]
fn moving_within_the_same_swatch_changes_nothing_and_reports_no_damage() {
    let mut a = session();
    let mut pm = tiny_skia::Pixmap::new(800, 600).expect("pixmap");
    let c = centre(&a, Control::Color(1));
    a.handle(Event::PointerMove(c));
    assert!(
        paint(&mut a, &mut pm).is_some(),
        "entering the swatch repaints"
    );
    for dx in [1.0, 2.0, -1.0, 0.5] {
        let view = a.handle(Event::PointerMove(p(c.x + dx, c.y + 1.0)));
        assert!(!view.needs_render, "same swatch: nothing to render");
        assert_eq!(paint(&mut a, &mut pm), None);
    }
    // Another swatch does change what is shown.
    let view = a.handle(Event::PointerMove(centre(&a, Control::Color(2))));
    assert!(view.needs_render);
}

#[test]
fn the_pen_cursor_is_a_ring_as_wide_as_the_stroke_in_the_stroke_color() {
    let mut a = session();
    // Excalidraw's freedraw size is 4.25 x the stroke width (1, 2, 4), so the
    // default medium Pen is 8.5 logical px wide, rounded to 9.
    assert_eq!(
        a.view().cursor,
        markuli_core::Cursor::Pen {
            diameter: 9,
            color: RED
        }
    );
    let pick = |a: &mut Annotator, c: Control| {
        let at = a.panel_center(c).expect("the panel is shown");
        a.handle(Event::PointerDown(at));
        a.handle(Event::PointerUp(at));
        a.handle(Event::PointerMove(p(10.0, 200.0)));
    };
    pick(&mut a, Control::Width(2));
    assert_eq!(
        a.view().cursor,
        markuli_core::Cursor::Pen {
            diameter: 17,
            color: RED
        }
    );
    pick(&mut a, Control::Width(0));
    // The thinnest stroke would be 4 px: a ring that small is hard to see.
    assert_eq!(
        a.view().cursor,
        markuli_core::Cursor::Pen {
            diameter: 6,
            color: RED
        }
    );
    pick(&mut a, Control::Color(3));
    assert_eq!(
        a.view().cursor,
        markuli_core::Cursor::Pen {
            diameter: 6,
            color: [0x19, 0x71, 0xc2]
        }
    );
}
