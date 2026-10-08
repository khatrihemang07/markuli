//! The pictures of the drawing cursors.
//!
//! The core says which cursor to show and how big (`Cursor`, logical px); this
//! module draws it at the display's scale, so rings stay crisp on a 2x display.
//! It is rebuilt only when the cursor or the scale changes, never per pointer
//! move. The hotspot is the middle of the (odd-sized, square) image.

use markuli_core::Cursor;
use tiny_skia::{
    Color, FillRule, GradientStop, Paint, PathBuilder, Pixmap, Point, RadialGradient, SpreadMode,
    Stroke, Transform,
};

/// A cursor picture: premultiplied RGBA, `size` x `size` pixels, the hotspot
/// at its centre pixel.
pub struct Image {
    pub rgba: Vec<u8>,
    pub size: u32,
}

impl Image {
    pub fn hotspot(&self) -> u32 {
        self.size / 2
    }
}

/// Line thickness of the rings, logical px.
const LINE: f32 = 1.5;
/// Contrast outline on each side of a line, logical px.
const OUTLINE: f32 = 1.0;
/// Laser glow radius and dot radius, logical px.
const GLOW: f32 = 9.0;
const DOT: f32 = 3.0;

/// The picture for `cursor` at `scale` physical px per logical px; `None` for
/// the system arrow.
pub fn render(cursor: Cursor, scale: f32) -> Option<Image> {
    let s = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    match cursor {
        Cursor::Arrow => None,
        Cursor::Pen { diameter, color } => {
            let radius = f32::from(diameter) * s / 2.0;
            // The ring's outer edge is the stroke's edge.
            let ring = radius - LINE * s / 2.0;
            let outline = contrast(color);
            let mut canvas = Canvas::new(radius + OUTLINE * s);
            canvas.ring(ring, LINE * s, OUTLINE * s, color, 1.0, outline);
            canvas.dot(1.25 * s, OUTLINE * s, color, outline);
            Some(canvas.finish())
        }
        Cursor::Eraser { diameter } => {
            let radius = f32::from(diameter) * s / 2.0;
            let mut canvas = Canvas::new(radius + LINE * s / 2.0 + OUTLINE * s);
            canvas.ring(radius, LINE * s, OUTLINE * s, [255; 3], 1.0, [0; 3]);
            Some(canvas.finish())
        }
        Cursor::Laser => {
            let mut canvas = Canvas::new(GLOW * s);
            canvas.glow(GLOW * s, [255, 32, 32]);
            canvas.dot(DOT * s, 0.0, [255, 32, 32], [255; 3]);
            Some(canvas.finish())
        }
    }
}

/// White outline for dark colors, black for light ones.
fn contrast([r, g, b]: [u8; 3]) -> [u8; 3] {
    let luma = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    if luma < 140.0 {
        [255; 3]
    } else {
        [0; 3]
    }
}

struct Canvas {
    pixmap: Pixmap,
    centre: f32,
}

