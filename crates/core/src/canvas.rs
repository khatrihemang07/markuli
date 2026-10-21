//! The drawing surface of one band: target-pixel coordinates over a reused
//! scratch pixmap, and one path builder shared by every shape of the frame.

use tiny_skia::{
    FillRule, FilterQuality, Paint, Path, PathBuilder, Pattern, PixmapMut, PixmapRef,
    Rect as SkiaRect, SpreadMode, Stroke, Transform,
};

/// A target seen from a band: drawing code works in target pixel coordinates,
/// the canvas shifts them into the band's own pixmap.
pub(crate) struct Canvas<'a, 'b> {
    pm: &'a mut PixmapMut<'b>,
    shift: Transform,
    /// One path builder for every `*_with` shape of the frame.
    builder: &'a mut Option<PathBuilder>,
}

impl<'a, 'b> Canvas<'a, 'b> {
    /// `pm`'s top-left pixel is the target pixel `(x, y)`.
    pub(crate) fn new(
        pm: &'a mut PixmapMut<'b>,
        (x, y): (u32, u32),
        builder: &'a mut Option<PathBuilder>,
    ) -> Self {
        Self {
            pm,
            shift: Transform::from_translate(-to_f32(x), -to_f32(y)),
            builder,
        }
    }

    /// Fills the shape `build` draws. The path is built in the frame's
    /// reused builder, so a shape costs no allocation once warm (rule 8).
    pub fn fill_with(&mut self, build: impl FnOnce(&mut PathBuilder), paint: &Paint<'_>) {
        let mut path = self.builder.take().unwrap_or_default();
        build(&mut path);
        if let Some(path) = path.finish() {
            self.pm
                .fill_path(&path, paint, FillRule::Winding, self.shift, None);
            *self.builder = Some(path.clear());
        }
    }

    pub fn stroke_with(
        &mut self,
        build: impl FnOnce(&mut PathBuilder),
        paint: &Paint<'_>,
        stroke: &Stroke,
    ) {
        let mut path = self.builder.take().unwrap_or_default();
        build(&mut path);
        if let Some(path) = path.finish() {
            self.pm.stroke_path(&path, paint, stroke, self.shift, None);
            *self.builder = Some(path.clear());
        }
    }

    /// Paints `pixels` (target pixel order) with their corner at `origin`,
    /// only inside the shape `build` draws: a picture with rounded corners
    /// and no mask to allocate.
    pub fn draw_pixmap(
        &mut self,
        build: impl FnOnce(&mut PathBuilder),
        pixels: PixmapRef<'_>,
        origin: (f32, f32),
    ) {
        let paint = Paint {
            shader: Pattern::new(
                pixels,
                SpreadMode::Pad,
                FilterQuality::Nearest,
                1.0,
                Transform::from_translate(origin.0, origin.1),
            ),
            anti_alias: true,
            ..Paint::default()
        };
        self.fill_with(build, &paint);
    }

    /// `at` maps the path into target pixels first.
    pub fn stroke_path(&mut self, path: &Path, paint: &Paint<'_>, stroke: &Stroke, at: Transform) {
        let to_band = at.post_concat(self.shift);
        self.pm.stroke_path(path, paint, stroke, to_band, None);
    }

    pub fn fill_rect(&mut self, rect: SkiaRect, paint: &Paint<'_>) {
        self.pm.fill_rect(rect, paint, self.shift, None);
    }
}

#[allow(clippy::cast_precision_loss, reason = "pixel sizes are far below 2^24")]
fn to_f32(v: u32) -> f32 {
    v as f32
}
