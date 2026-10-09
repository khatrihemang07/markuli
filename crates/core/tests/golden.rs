//! Seam 1, pixels out: the toolbar against golden images.
//!
//! Each scene is composited over a page color so the PNGs are easy to look
//! at. Regenerate after an intended change with `UPDATE_GOLDEN=1 cargo test`
//! and look at the PNGs in `tests/golden/` before committing them.

use markuli_core::{
    Annotator, Button, DisplayId, Event, Format, Key, Point, Theme, ToolbarPosition,
};
use std::path::PathBuf;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

/// Wide enough for the one-row Toolbar (about 580 logical px).
const W: u32 = 640;
const H: u32 = 150;

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// Draw Mode on a `W` x `H` Overlay at `scale`, in `theme`.
fn scene(theme: Theme, scale: f32) -> Annotator {
    scene_sized(theme, scale, (W, H))
}

/// Draw Mode on a physical `size` Overlay at `scale`, in `theme`.
fn scene_sized(theme: Theme, scale: f32, (width, height): (u32, u32)) -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Theme(theme));
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::Resize { width, height });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
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
    let mut out = Pixmap::new(overlay.width(), overlay.height()).expect("size is non-zero");
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

/// Compares with the golden `name`, which must be exactly `expected` pixels:
/// the size is asserted here, so a change of it fails the test.
fn check(name: &str, expected: (u32, u32), actual: &Pixmap) {
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
    assert_eq!(
        (actual.width(), actual.height()),
        expected,
        "{name}: rendered size"
    );
    assert_eq!(
        (golden.width(), golden.height()),
        expected,
        "{name}: golden size"
    );
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
    let mut a = scene(Theme::Light, 1.0);
    check("toolbar_light", (W, H), &image(&mut a, WHITE_PAGE));
}

#[test]
fn toolbar_dark_theme() {
    let mut a = scene(Theme::Dark, 1.0);
    check("toolbar_dark", (W, H), &image(&mut a, dark_page()));
}

#[test]
fn toolbar_with_ink_enables_undo_and_clear_and_highlights_hover() {
    let mut a = scene(Theme::Light, 1.0);
    stroke(&mut a);
    let undo = a.button_center(Button::Undo).expect("toolbar visible");
    a.handle(Event::PointerMove(undo));
    check(
        "toolbar_light_ink_hover_undo",
        (W, H),
        &image(&mut a, WHITE_PAGE),
    );
}

#[test]
fn toolbar_after_undo_enables_redo_in_dark_theme() {
    let mut a = scene(Theme::Dark, 1.0);
    stroke(&mut a);
    let undo = a.button_center(Button::Undo).expect("toolbar visible");
    a.handle(Event::PointerDown(undo));
    a.handle(Event::PointerUp(undo));
    a.handle(Event::PointerMove(p(300.0, 100.0)));
    check(
        "toolbar_dark_after_undo",
        (W, H),
        &image(&mut a, dark_page()),
    );
}

fn key(a: &mut Annotator, c: char) {
    a.handle(Event::Key {
        key: Key::Char(c),
        command: false,
        shift: false,
        alt: false,
    });
}

/// Two strokes drawn, then one selected with a click.
fn one_selected(theme: Theme, scale: f32) -> Annotator {
    let mut a = scene(theme, scale);
    a.handle(Event::PointerDown(p(30.0, 70.0)));
    a.handle(Event::PointerMove(p(80.0, 110.0)));
    a.handle(Event::PointerUp(p(130.0, 80.0)));
    a.handle(Event::PointerDown(p(200.0, 120.0)));
    a.handle(Event::PointerMove(p(250.0, 90.0)));
    a.handle(Event::PointerUp(p(320.0, 130.0)));
    key(&mut a, 'v');
    a.handle(Event::PointerDown(p(80.0, 110.0)));
    a.handle(Event::PointerUp(p(80.0, 110.0)));
    a
}

#[test]
fn selection_of_one_element_light() {
    let a = &mut one_selected(Theme::Light, 1.0);
    check("selection_one_light", (W, H), &image(a, WHITE_PAGE));
}

