//! Seam 1, pixels out: every stroked digit 0-9 as the style panel's opacity
//! label prints it (tens place gives 1-9, the units place gives 0). The sheet
//! is one crop per opacity value at 4x so a malformed glyph (a box for "0")
//! is visible when the golden is looked at.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test pixel coordinates are small"
)]

use markuli_core::{Annotator, Control, DisplayId, Event, Format, Point, Theme};
use std::path::PathBuf;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

const SCALE: f32 = 4.0;
const W: u32 = 3200;
const H: u32 = 1100;
const CROP_W: u32 = 140;
const CROP_H: u32 = 70;

fn label_crop(value: u8) -> Pixmap {
    let mut a = Annotator::new();
    a.handle(Event::Theme(Theme::Light));
    a.handle(Event::ScaleFactor(SCALE));
    a.handle(Event::Resize {
        width: W,
        height: H,
    });
    a.handle(Event::ToggleDrawMode(DisplayId::new(1)));
    let thumb = a
        .panel_center(Control::Opacity(value))
        .expect("the panel is shown");
    a.handle(Event::PointerDown(thumb));
    a.handle(Event::PointerUp(thumb));
    // Park the pointer away so no hover ring is painted.
    a.handle(Event::PointerMove(Point { x: 1500.0, y: 900.0 }));
    let mut overlay = Pixmap::new(W, H).expect("size is non-zero");
    a.render(&mut overlay.as_mut(), Format::Rgba);
    let (x, y) = (thumb.x as i32 - 70, thumb.y as i32 + 20);
    let mut crop = Pixmap::new(CROP_W, CROP_H).expect("size is non-zero");
    crop.fill(Color::WHITE);
    crop.draw_pixmap(
        -x,
        -y,
        overlay.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    crop
}

#[test]
fn opacity_label_prints_every_digit() {
    let values = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
    let mut sheet = Pixmap::new(CROP_W * 5, CROP_H * 2).expect("size is non-zero");
    for (i, v) in values.into_iter().enumerate() {
        let (col, row) = (i % 5, i / 5);
        sheet.draw_pixmap(
            i32::try_from(u32::try_from(col).expect("small") * CROP_W).expect("small"),
            i32::try_from(u32::try_from(row).expect("small") * CROP_H).expect("small"),
            label_crop(v).as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/digits.png");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        sheet.save_png(&path).expect("write golden");
        return;
    }
    let golden = Pixmap::load_png(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {} ({e}); run with UPDATE_GOLDEN=1",
            path.display()
        )
    });
    assert_eq!((golden.width(), golden.height()), (sheet.width(), sheet.height()));
    let worst = golden
        .data()
        .iter()
        .zip(sheet.data())
        .map(|(g, a)| g.abs_diff(*a))
        .max()
        .unwrap_or(0);
    assert!(worst <= 2, "digits differ from the golden by {worst}");
}
