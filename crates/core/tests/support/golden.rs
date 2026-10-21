//! Golden-image helpers shared by the golden test binaries.

#![allow(dead_code, reason = "each test binary uses a subset")]

use std::path::PathBuf;
use tiny_skia::{Color, Pixmap, PixmapPaint, Transform};

pub const WHITE_PAGE: Color = Color::WHITE;

pub fn dark_page() -> Color {
    Color::from_rgba8(0x1e, 0x1e, 0x1e, 255)
}

/// The Overlay buffer composited over the page.
pub fn over(overlay: &Pixmap, page: Color) -> Pixmap {
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
pub fn check(name: &str, expected: (u32, u32), actual: &Pixmap) {
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
    // An edge that falls exactly on an anti-aliasing sample row can also land
    // on either side of it per platform (ADR-0003; seen on the Hand icon's
    // fingertip, macOS vs Windows): a few pixels may be off by up to three of
    // the 16 coverage samples.
    let width = expected.0 as usize;
    let off = |limit: u8| {
        golden
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(actual.data().as_chunks::<4>().0)
            .enumerate()
            .filter(move |(_, (g, a))| g.iter().zip(*a).any(|(g, a)| g.abs_diff(*a) > limit))
            .map(move |(i, (g, a))| format!("({}, {}) {g:?} -> {a:?}", i % width, i / width))
    };
    let (rounding, samples) = (off(2).count(), off(48).count());
    assert!(
        rounding <= 8 && samples == 0,
        "{name}: {rounding} pixels differ from the golden, {samples} by more than 48: {:#?}",
        off(2).take(40).collect::<Vec<_>>()
    );
}
