//! The Custom tab's two pictures: the saturation/value square for the
//! current hue, and the hue strip. They are premultiplied pixel buffers in the
//! target's channel order, refilled only when the hue, the size or the order
//! changes (never while only the knob moves), allocated at the first open and
//! freed when Draw Mode ends.

use super::color::hsv_to_rgb;
use crate::render::Format;
use tiny_skia::PixmapRef;

#[derive(Debug, Default)]
struct Picture {
    bytes: Vec<u8>,
    size: (u32, u32),
    /// What the pixels were filled for; a different key means a refill.
    key: Option<(u32, Format)>,
}

impl Picture {
    /// Makes the buffer `size` pixels and tells whether it needs filling for
    /// `key`. The allocation only happens when the size changes.
    fn stale(&mut self, size: (u32, u32), key: (u32, Format)) -> bool {
        if self.size != size {
            self.size = size;
            self.bytes.clear();
            self.bytes.resize(len(size), 0);
            self.key = None;
        }
        let stale = self.key != Some(key);
        self.key = Some(key);
        stale
    }

    fn pixmap(&self) -> Option<PixmapRef<'_>> {
        PixmapRef::from_bytes(&self.bytes, self.size.0, self.size.1)
    }
}

fn len((w, h): (u32, u32)) -> usize {
    w as usize * h as usize * 4
}

#[derive(Debug, Default)]
pub(crate) struct Custom {
    square: Picture,
    strip: Picture,
}

/// The pixel at channel order `format`, opaque.
fn pixel([r, g, b]: [u8; 3], format: Format) -> [u8; 4] {
    match format {
        Format::Rgba => [r, g, b, 255],
        Format::Bgra => [b, g, r, 255],
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "pixel indices and 0..=255 channels"
)]
impl Custom {
    /// Fills the square (`size` pixels) for `hue`: saturation grows to the
    /// right, value falls downwards. Per pixel `v * (1 - s + s * pure)`, which
    /// is the HSV conversion for a fixed hue without its branches.
    pub fn square(&mut self, size: (u32, u32), hue: f32, format: Format) -> Option<PixmapRef<'_>> {
        if self.square.stale(size, (hue.to_bits(), format)) {
            let pure = hsv_to_rgb(hue, 1.0, 1.0).map(|c| f32::from(c) / 255.0);
            let (w, h) = (size.0 as usize, size.1 as usize);
            for (y, row) in self.square.bytes.chunks_exact_mut(w * 4).enumerate() {
                let v = 1.0 - (y as f32 + 0.5) / h as f32;
                for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let s = (x as f32 + 0.5) / w as f32;
                    let rgb = pure.map(|c| (v * (1.0 - s + s * c) * 255.0).round() as u8);
                    px.copy_from_slice(&pixel(rgb, format));
                }
            }
        }
        self.square.pixmap()
    }

    /// Fills the strip: the hue runs from 0 to 360 over its width, at full
    /// saturation and value. It depends on nothing but its size.
    pub fn strip(&mut self, size: (u32, u32), format: Format) -> Option<PixmapRef<'_>> {
        if self.strip.stale(size, (0, format)) {
            let w = size.0 as usize;
            if let Some((first, rest)) = self.strip.bytes.split_at_mut_checked(w * 4) {
                for (x, px) in first.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let hue = (x as f32 + 0.5) / w as f32 * 360.0;
                    px.copy_from_slice(&pixel(hsv_to_rgb(hue, 1.0, 1.0), format));
                }
                for row in rest.chunks_exact_mut(w * 4) {
                    row.copy_from_slice(first);
                }
            }
        }
        self.strip.pixmap()
    }
}
