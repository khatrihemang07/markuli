//! Seam 1, pixels out: the Color Picker and Width Picker against golden
//! images. Regenerate after an intended change with `UPDATE_GOLDEN=1 cargo
//! test` and look at the PNGs in `tests/golden/` before committing them.

mod support;

use markuli_core::{
    Annotator, Button, Event, Format, PickerPart, RecentColors, Theme, ToolbarPosition,
};
use support::golden::{check, dark_page, WHITE_PAGE};
use support::picker::{click_part, drag, open};
use tiny_skia::{Color, Pixmap};

const W: u32 = 640;
const H: u32 = 360;
const TALL: u32 = 700;

fn scene(theme: Theme, scale: f32, position: ToolbarPosition, (w, h): (u32, u32)) -> Annotator {
    let mut a = Annotator::new();
    a.handle(Event::Theme(theme));
    a.handle(Event::ScaleFactor(scale));
    a.handle(Event::Resize {
        width: w,
        height: h,
    });
    a.handle(Event::ToolbarPosition(position));
    a.handle(Event::ToggleDrawMode(markuli_core::DisplayId::new(1)));
    a
}

fn top(theme: Theme) -> Annotator {
    scene(theme, 1.0, ToolbarPosition::Top, (W, H))
}

fn image(a: &mut Annotator, page: Color, (w, h): (u32, u32)) -> Pixmap {
    let mut overlay = Pixmap::new(w, h).expect("size is non-zero");
    a.render(&mut overlay.as_mut(), Format::Rgba);
    support::golden::over(&overlay, page)
}

fn recent() -> Event {
    Event::RecentColors(RecentColors::new(&[
        [0x12, 0xb8, 0x86],
        [0xf7, 0x83, 0xac],
        [0x4d, 0xab, 0xf7],
    ]))
}

/// A Palette with slot `slot` set to `rgb`.
fn with_color(a: &mut Annotator, slot: usize, rgb: [u8; 3]) {
    let mut palette = a.view().palette;
    palette.colors[slot] = rgb;
    a.handle(Event::Palette(palette));
}

#[test]
fn color_picker_presets_light() {
    let mut a = top(Theme::Light);
    open(&mut a, Button::Color(2));
    check(
        "color_picker_presets_light",
        (W, H),
        &image(&mut a, WHITE_PAGE, (W, H)),
    );
}

#[test]
fn color_picker_presets_dark_with_recent_colors_and_a_hovered_cell() {
    let mut a = top(Theme::Dark);
    a.handle(recent());
    open(&mut a, Button::Color(3));
    let at = support::picker::part(&a, PickerPart::Preset(10));
    a.handle(Event::PointerMove(at));
    check(
        "color_picker_presets_dark_recent_hover",
        (W, H),
        &image(&mut a, dark_page(), (W, H)),
    );
}

#[test]
fn color_picker_custom_tab_light() {
    let mut a = top(Theme::Light);
    open(&mut a, Button::Color(3));
    click_part(&mut a, PickerPart::Tab(1));
    check(
        "color_picker_custom_light",
        (W, H),
        &image(&mut a, WHITE_PAGE, (W, H)),
    );
}

#[test]
fn color_picker_custom_tab_dark_mid_drag() {
    let mut a = top(Theme::Dark);
    open(&mut a, Button::Color(4));
    click_part(&mut a, PickerPart::Tab(1));
    a.handle(Event::PointerDown(support::picker::part(
        &a,
        PickerPart::Hue(0.8),
    )));
    a.handle(Event::PointerMove(support::picker::part(
        &a,
        PickerPart::Hue(0.6),
    )));
    a.handle(Event::PointerUp(support::picker::part(
        &a,
        PickerPart::Hue(0.6),
    )));
    open(&mut a, Button::Color(4));
    click_part(&mut a, PickerPart::Tab(1));
    a.handle(Event::PointerDown(support::picker::part(
        &a,
        PickerPart::Square(0.7, 0.3),
    )));
    check(
        "color_picker_custom_dark",
        (W, H),
        &image(&mut a, dark_page(), (W, H)),
    );
}