impl Canvas {
    /// A square canvas whose centre is the middle of the centre pixel, big
    /// enough for `extent` px around it.
    fn new(extent: f32) -> Self {
        let half = extent.ceil().clamp(4.0, 120.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "4..=120"
        )]
        let size = 2 * (half as u32 + 1) + 1;
        #[allow(clippy::cast_precision_loss, reason = "at most 243")]
        let centre = size as f32 / 2.0;
        Self {
            // A size of at least 9 is never zero.
            pixmap: Pixmap::new(size, size).unwrap_or_else(|| Pixmap::new(9, 9).expect("9x9")),
            centre,
        }
    }

    fn paint(rgb: [u8; 3], alpha: f32) -> Paint<'static> {
        let mut paint = Paint::default();
        let mut color = Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255);
        color.apply_opacity(alpha);
        paint.set_color(color);
        paint.anti_alias = true;
        paint
    }

    /// A ring of centre-line `radius` and `width`, with `outline` px of
    /// `outline_color` on both sides.
    fn ring(
        &mut self,
        radius: f32,
        width: f32,
        outline: f32,
        color: [u8; 3],
        alpha: f32,
        outline_color: [u8; 3],
    ) {
        let Some(path) = PathBuilder::from_circle(self.centre, self.centre, radius) else {
            return;
        };
        let line = |w| Stroke {
            width: w,
            ..Stroke::default()
        };
        self.pixmap.stroke_path(
            &path,
            &Self::paint(outline_color, 0.9),
            &line(width + 2.0 * outline),
            Transform::identity(),
            None,
        );
        self.pixmap.stroke_path(
            &path,
            &Self::paint(color, alpha),
            &line(width),
            Transform::identity(),
            None,
        );
    }

    fn dot(&mut self, radius: f32, outline: f32, color: [u8; 3], outline_color: [u8; 3]) {
        for (r, rgb) in [(radius + outline, outline_color), (radius, color)] {
            if r <= 0.0 {
                continue;
            }
            if let Some(path) = PathBuilder::from_circle(self.centre, self.centre, r) {
                let alpha = if rgb == color { 1.0 } else { 0.9 };
                self.pixmap.fill_path(
                    &path,
                    &Self::paint(rgb, alpha),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    /// A soft glow fading from `color` at the middle to nothing at `radius`.
    fn glow(&mut self, radius: f32, [r, g, b]: [u8; 3]) {
        let at = Point::from_xy(self.centre, self.centre);
        let stop = |t, a| GradientStop::new(t, Color::from_rgba8(r, g, b, a));
        let shader = RadialGradient::new(
            at,
            0.0,
            at,
            radius,
            vec![stop(0.0, 200), stop(0.35, 110), stop(0.7, 35), stop(1.0, 0)],
            SpreadMode::Pad,
            Transform::identity(),
        );
        let (Some(shader), Some(path)) = (
            shader,
            PathBuilder::from_circle(self.centre, self.centre, radius),
        ) else {
            return;
        };
        let paint = Paint {
            shader,
            anti_alias: true,
            ..Paint::default()
        };
        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    fn finish(self) -> Image {
        Image {
            size: self.pixmap.width(),
            rgba: self.pixmap.take(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const RED: [u8; 3] = [0xe0, 0x31, 0x31];
    const BLUE: [u8; 3] = [0x19, 0x71, 0xc2];
    const BLACK: [u8; 3] = [0x1e, 0x1e, 0x1e];

    fn golden(name: &str, cursor: Cursor, scale: f32) {
        let image = render(cursor, scale).expect("a drawing cursor has a picture");
        let mut pixmap = Pixmap::new(image.size, image.size).expect("size");
        pixmap.data_mut().copy_from_slice(&image.rgba);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(format!("{name}.png"));
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            pixmap.save_png(&path).expect("write golden");
            return;
        }
        let golden = Pixmap::load_png(&path)
            .unwrap_or_else(|e| panic!("missing golden {} ({e}); UPDATE_GOLDEN=1", path.display()));
        assert_eq!(
            (golden.width(), golden.height()),
            (pixmap.width(), pixmap.height())
        );
        let worst = golden
            .data()
            .iter()
            .zip(pixmap.data())
            .map(|(g, a)| g.abs_diff(*a))
            .max()
            .unwrap_or(0);
        assert!(worst <= 2, "{name} differs from its golden by {worst}");
    }

    #[test]
    fn cursor_pictures_match_their_goldens() {
        let pen = |diameter, color| Cursor::Pen { diameter, color };
        golden("cursor_pen_medium_red_2x", pen(9, RED), 2.0);
        golden("cursor_pen_thin_blue_2x", pen(6, BLUE), 2.0);
        golden("cursor_pen_bold_black_2x", pen(17, BLACK), 2.0);
        golden("cursor_pen_medium_red_1x", pen(9, RED), 1.0);
        golden("cursor_eraser_2x", Cursor::Eraser { diameter: 16 }, 2.0);
        golden("cursor_laser_2x", Cursor::Laser, 2.0);
    }

    #[test]
    fn the_arrow_is_the_system_cursor_and_every_picture_is_centred_on_its_hotspot() {
        assert!(render(Cursor::Arrow, 2.0).is_none());
        for cursor in [
            Cursor::Pen {
                diameter: 9,
                color: RED,
            },
            Cursor::Eraser { diameter: 16 },
            Cursor::Laser,
        ] {
            let image = render(cursor, 2.0).expect("picture");
            assert_eq!(image.size % 2, 1, "odd size: a centre pixel exists");
            assert_eq!(image.hotspot() * 2 + 1, image.size);
        }
    }

    #[test]
    fn the_pen_ring_is_as_wide_as_the_stroke() {
        // 17 logical px at 2x: the ring reaches 17 px from the centre, its outline 2 more.
        let image = render(
            Cursor::Pen {
                diameter: 17,
                color: BLACK,
            },
            2.0,
        )
        .expect("picture");
        let row = image.hotspot() as usize;
        let at = |x: usize| image.rgba[(row * image.size as usize + x) * 4 + 3];
        let centre = image.hotspot() as usize;
        assert_eq!(at(centre + 20), 0, "nothing beyond the outline");
        assert!(at(centre + 16) > 0, "the ring reaches the stroke's edge");
    }
}
