//! The pictures of the drawing cursors.
//!
//! The core says which cursor to show (`Cursor`: kind, brush diameter, color
//! in logical px); this module draws it at the display's scale, so it stays
//! crisp on a 2x display. It is rebuilt only when the cursor or the scale
//! changes, never per pointer move. The designs are the ones picked in the
//! pointer lab: Pen "Marker nib" (P10), Eraser "Eraser block" (E3), Laser
//! "Ring + dot" (L2). Each picture has its own hotspot pixel.

use markuli_core::Cursor;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// A cursor picture: premultiplied RGBA, `size` x `size` pixels, with the
/// hotspot at pixel (`hot_x`, `hot_y`); the drawing point is that pixel's centre.
pub struct Image {
    pub rgba: Vec<u8>,
    pub size: u32,
    pub hot_x: u32,
    pub hot_y: u32,
}

/// Dark outline of the icons, as in the lab.
const INK: [u8; 3] = [0x1e, 0x1e, 0x1e];
/// Light body of the marker.
const BODY: [u8; 3] = [0xf4, 0xf4, 0xf7];
const WHITE: [u8; 3] = [255; 3];
const LASER_RED: [u8; 3] = [0xff, 0x2b, 0x2b];
/// How far the icons reach up and to the right of the hotspot, logical px.
const ICON_REACH: f32 = 25.0;

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
            let d = f32::from(diameter);
            // Room left of and below the hotspot for the dot and its white edge.
            let mut canvas = Canvas::corner((d / 2.0 + 2.0).min(30.0), s);
            // The dot is the stroke's footprint, centred on the drawing point;
            // the marker is drawn over it.
            canvas.disc(d / 2.0 + 1.0, WHITE, 0.95);
            canvas.disc(d / 2.0, color, 1.0);
            canvas.pen(color, nib_half_width(d));
            Some(canvas.finish())
        }
        Cursor::Eraser { .. } => {
            let mut canvas = Canvas::corner(6.0, s);
            canvas.eraser();
            Some(canvas.finish())
        }
        Cursor::Laser => {
            let mut canvas = Canvas::centred(9.0, s);
            canvas.ring(6.0, 1.8, LASER_RED);
            canvas.disc(3.0, WHITE, 0.95);
            canvas.disc(2.0, LASER_RED, 1.0);
            Some(canvas.finish())
        }
    }
}

/// Half the width of the marker's tip: thin (diameter 6), medium (9) and bold
/// (17) strokes get a visibly thin, medium and wide chisel.
fn nib_half_width(diameter: f32) -> f32 {
    (0.9 + (diameter - 6.0) * 0.28).clamp(0.9, 4.0)
}

struct Canvas {
    pixmap: Pixmap,
    /// The hotspot pixel.
    hot: (u32, u32),
    scale: f32,
}

impl Canvas {
    /// Hotspot `pad` logical px from the left and bottom edges, the icon
    /// reaching up and to the right.
    fn corner(pad: f32, scale: f32) -> Self {
        Self::new(pad, ICON_REACH, scale, false)
    }

    /// Hotspot in the middle, `extent` logical px to every edge.
    fn centred(extent: f32, scale: f32) -> Self {
        Self::new(extent, extent, scale, true)
    }