#[test]
fn a_color_that_is_hard_to_see_on_a_white_page_is_marked_light() {
    // Amber is the faint default: 2.1 to 1 on white.
    let mut a = top(Theme::Light);
    open(&mut a, Button::Color(1));
    check(
        "color_picker_low_contrast_light",
        (W, H),
        &image(&mut a, WHITE_PAGE, (W, H)),
    );
}

#[test]
fn a_color_that_is_hard_to_see_on_a_dark_page_is_marked_dark() {
    let mut a = top(Theme::Dark);
    with_color(&mut a, 4, [0x1e, 0x1e, 0x1e]);
    open(&mut a, Button::Color(4));
    check(
        "color_picker_low_contrast_dark",
        (W, H),
        &image(&mut a, dark_page(), (W, H)),
    );
}

#[test]
fn color_picker_above_a_bottom_toolbar() {
    let mut a = scene(Theme::Light, 1.0, ToolbarPosition::Bottom, (W, H));
    a.handle(recent());
    open(&mut a, Button::Color(2));
    check(
        "color_picker_bottom_light",
        (W, H),
        &image(&mut a, WHITE_PAGE, (W, H)),
    );
}

#[test]
fn color_picker_beside_a_left_column_dark() {
    let mut a = scene(Theme::Dark, 1.0, ToolbarPosition::Left, (W, TALL));
    open(&mut a, Button::Color(2));
    check(
        "color_picker_left_dark",
        (W, TALL),
        &image(&mut a, dark_page(), (W, TALL)),
    );
}

#[test]
fn color_picker_beside_a_right_column_light() {
    let mut a = scene(Theme::Light, 1.0, ToolbarPosition::Right, (W, TALL));
    a.handle(recent());
    open(&mut a, Button::Color(0));
    check(
        "color_picker_right_light",
        (W, TALL),
        &image(&mut a, WHITE_PAGE, (W, TALL)),
    );
}

#[test]
fn color_picker_is_crisp_at_scale_two() {
    let size = (1280, 720);
    let mut a = scene(Theme::Light, 2.0, ToolbarPosition::Top, size);
    a.handle(recent());
    open(&mut a, Button::Color(2));
    click_part(&mut a, PickerPart::Tab(1));
    let at = support::picker::part(&a, PickerPart::More);
    a.handle(Event::PointerMove(at));
    check(
        "color_picker_scale_2",
        size,
        &image(&mut a, WHITE_PAGE, size),
    );
}

#[test]
fn color_picker_is_clamped_at_the_edge_of_a_narrow_overlay() {
    let size = (300, 330);
    let mut a = scene(Theme::Light, 1.0, ToolbarPosition::Top, size);
    open(&mut a, Button::Color(4));
    check(
        "color_picker_clamped_edge",
        size,
        &image(&mut a, WHITE_PAGE, size),
    );
}

#[test]
fn width_picker_light() {
    let mut a = top(Theme::Light);
    open(&mut a, Button::Width(1));
    check(
        "width_picker_light",
        (W, H),
        &image(&mut a, WHITE_PAGE, (W, H)),
    );
}

#[test]
fn width_picker_dark_at_thirty_points() {
    let mut a = top(Theme::Dark);
    open(&mut a, Button::Width(2));
    drag(&mut a, &[PickerPart::Track(1.0)]);
    open(&mut a, Button::Width(2));
    let at = support::picker::part(&a, PickerPart::Reset);
    a.handle(Event::PointerMove(at));
    check(
        "width_picker_dark_30pt",
        (W, H),
        &image(&mut a, dark_page(), (W, H)),
    );
    assert!((a.view().palette.widths[2] - 30.0).abs() < f32::EPSILON);
}

#[test]
fn width_picker_beside_a_left_column() {
    let mut a = scene(Theme::Light, 1.0, ToolbarPosition::Left, (W, TALL));
    open(&mut a, Button::Width(0));
    check(
        "width_picker_left_light",
        (W, TALL),
        &image(&mut a, WHITE_PAGE, (W, TALL)),
    );
}
