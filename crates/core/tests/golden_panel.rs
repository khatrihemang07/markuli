//! Seam 1, pixels out: the style panel against golden images.
//!
//! Composited over a page colour so the PNGs are easy to look at. Regenerate
//! after an intended change with `UPDATE_GOLDEN=1 cargo test` and look at the
//! PNGs in `tests/golden/` before committing them.

use markuli_core::{Annotator, Control, DisplayId, Event, Format, Key, Point, Theme};
use std::path::PathBuf;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

const W: u32 = 720;
const H: u32 = 260;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

fn scene(theme: Theme) -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Theme(theme));
    a.handle(Event::Resize {
        width: W,
        height: H,
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

fn stroke(a: &mut Annotator) {
    a.handle(Event::PointerDown(p(220.0, 200.0)));
    a.handle(Event::PointerMove(p(330.0, 150.0)));
    a.handle(Event::PointerMove(p(440.0, 215.0)));
    a.handle(Event::PointerUp(p(560.0, 170.0)));
}

fn pick(a: &mut Annotator, control: Control) {
    let at = a.panel_center(control).expect("the panel is shown");
    a.handle(Event::PointerDown(at));
    a.handle(Event::PointerUp(at));
}

fn image(a: &mut Annotator, page: Color) -> Pixmap {
    let mut overlay = Pixmap::new(W, H).expect("size is non-zero");
    a.render(&mut overlay.as_mut(), Format::Rgba);
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
    let worst = golden
        .data()
        .iter()
        .zip(actual.data())
        .map(|(g, a)| g.abs_diff(*a))
        .max()
        .unwrap_or(0);
    assert!(worst <= 2, "{name}: differs from its golden by {worst}");
}

#[test]
fn style_panel_light_with_the_default_style() {
    let mut a = scene(Theme::Light);
    stroke(&mut a);
    check("style_panel_light", &image(&mut a, Color::WHITE));
}

#[test]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "test pixel coordinates are small and positive"
)]
fn the_panel_ends_below_the_width_buttons_with_no_opacity_slider() {
    let mut a = scene(Theme::Light);
    let mut overlay = Pixmap::new(W, H).expect("size is non-zero");
    a.render(&mut overlay.as_mut(), Format::Rgba);
    let button = a
        .panel_center(Control::Width(1))
        .expect("the panel is shown");
    // Where the slider track used to be: now only the shadow reaches here.
    let (x, y) = (button.x as u32, button.y as u32 + 40);
    let alpha = overlay.data()[((y * W + x) * 4 + 3) as usize];
    assert!(
        alpha < 128,
        "alpha {alpha}: the panel still reaches this low"
    );
}

#[test]
fn style_panel_dark_showing_a_restyled_selection() {
    let mut a = scene(Theme::Dark);
    stroke(&mut a);
    key(&mut a, 'v');
    a.handle(Event::PointerDown(p(330.0, 150.0)));
    a.handle(Event::PointerUp(p(330.0, 150.0)));
    pick(&mut a, Control::Color(3));
    pick(&mut a, Control::Width(2));
    // The pointer rests on a swatch: its hover ring shows.
    let hover = a.panel_center(Control::Color(2)).expect("panel");
    a.handle(Event::PointerMove(hover));
    check(
        "style_panel_dark_selection",
        &image(&mut a, Color::from_rgba8(0x1e, 0x1e, 0x1e, 255)),
    );
}