    fn new(before: f32, after: f32, scale: f32, centred: bool) -> Self {
        let to_px = |v: f32| (v * scale).ceil().clamp(1.0, 240.0);
        let (before, after) = (to_px(before), to_px(after));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "at most 481"
        )]
        let (size, hot) = ((before + after + 1.0) as u32, before as u32);
        let hot_y = if centred { hot } else { size - 1 - hot };
        Self {
            // A size of at least 3 is never zero.
            pixmap: Pixmap::new(size, size).unwrap_or_else(|| Pixmap::new(3, 3).expect("3x3")),
            hot: (hot, hot_y),
            scale,
        }
    }

    /// Centre of the hotspot pixel, physical px.
    #[allow(clippy::cast_precision_loss, reason = "at most 481")]
    fn at(&self) -> (f32, f32) {
        (self.hot.0 as f32 + 0.5, self.hot.1 as f32 + 0.5)
    }

    /// Logical px, origin at the hotspot, y down.
    fn plain(&self) -> Transform {
        Transform::from_translate(self.at().0, self.at().1).pre_scale(self.scale, self.scale)
    }

    /// The same, turned 45 degrees so that +x points up and to the right.
    fn tilted(&self) -> Transform {
        self.plain().pre_rotate(-45.0)
    }

    fn paint(rgb: [u8; 3], alpha: f32) -> Paint<'static> {
        let mut paint = Paint::default();
        let mut color = Color::from_rgba8(rgb[0], rgb[1], rgb[2], 255);
        color.apply_opacity(alpha);
        paint.set_color(color);
        paint.anti_alias = true;
        paint
    }

    fn fill(&mut self, path: &tiny_skia::Path, rgb: [u8; 3], alpha: f32, to: Transform) {
        self.pixmap
            .fill_path(path, &Self::paint(rgb, alpha), FillRule::Winding, to, None);
    }

    fn line(
        &mut self,
        path: &tiny_skia::Path,
        rgb: [u8; 3],
        alpha: f32,
        width: f32,
        to: Transform,
    ) {
        let stroke = Stroke {
            width,
            line_join: tiny_skia::LineJoin::Round,
            ..Stroke::default()
        };
        self.pixmap
            .stroke_path(path, &Self::paint(rgb, alpha), &stroke, to, None);
    }

    /// A filled circle of `radius` logical px on the hotspot.
    fn disc(&mut self, radius: f32, rgb: [u8; 3], alpha: f32) {
        if let Some(path) = PathBuilder::from_circle(0.0, 0.0, radius) {
            self.fill(&path, rgb, alpha, self.plain());
        }
    }

    /// A ring of centre-line `radius` and `width` with a white halo 1 px
    /// wider on each side.
    fn ring(&mut self, radius: f32, width: f32, rgb: [u8; 3]) {
        if let Some(path) = PathBuilder::from_circle(0.0, 0.0, radius) {
            let to = self.plain();
            self.line(&path, WHITE, 0.95, width + 2.0, to);
            self.line(&path, rgb, 1.0, width, to);
        }
    }

    /// Fills and outlines a (rounded) rectangle in the tilted frame.
    fn block(&mut self, rect: (f32, f32, f32, f32), radius: f32, rgb: [u8; 3]) {
        if let Some(path) = rounded_rect(rect, radius) {
            let to = self.tilted();
            self.fill(&path, rgb, 1.0, to);
            self.line(&path, INK, 1.0, 1.2, to);
        }
    }

    /// Marker nib: the chisel tip is on the hotspot, in the stroke color. The
    /// body is light with a dark outline, and a white halo around the whole
    /// silhouette keeps that outline visible on dark backgrounds.
    fn pen(&mut self, color: [u8; 3], tip: f32) {
        let mut b = PathBuilder::new();
        b.move_to(0.0, -tip);
        b.line_to(6.0, -4.0);
        b.line_to(6.0, 4.0);
        b.line_to(0.0, tip);
        b.close();
        let nib = b.finish();
        let body = rounded_rect((6.0, -4.5, 16.0, 9.0), 2.0);
        let band = rounded_rect((10.0, -4.5, 3.0, 9.0), 0.0);
        let to = self.tilted();
        // The halo reaches 1 px past the outline's outer edge.
        for path in [&nib, &body].into_iter().flatten() {
            self.line(path, WHITE, 1.0, 1.2 + 2.0, to);
        }
        if let Some(path) = &nib {
            self.fill(path, color, 1.0, to);
            self.line(path, INK, 1.2, 1.2, to);
        }
        if let Some(path) = &body {
            self.fill(path, BODY, 1.0, to);
        }
        if let Some(band) = &band {
            self.fill(band, color, 1.0, to);
        }
        if let Some(path) = &body {
            self.line(path, INK, 1.0, 1.2, to);
        }
    }

    /// Eraser block: its white end's edge touches the hotspot.
    fn eraser(&mut self) {
        self.block((0.0, -6.0, 10.0, 12.0), 2.0, WHITE);
        self.block((10.0, -6.0, 14.0, 12.0), 2.0, [0xf7, 0xa1, 0xad]);
    }

    fn finish(self) -> Image {
        Image {
            size: self.pixmap.width(),
            rgba: self.pixmap.take(),
            hot_x: self.hot.0,
            hot_y: self.hot.1,
        }
    }
}

