//! Seam 1, pixels out: the toolbar against golden images.
//!
//! Each scene is composited over a page colour so the PNGs are easy to look
//! at. Regenerate after an intended change with `UPDATE_GOLDEN=1 cargo test`
//! and look at the PNGs in `tests/golden/` before committing them.

use markuli_core::{Annotator, Button, DisplayId, Event, Format, Key, Point, Theme};
use std::path::PathBuf;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

const W: u32 = 360;
const H: u32 = 150;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// Draw Mode on a `W` x `H` Overlay at `scale`, in `theme`.
fn scene(theme: Theme, scale: f32) -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Theme(theme));
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::ToggleDrawMode(DisplayId(1)));
    a
}

fn stroke(a: &mut Annotator) {
    a.handle(Event::PointerDown(p(40.0, 120.0)));
    a.handle(Event::PointerMove(p(120.0, 100.0)));
    a.handle(Event::PointerUp(p(200.0, 125.0)));
}

fn image(a: &mut Annotator, page: Color) -> Pixmap {
    let mut overlay = Pixmap::new(W, H).expect("size is non-zero");
    a.render(&mut overlay.as_mut(), Format::Rgba);
    over(&overlay, page)
}

/// The Overlay buffer composited over the page.
fn over(overlay: &Pixmap, page: Color) -> Pixmap {
    let mut out = Pixmap::new(W, H).expect("size is non-zero");
    out.fill(page);
    out.draw_pixmap(
        0,
        0,
        overlay.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    out
}

fn check(name: &str, actual: &Pixmap) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create golden dir");
        actual.save_png(&path).expect("write golden");
        return;
    }
    let golden = Pixmap::load_png(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {} ({e}); run with UPDATE_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!((golden.width(), golden.height()), (W, H), "{name}: size");
    // The SIMD and scalar rasterizer paths may differ by a rounding step.
    let worst = golden
        .data()
        .iter()
        .zip(actual.data())
        .map(|(g, a)| g.abs_diff(*a))
        .max()
        .unwrap_or(0);
    assert!(worst <= 2, "{name}: differs from its golden by {worst}");
}

const WHITE_PAGE: Color = Color::WHITE;

fn dark_page() -> Color {
    Color::from_rgba8(0x1e, 0x1e, 0x1e, 255)
}

#[test]
fn toolbar_light_theme() {
    let mut a = scene(Theme::Light, 2.0);
    check("toolbar_light", &image(&mut a, WHITE_PAGE));
}

#[test]
fn toolbar_dark_theme() {
    let mut a = scene(Theme::Dark, 2.0);
    check("toolbar_dark", &image(&mut a, dark_page()));
}

#[test]
fn toolbar_with_ink_enables_undo_and_clear_and_highlights_hover() {
    let mut a = scene(Theme::Light, 1.0);
    stroke(&mut a);
    let undo = a.button_center(Button::Undo).expect("toolbar visible");
    a.handle(Event::PointerMove(undo));
    check("toolbar_light_ink_hover_undo", &image(&mut a, WHITE_PAGE));
}

#[test]
fn toolbar_after_undo_enables_redo_in_dark_theme() {
    let mut a = scene(Theme::Dark, 1.0);
    stroke(&mut a);
    let undo = a.button_center(Button::Undo).expect("toolbar visible");
    a.handle(Event::PointerDown(undo));
    a.handle(Event::PointerUp(undo));
    a.handle(Event::PointerMove(p(300.0, 100.0)));
    check("toolbar_dark_after_undo", &image(&mut a, dark_page()));
}

fn key(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: false,
        shift: false,
    });
}

/// A wavy Laser drag of 60 points, 6 ms apart from t = 1000; returns the time
/// of the pointer up.
fn laser_swoosh(a: &mut Annotator) -> u64 {
    key(a, 'k');
    let at = |i: u32| {
        let t = f32::from(u16::try_from(i).expect("small"));
        p(25.0 + t * 5.0, 100.0 + 25.0 * (t / 7.0).sin())
    };
    a.handle(Event::Clock(1000));
    a.handle(Event::PointerDown(at(0)));
    for i in 1..60 {
        a.handle(Event::Clock(1000 + u64::from(i) * 6));
        a.handle(Event::PointerMove(at(i)));
    }
    a.handle(Event::Clock(1000 + 60 * 6));
    a.handle(Event::PointerUp(at(60)));
    1000 + 60 * 6
}

#[test]
fn laser_trail_just_after_the_drag() {
    let mut a = scene(Theme::Light, 1.0);
    let end = laser_swoosh(&mut a);
    a.handle(Event::Clock(end + 16));
    check("laser_fresh", &image(&mut a, WHITE_PAGE));
}

#[test]
fn laser_trail_mid_fade() {
    let mut a = scene(Theme::Light, 1.0);
    let end = laser_swoosh(&mut a);
    a.handle(Event::Clock(end + 16));
    // Frames draw incrementally into the same buffer, like the real Overlay.
    let mut overlay = Pixmap::new(W, H).expect("size is non-zero");
    a.render(&mut overlay.as_mut(), Format::Rgba);
    a.handle(Event::Clock(end + 800));
    a.render(&mut overlay.as_mut(), Format::Rgba);
    check("laser_mid_fade", &over(&overlay, WHITE_PAGE));
}

#[test]
fn eraser_drag_draws_the_marked_stroke_faded() {
    let mut a = scene(Theme::Light, 1.0);
    stroke(&mut a);
    key(&mut a, 'e');
    // Marks the stroke; the drag is still going, so it is drawn faded.
    a.handle(Event::PointerDown(p(120.0, 100.0)));
    a.handle(Event::PointerMove(p(122.0, 101.0)));
    check("eraser_pending", &image(&mut a, WHITE_PAGE));
}