#[test]
fn selection_of_two_elements_has_a_dashed_common_box() {
    let a = &mut one_selected(Theme::Light, 1.0);
    a.handle(Event::Modifiers { shift: true });
    a.handle(Event::PointerDown(p(250.0, 90.0)));
    a.handle(Event::PointerUp(p(250.0, 90.0)));
    check("selection_two_light", (W, H), &image(a, WHITE_PAGE));
}

#[test]
fn selection_box_while_dragging_dark() {
    let a = &mut one_selected(Theme::Dark, 1.0);
    a.handle(Event::PointerDown(p(10.0, 50.0)));
    a.handle(Event::PointerMove(p(150.0, 140.0)));
    check("selection_box_dark", (W, H), &image(a, dark_page()));
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
    check("laser_fresh", (W, H), &image(&mut a, WHITE_PAGE));
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
    check("laser_mid_fade", (W, H), &over(&overlay, WHITE_PAGE));
}

#[test]
fn eraser_drag_draws_the_marked_stroke_faded() {
    let mut a = scene(Theme::Light, 1.0);
    stroke(&mut a);
    key(&mut a, 'e');
    // Marks the stroke; the drag is still going, so it is drawn faded.
    a.handle(Event::PointerDown(p(120.0, 100.0)));
    a.handle(Event::PointerMove(p(122.0, 101.0)));
    check("eraser_pending", (W, H), &image(&mut a, WHITE_PAGE));
}

#[test]
fn toolbar_highlights_nothing_for_a_selection_of_mixed_values() {
    let mut a = scene(Theme::Light, 1.0);
    // A red medium stroke, then a blue bold one; both selected.
    a.handle(Event::PointerDown(p(30.0, 80.0)));
    a.handle(Event::PointerMove(p(80.0, 110.0)));
    a.handle(Event::PointerUp(p(130.0, 85.0)));
    key(&mut a, '4');
    key(&mut a, ']');
    a.handle(Event::PointerDown(p(200.0, 120.0)));
    a.handle(Event::PointerMove(p(250.0, 90.0)));
    a.handle(Event::PointerUp(p(320.0, 130.0)));
    key(&mut a, 'v');
    a.handle(Event::PointerDown(p(80.0, 110.0)));
    a.handle(Event::PointerUp(p(80.0, 110.0)));
    a.handle(Event::Modifiers { shift: true });
    a.handle(Event::PointerDown(p(250.0, 90.0)));
    a.handle(Event::PointerUp(p(250.0, 90.0)));
    assert_eq!(a.selection().len(), 2);
    check(
        "toolbar_light_mixed_selection",
        (W, H),
        &image(&mut a, WHITE_PAGE),
    );
}

#[test]
fn toolbar_dims_colors_and_widths_for_the_eraser() {
    let mut a = scene(Theme::Dark, 1.0);
    stroke(&mut a);
    key(&mut a, 'e');
    check(
        "toolbar_dark_eraser_dimmed",
        (W, H),
        &image(&mut a, dark_page()),
    );
}

#[test]
fn toolbar_highlights_the_chosen_color_and_width_in_both_themes() {
    for (theme, page, name) in [
        (Theme::Light, WHITE_PAGE, "toolbar_light_blue_bold"),
        (Theme::Dark, dark_page(), "toolbar_dark_blue_bold"),
    ] {
        let mut a = scene(theme, 1.0);
        key(&mut a, '2');
        key(&mut a, ']');
        check(name, (W, H), &image(&mut a, page));
    }
}

/// A tall Overlay for the column positions (the column is about 612 px).
const TALL: u32 = 700;

fn placed(theme: Theme, pos: ToolbarPosition, height: u32) -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Theme(theme));
    a.handle(Event::Resize { width: W, height });
    a.handle(Event::ToolbarPosition(pos));
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    a
}

/// A stroke clear of the toolbar in every position, from `(x, y)`.
fn stroke_at(a: &mut Annotator, x: f32, y: f32) {
    a.handle(Event::PointerDown(p(x, y)));
    a.handle(Event::PointerMove(p(x + 50.0, y - 30.0)));
    a.handle(Event::PointerUp(p(x + 130.0, y + 20.0)));
}

