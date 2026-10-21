//! Color math and the Presets: HSV and RGB, WCAG contrast, and the tables.

#![allow(
    clippy::many_single_char_names,
    reason = "h, s, v and r, g, b are the color channels"
)]

/// Hue in degrees (0..360), saturation and value (0..=1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

impl Hsv {
    /// The color of `rgb`. A gray or black has no hue: `hue` is kept, so
    /// dragging through the square's left or bottom edge does not forget it.
    pub fn of(rgb: [u8; 3], hue: f32) -> Self {
        let [r, g, b] = rgb.map(|c| f32::from(c) / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let delta = max - min;
        let h = if delta <= f32::EPSILON || max <= f32::EPSILON {
            hue
        } else if (max - r).abs() <= f32::EPSILON {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if (max - g).abs() <= f32::EPSILON {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        Self {
            h,
            s: if max <= f32::EPSILON {
                0.0
            } else {
                delta / max
            },
            v: max,
        }
    }

    pub fn rgb(self) -> [u8; 3] {
        hsv_to_rgb(self.h, self.s, self.v)
    }
}

/// The six-sector conversion. `h` wraps; `s` and `v` are clamped.
pub(crate) fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let (s, v) = (s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h {
        h if h < 1.0 => (c, x, 0.0),
        h if h < 2.0 => (x, c, 0.0),
        h if h < 3.0 => (0.0, c, x),
        h if h < 4.0 => (0.0, x, c),
        h if h < 5.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m].map(byte)
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 first"
)]
fn byte(v: f32) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// WCAG relative luminance of an sRGB color.
fn luminance(rgb: [u8; 3]) -> f32 {
    let [r, g, b] = rgb.map(|c| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG contrast ratio, 1 to 21.
pub(crate) fn contrast(a: [u8; 3], b: [u8; 3]) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Below this ratio against the page a color is marked with a warning.
const LOW_CONTRAST: f32 = 3.0;

pub(crate) fn is_low(rgb: [u8; 3], page: [u8; 3]) -> bool {
    contrast(rgb, page) < LOW_CONTRAST
}

/// The Presets grid is this many hues wide and this many tones tall.
pub(crate) const COLUMNS: usize = 9;
pub(crate) const TONES: usize = 4;
pub(crate) const PRESETS: usize = COLUMNS * TONES;

/// The neutral row above the grid: white to black.
pub(crate) const NEUTRALS: [[u8; 3]; COLUMNS] = [
    [0xff, 0xff, 0xff],
    [0xf1, 0xf3, 0xf5],
    [0xad, 0xb5, 0xbd],
    [0x86, 0x8e, 0x96],
    [0x49, 0x50, 0x57],
    [0x34, 0x3a, 0x40],
    [0x21, 0x25, 0x29],
    [0x1e, 0x1e, 0x1e],
    [0x00, 0x00, 0x00],
];

/// Open-color tones 2, 4, 6 and 8 of gray, red, orange, yellow, green, teal,
/// blue, grape (in place of violet) and pink, light to dark. Open-color is
/// MIT (Excalidraw's palette source). Deviation: yellow 6, green 8 and blue 6
/// are the Palette defaults (Tailwind amber-500 and green-600, Apple blue), so
/// every default colors a cell and gets its ring.
const HUES: [[[u8; 3]; TONES]; COLUMNS] = [
    [
        [0xe9, 0xec, 0xef],
        [0xce, 0xd4, 0xda],
        [0x86, 0x8e, 0x96],
        [0x34, 0x3a, 0x40],
    ],
    [
        [0xff, 0xc9, 0xc9],
        [0xff, 0x87, 0x87],
        [0xfa, 0x52, 0x52],
        [0xe0, 0x31, 0x31],
    ],
    [
        [0xff, 0xd8, 0xa8],
        [0xff, 0xa9, 0x4d],
        [0xfd, 0x7e, 0x14],
        [0xe8, 0x59, 0x0c],
    ],
    [
        [0xff, 0xec, 0x99],
        [0xff, 0xd4, 0x3b],
        [0xf5, 0x9e, 0x0b],
        [0xf0, 0x8c, 0x00],
    ],
    [
        [0xb2, 0xf2, 0xbb],
        [0x69, 0xdb, 0x7c],
        [0x40, 0xc0, 0x57],
        [0x16, 0xa3, 0x4a],
    ],
    [
        [0x96, 0xf2, 0xd7],
        [0x38, 0xd9, 0xa9],
        [0x12, 0xb8, 0x86],
        [0x09, 0x92, 0x68],
    ],
    [
        [0xa5, 0xd8, 0xff],
        [0x4d, 0xab, 0xf7],
        [0x00, 0x7a, 0xff],
        [0x19, 0x71, 0xc2],
    ],
    [
        [0xee, 0xbe, 0xfa],
        [0xda, 0x77, 0xf2],
        [0xbe, 0x4b, 0xdb],
        [0x9c, 0x36, 0xb5],
    ],
    [
        [0xfc, 0xc2, 0xd7],
        [0xf7, 0x83, 0xac],
        [0xe6, 0x49, 0x80],
        [0xc2, 0x25, 0x5c],
    ],
];

/// Cell `i` of the grid in reading order (row of one tone, then the next).
pub(crate) fn preset(i: usize) -> Option<[u8; 3]> {
    HUES.get(i % COLUMNS)?.get(i / COLUMNS).copied()
}
