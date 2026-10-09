//! Seam 1: a width slot is the drawn line's thickness in pt (logical px) at
//! mid pressure. Thickness is measured from rendered pixels.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "tiny test values"
)]

use markuli_core::{Annotator, Cursor, DisplayId, Event, Format, Key, Palette, Point};
use tiny_skia::Pixmap;

const W: u32 = 400;
const H: u32 = 200;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn session(scale: f32) -> (Annotator, Pixmap) {
    let mut a = Annotator::new();
    let (w, h) = ((W as f32 * scale) as u32, (H as f32 * scale) as u32);
    a.handle(Event::Resize {
        width: w,
        height: h,
    });
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    (a, Pixmap::new(w, h).expect("pixmap"))
}

/// A straight horizontal stroke at constant, non-simulated mid pressure.
fn draw(a: &mut Annotator, scale: f32, y: f32) {
    a.handle(Event::Pressure(Some(0.5001)));
    a.handle(Event::PointerDown(p(40.0 * scale, y * scale)));
    for i in 1..=30 {
        a.handle(Event::PointerMove(p(
            (40.0 + 6.0 * i as f32) * scale,
            y * scale,
        )));
    }
    a.handle(Event::PointerUp(p(220.0 * scale, y * scale)));
}

/// Summed alpha, in logical px, of the column at logical x, rows 60..140.
fn column(pm: &Pixmap, x: f32, scale: f32) -> f32 {
    let col = (x * scale) as u32;
    let sum: u32 = ((60.0 * scale) as u32..(140.0 * scale) as u32)
        .map(|y| u32::from(pm.data()[((y * pm.width() + col) * 4 + 3) as usize]))
        .sum();
    sum as f32 / 255.0 / scale
}

/// Ink thickness at logical x=130: that column minus the Draw Mode backdrop,
/// measured in a column the stroke does not reach.
fn thickness(a: &mut Annotator, pm: &mut Pixmap, scale: f32) -> f32 {
    a.render(&mut pm.as_mut(), Format::Rgba);
    column(pm, 130.0, scale) - column(pm, 20.0, scale)
}

fn measure(pt: f32, scale: f32) -> f32 {
    let (mut a, mut pm) = session(scale);
    a.handle(Event::EditWidth { slot: 1, width: pt });
    // The toolbar is not at y=100.
    draw(&mut a, scale, 100.0);
    thickness(&mut a, &mut pm, scale)
}

#[test]
fn a_slot_of_n_pt_draws_a_stroke_n_pt_thick() {
    for scale in [1.0, 2.0] {
        for pt in [2.0, 4.0, 8.0] {
            let got = measure(pt, scale);
            assert!(
                (got - pt).abs() <= pt * 0.12 + 0.3,
                "{pt} pt at scale {scale} measured {got}"
            );
        }
    }
}

#[test]
fn copied_json_stroke_width_is_pt_over_6_01() {
    let (mut a, _pm) = session(1.0);
    a.handle(Event::EditWidth {
        slot: 1,
        width: 6.0,
    });
    draw(&mut a, 1.0, 100.0);
    a.handle(Event::Key {
        key: Key::Char('a'),
        command: true,
        shift: false,
        alt: false,
    });
    a.handle(Event::Key {
        key: Key::Char('c'),
        command: true,
        shift: false,
        alt: false,
    });
    let text = a.take_copy().expect("ink to copy");
    let at = text.find("\"strokeWidth\":").expect("strokeWidth") + 14;
    let end = text[at..].find(',').expect("comma") + at;
    let sw: f32 = text[at..end].parse().expect("number");
    assert!((sw - 6.0 / 6.01).abs() < 0.01, "{sw}");
    // Excalidraw reads it back as 4.25 * sqrt(2) * strokeWidth pt.
    assert!((sw * 4.25 * std::f32::consts::SQRT_2 - 6.0).abs() < 0.01);
}

#[test]
fn the_new_defaults() {
    let d = Palette::DEFAULT;
    assert_eq!(d.colors[0], [0xe0, 0x31, 0x31]);
    assert_eq!(d.colors[1], [0x19, 0x71, 0xc2]);
    assert_eq!(d.colors[2], [0x2f, 0x9e, 0x44]);
    assert_eq!(d.colors[3], [0xfa, 0xb0, 0x05]);
    assert_eq!(d.colors[4], [0x1e, 0x1e, 0x1e]);
    assert_eq!(d.widths, [2.0, 4.0, 8.0]);
    let (mut a, _) = session(1.0);
    assert_eq!((a.view().style.color, a.view().style.width), (0, 1));
    a.handle(Event::Key {
        key: Key::Char('1'),
        command: false,
        shift: false,
        alt: false,
    });
    assert_eq!(a.view().palette.colors[a.view().style.color], d.colors[0]);
}

#[test]
fn widths_clamp_to_half_to_thirty_and_snap_to_quarters() {
    assert_eq!(Palette::snap_width(0.1), Some(0.5));
    assert_eq!(Palette::snap_width(99.0), Some(30.0));
    assert_eq!(Palette::snap_width(2.3), Some(2.25));
    assert_eq!(Palette::snap_width(2.4), Some(2.5));
    assert_eq!(Palette::parse_width("2,25"), Some(2.25));
    assert_eq!(Palette::width_text(2.25), "2.25");
    assert_eq!(Palette::width_text(2.5), "2.5");
    assert_eq!(Palette::width_text(2.0), "2");
    let (mut a, _) = session(1.0);
    a.handle(Event::EditWidth {
        slot: 0,
        width: 31.0,
    });
    assert!((a.view().palette.widths[0] - 30.0).abs() < f32::EPSILON);
    a.handle(Event::EditWidth {
        slot: 0,
        width: 3.1,
    });
    assert!((a.view().palette.widths[0] - 3.0).abs() < f32::EPSILON);
}

#[test]
fn the_pen_ring_follows_the_width_in_pt() {
    let (mut a, _) = session(1.0);
    let mut rings = Vec::new();
    for pt in [2.0, 4.0, 8.0] {
        a.handle(Event::EditWidth { slot: 1, width: pt });
        let Cursor::Pen { diameter, .. } = a.view().cursor else {
            panic!("pen cursor");
        };
        rings.push(diameter);
    }
    assert!(rings[0] < rings[1] && rings[1] < rings[2], "{rings:?}");
    assert_eq!(rings[2], 8);
}