fn image_sized(a: &mut Annotator, page: Color, (width, height): (u32, u32)) -> Pixmap {
    let mut overlay = Pixmap::new(width, height).expect("size is non-zero");
    a.render(&mut overlay.as_mut(), Format::Rgba);
    over(&overlay, page)
}

#[test]
fn toolbar_on_the_left_light() {
    let mut a = placed(Theme::Light, ToolbarPosition::Left, TALL);
    stroke_at(&mut a, 120.0, 300.0);
    check(
        "toolbar_left_light",
        (W, TALL),
        &image_sized(&mut a, WHITE_PAGE, (W, TALL)),
    );
}

#[test]
fn toolbar_on_the_right_dark_with_a_mixed_selection() {
    let mut a = placed(Theme::Dark, ToolbarPosition::Right, TALL);
    stroke_at(&mut a, 100.0, 300.0);
    key(&mut a, '4');
    key(&mut a, ']');
    stroke_at(&mut a, 100.0, 450.0);
    key(&mut a, 'v');
    a.handle(Event::PointerDown(p(150.0, 270.0)));
    a.handle(Event::PointerUp(p(150.0, 270.0)));
    a.handle(Event::Modifiers { shift: true });
    a.handle(Event::PointerDown(p(150.0, 420.0)));
    a.handle(Event::PointerUp(p(150.0, 420.0)));
    assert_eq!(a.selection().len(), 2);
    check(
        "toolbar_right_dark_mixed",
        (W, TALL),
        &image_sized(&mut a, dark_page(), (W, TALL)),
    );
}

#[test]
fn toolbar_at_the_bottom_light_with_the_eraser_dimming_the_style() {
    let mut a = placed(Theme::Light, ToolbarPosition::Bottom, H);
    stroke_at(&mut a, 60.0, 60.0);
    key(&mut a, 'e');
    check(
        "toolbar_bottom_light_eraser",
        (W, H),
        &image_sized(&mut a, WHITE_PAGE, (W, H)),
    );
}

/// Wide enough for the Toolbar at scale 2 (about 1160 physical px).
const RETINA: (u32, u32) = (1280, 300);

#[test]
fn toolbar_at_scale_two_is_crisp_with_a_hovered_button_and_a_chosen_style() {
    let mut a = scene_sized(Theme::Light, 2.0, RETINA);
    key(&mut a, '4');
    key(&mut a, ']');
    a.handle(Event::PointerDown(p(100.0, 220.0)));
    a.handle(Event::PointerMove(p(300.0, 180.0)));
    a.handle(Event::PointerUp(p(500.0, 230.0)));
    let undo = a.button_center(Button::Undo).expect("toolbar visible");
    a.handle(Event::PointerMove(undo));
    check(
        "toolbar_light_scale_2",
        RETINA,
        &image_sized(&mut a, WHITE_PAGE, RETINA),
    );
}

#[test]
fn toolbar_shows_a_custom_palette_with_a_capped_width_icon() {
    let mut a = scene_sized(Theme::Light, 2.0, RETINA);
    let palette = markuli_core::Palette {
        colors: [
            [0x12, 0xb8, 0xa6],
            [0xff, 0x66, 0xcc],
            [0x7c, 0x3a, 0xed],
            [0xff, 0xff, 0xff],
            [0x00, 0x00, 0x00],
        ],
        widths: [0.5, 8.0, 12.0],
    };
    a.handle(Event::Palette(palette));
    a.handle(Event::EditColor {
        slot: 2,
        rgb: [0x7c, 0x3a, 0xed],
    });
    a.handle(Event::EditWidth {
        slot: 2,
        width: 30.0,
    });
    a.handle(Event::PointerDown(p(100.0, 220.0)));
    a.handle(Event::PointerMove(p(300.0, 180.0)));
    a.handle(Event::PointerUp(p(500.0, 230.0)));
    check(
        "toolbar_light_custom_palette",
        RETINA,
        &image_sized(&mut a, WHITE_PAGE, RETINA),
    );
}
