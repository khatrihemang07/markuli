//! Seam 1, pixel output: render into a caller-owned buffer, check pixels and damage.

#![allow(clippy::cast_precision_loss, reason = "tiny test values")]

use markuli_core::{Annotator, Damage, DisplayId, Event, Format, Point, Theme};
use tiny_skia::{Pixmap, PixmapMut};

const D1: DisplayId = DisplayId::new(1);
const W: u32 = 100;
const H: u32 = 100;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn render(a: &mut Annotator, pm: &mut Pixmap, format: Format) -> Option<Damage> {
    let mut m: PixmapMut<'_> = pm.as_mut();
    a.render(&mut m, format)
}

/// Premultiplied RGBA bytes of one pixel.
fn px(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * W + x) * 4) as usize;
    pm.data()[i..i + 4].try_into().expect("4 bytes")
}

fn drawing() -> (Annotator, Pixmap) {
    let mut a = Annotator::new();
    a.handle(Event::ToggleDrawMode(D1));
    (a, Pixmap::new(W, H).expect("pixmap"))
}

#[test]
fn a_stroke_paints_opaque_ink_colour_along_its_path_only() {
    let (mut a, mut pm) = drawing();
    render(&mut a, &mut pm, Format::Rgba);
    a.handle(Event::PointerDown(p(10.0, 50.0)));
    a.handle(Event::PointerMove(p(60.0, 50.0)));
    a.handle(Event::PointerUp(p(60.0, 50.0)));
    render(&mut a, &mut pm, Format::Rgba);
    // #e03131 on the path, fully opaque.
    assert_eq!(px(&pm, 35, 50), [0xe0, 0x31, 0x31, 0xff]);
    // Far from the path: only the hit-test backdrop, not visible ink.
    assert!(px(&pm, 35, 90)[3] <= 1);
}

#[test]
fn bgra_format_swaps_red_and_blue_for_windows_buffers() {
    let (mut a, mut pm) = drawing();
    render(&mut a, &mut pm, Format::Bgra);
    a.handle(Event::PointerDown(p(10.0, 50.0)));
    a.handle(Event::PointerMove(p(60.0, 50.0)));
    a.handle(Event::PointerUp(p(60.0, 50.0)));
    render(&mut a, &mut pm, Format::Bgra);
    assert_eq!(px(&pm, 35, 50), [0x31, 0x31, 0xe0, 0xff]);
}

#[test]
fn draw_mode_has_a_nearly_invisible_backdrop_so_the_os_routes_clicks_to_the_overlay() {
    let (mut a, mut pm) = drawing();
    render(&mut a, &mut pm, Format::Rgba);
    assert_eq!(px(&pm, 5, 5), [0, 0, 0, 1]);
}

#[test]
fn outside_draw_mode_the_overlay_is_fully_transparent_where_there_is_no_ink() {
    let (mut a, mut pm) = drawing();
    render(&mut a, &mut pm, Format::Rgba);
    a.handle(Event::PointerDown(p(10.0, 50.0)));
    a.handle(Event::PointerMove(p(60.0, 50.0)));
    a.handle(Event::PointerUp(p(60.0, 50.0)));
    a.handle(Event::ToggleDrawMode(D1));
    render(&mut a, &mut pm, Format::Rgba);
    assert_eq!(px(&pm, 5, 5), [0, 0, 0, 0]);
    assert_eq!(px(&pm, 35, 50), [0xe0, 0x31, 0x31, 0xff]);
}

#[test]
fn render_with_nothing_changed_does_no_work() {
    let (mut a, mut pm) = drawing();
    assert!(a.view().needs_render);
    assert!(render(&mut a, &mut pm, Format::Rgba).is_some());
    assert!(!a.view().needs_render);
    assert_eq!(render(&mut a, &mut pm, Format::Rgba), None);
}

#[test]
fn damage_after_a_new_segment_is_local_not_the_whole_screen() {
    let (mut a, mut pm) = drawing();
    render(&mut a, &mut pm, Format::Rgba);
    a.handle(Event::PointerDown(p(10.0, 50.0)));
    a.handle(Event::PointerMove(p(20.0, 50.0)));
    render(&mut a, &mut pm, Format::Rgba);
    a.handle(Event::PointerMove(p(40.0, 60.0)));
    let d = render(&mut a, &mut pm, Format::Rgba).expect("damage");
    // Only the changed part of the live outline, not the whole screen. That the
    // damage covers every changed pixel is checked in `tests/damage.rs`.
    assert!(d.width < W / 2 + 10 && d.height < H / 2, "{d:?}");
}

#[test]
fn a_surface_reset_redraws_all_ink_into_the_fresh_buffer() {
    let (mut a, mut pm) = drawing();
    a.handle(Event::PointerDown(p(10.0, 50.0)));
    a.handle(Event::PointerMove(p(60.0, 50.0)));
    a.handle(Event::PointerUp(p(60.0, 50.0)));
    render(&mut a, &mut pm, Format::Rgba);
    let mut fresh = Pixmap::new(W, H).expect("pixmap");
    a.handle(Event::SurfaceReset);
    let d = render(&mut a, &mut fresh, Format::Rgba).expect("damage");
    assert_eq!(
        d,
        Damage {
            x: 0,
            y: 0,
            width: W,
            height: H
        }
    );
    assert_eq!(px(&fresh, 35, 50), [0xe0, 0x31, 0x31, 0xff]);
}

#[test]
fn a_single_click_leaves_a_dot() {
    let (mut a, mut pm) = drawing();
    render(&mut a, &mut pm, Format::Rgba);
    a.handle(Event::PointerDown(p(50.0, 50.0)));
    a.handle(Event::PointerUp(p(50.0, 50.0)));
    render(&mut a, &mut pm, Format::Rgba);
    assert_eq!(px(&pm, 50, 50), [0xe0, 0x31, 0x31, 0xff]);
}

#[test]
fn render_never_panics_on_degenerate_input() {
    let (mut a, mut pm) = drawing();
    a.handle(Event::PointerDown(p(f32::NAN, 5.0)));
    a.handle(Event::PointerMove(p(f32::INFINITY, -1e9)));
    a.handle(Event::PointerMove(p(1e9, 1e9)));
    a.handle(Event::PointerUp(p(-5.0, -5.0)));
    render(&mut a, &mut pm, Format::Rgba);
}

#[test]
fn random_event_sequences_never_panic() {
    // Tiny LCG: no dev-dependency needed for a deterministic fuzz.
    let mut state: u32 = 0x1234_5678;
    let mut next = move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state >> 8
    };
    let mut a = Annotator::new();
    let mut pm = Pixmap::new(W, H).expect("pixmap");
    for _ in 0..1_500 {
        let at = p((next() % 160) as f32 - 30.0, (next() % 160) as f32 - 30.0);
        let event = match next() % 9 {
            0 => Event::ToggleDrawMode(DisplayId::new(u64::from(next() % 2))),
            1 => Event::SurfaceReset,
            2 => Event::PointerDown(at),
            3 | 4 => Event::PointerMove(at),
            5 => Event::Resize {
                width: next() % 200,
                height: next() % 100,
            },
            7 => Event::ScaleFactor([0.0, 1.0, 2.0, f32::NAN][(next() % 4) as usize]),
            6 => Event::Theme(if next() % 2 == 0 {
                Theme::Light
            } else {
                Theme::Dark
            }),
            _ => Event::PointerUp(at),
        };
        a.handle(event);
        render(&mut a, &mut pm, Format::Rgba);
    }
}