/// A rectangle (x, y, w, h) with corners of `r`.
#[allow(clippy::many_single_char_names, reason = "rectangle geometry")]
fn rounded_rect((x, y, w, h): (f32, f32, f32, f32), r: f32) -> Option<tiny_skia::Path> {
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.quad_to(x + w, y, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.quad_to(x + w, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.quad_to(x, y + h, x, y + h - r);
    b.line_to(x, y + r);
    b.quad_to(x, y, x + r, y);
    b.close();
    b.finish()
}

#[cfg(test)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "pixel arithmetic in tests"
)]
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
        for (scale, tag) in [(1.0, "1x"), (2.0, "2x")] {
            golden(&format!("cursor_pen_thin_blue_{tag}"), pen(6, BLUE), scale);
            golden(&format!("cursor_pen_medium_red_{tag}"), pen(9, RED), scale);
            golden(
                &format!("cursor_pen_bold_black_{tag}"),
                pen(17, BLACK),
                scale,
            );
            golden(&format!("cursor_pen_bold_red_{tag}"), pen(17, RED), scale);
            golden(
                &format!("cursor_eraser_{tag}"),
                Cursor::Eraser { diameter: 16 },
                scale,
            );
            golden(&format!("cursor_laser_{tag}"), Cursor::Laser, scale);
        }
    }

    fn alpha(image: &Image, x: i64, y: i64) -> u8 {
        let size = i64::from(image.size);
        if x < 0 || y < 0 || x >= size || y >= size {
            return 0;
        }
        image.rgba[usize::try_from((y * size + x) * 4 + 3).expect("index")]
    }

    fn pixel(image: &Image, x: i64, y: i64) -> [u8; 4] {
        let at = usize::try_from((y * i64::from(image.size) + x) * 4).expect("index");
        [
            image.rgba[at],
            image.rgba[at + 1],
            image.rgba[at + 2],
            image.rgba[at + 3],
        ]
    }

    #[test]
    fn the_arrow_is_the_system_cursor_and_every_hotspot_is_inside_its_picture() {
        assert!(render(Cursor::Arrow, 2.0).is_none());
        for scale in [1.0, 2.0] {
            for cursor in [
                Cursor::Pen {
                    diameter: 17,
                    color: RED,
                },
                Cursor::Eraser { diameter: 16 },
                Cursor::Laser,
            ] {
                let image = render(cursor, scale).expect("picture");
                assert!(image.hot_x < image.size && image.hot_y < image.size);
            }
        }
    }

    #[test]
    fn the_pen_dot_is_exactly_the_brush_diameter_centred_on_the_hotspot() {
        // The nib points up and right from the hotspot, so the dot is
        // measured to the left and below it.
        for (diameter, scale) in [(6_u16, 2.0_f32), (9, 2.0), (17, 2.0), (17, 1.0), (9, 1.0)] {
            let image = render(
                Cursor::Pen {
                    diameter,
                    color: BLUE,
                },
                scale,
            )
            .expect("picture");
            let (hx, hy) = (i64::from(image.hot_x), i64::from(image.hot_y));
            let reach = f64::from(f32::from(diameter) * scale) / 2.0;
            let inner = (reach - 0.5).floor() as i64;
            // Up to the brush radius the dot is solid, in the stroke colour.
            for (dx, dy) in [(-inner, 0), (0, inner), (-(inner * 7 / 10), inner * 7 / 10)] {
                let [r, g, b, a] = pixel(&image, hx + dx, hy + dy);
                assert_eq!(a, 255, "{diameter}@{scale}: solid at ({dx},{dy})");
                assert_eq!([r, g, b], BLUE, "{diameter}@{scale}: colour");
            }
            // Just beyond it comes the white edge (at most 1 logical px), then nothing.
            let edge = pixel(
                &image,
                hx - (reach + f64::from(scale) / 2.0).round() as i64,
                hy,
            );
            assert!(
                edge[3] > 0 && edge[0] > 200,
                "{diameter}@{scale}: white edge"
            );
            let past = (reach + f64::from(scale) + 2.0).ceil() as i64;
            assert_eq!(alpha(&image, hx - past, hy), 0, "nothing past the edge");
            assert_eq!(alpha(&image, hx, hy + past), 0, "nothing past the edge");
            // The whole dot and its edge fit in the picture.
            assert!(hx >= past - 1 && i64::from(image.size) - hy >= past, "fits");
        }
    }

    #[test]
    #[allow(clippy::many_single_char_names, reason = "pixel geometry")]
    fn the_pen_nib_widens_with_the_stroke() {
        // Across the chisel just past the tip, the dark outlines of its two
        // sides are further apart the bolder the stroke.
        let span = |diameter| {
            let image = render(
                Cursor::Pen {
                    diameter,
                    color: [0, 255, 0],
                },
                2.0,
            )
            .expect("picture");
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for step in -120..=120 {
                let v = f64::from(step) * 0.05;
                let (x, y) = (
                    f64::from(image.hot_x)
                        + 0.5
                        + (1.0 + v) * std::f64::consts::FRAC_1_SQRT_2 * 2.0,
                    f64::from(image.hot_y)
                        + 0.5
                        + (v - 1.0) * std::f64::consts::FRAC_1_SQRT_2 * 2.0,
                );
                let [r, g, b, a] = pixel(&image, x.floor() as i64, y.floor() as i64);
                if a == 255 && r < 90 && g < 90 && b < 90 {
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
            hi - lo
        };
        let (thin, medium, bold) = (span(6), span(9), span(17));
        assert!(
            thin + 0.5 < medium && medium + 1.0 < bold,
            "{thin} {medium} {bold}"
        );
    }

    #[test]
    fn the_eraser_and_laser_touch_the_hotspot() {
        let eraser = render(Cursor::Eraser { diameter: 16 }, 2.0).expect("picture");
        // The contact edge is on the hotspot, the block lies up and to the right.
        assert!(alpha(&eraser, i64::from(eraser.hot_x), i64::from(eraser.hot_y)) > 0);
        assert_eq!(alpha(&eraser, 0, i64::from(eraser.size) - 1), 0);
        // Within the 8 px reach of the hit-test: the edge's ends are 6 px away.
        let reach = 8.0 * 2.0;
        let mut worst = 0.0_f64;
        for y in 0..i64::from(eraser.size) {
            for x in 0..i64::from(eraser.size) {
                let (dx, dy) = (x - i64::from(eraser.hot_x), y - i64::from(eraser.hot_y));
                if alpha(&eraser, x, y) > 0 && dx <= 0 && dy >= 0 {
                    worst = worst.max(((dx * dx + dy * dy) as f64).sqrt());
                }
            }
        }
        assert!(
            worst <= reach,
            "the back of the contact edge is {worst}px away"
        );
        let laser = render(Cursor::Laser, 2.0).expect("picture");
        let (hx, hy) = (i64::from(laser.hot_x), i64::from(laser.hot_y));
        assert_eq!(pixel(&laser, hx, hy)[0], 255);
        assert_eq!(alpha(&laser, hx - 12, hy), 255, "ring left of the centre");
        assert_eq!(hx * 2 + 1, i64::from(laser.size), "laser is centred");
    }
}
