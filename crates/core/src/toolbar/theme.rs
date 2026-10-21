//! Theme tokens and the shapes the toolbar paints with:
//! shadow, solid paints and rounded rectangles.

use super::layout::Area;
use crate::render::{Format, Painter};
use tiny_skia::{Color, Paint, Path, PathBuilder};

/// Corner radius of islands and buttons, in logical pixels.
pub(crate) const RADIUS: f32 = 8.0;

/// The OS light or dark theme, an input to the core.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// Excalidraw's theme tokens, for the whole row.
pub(crate) struct Tokens {
    pub island: [u8; 3],
    pub icon: [u8; 3],
    pub hover: [u8; 3],
    pub selected: [u8; 3],
    pub selected_icon: [u8; 3],
    pub disabled: [u8; 3],
    /// Swatch border (`--color-gray-30`) and the active swatch outline
    /// (`--color-primary-darkest`).
    pub swatch_border: [u8; 3],
    pub swatch_active: [u8; 3],
    /// The warning on a color that is hard to see on the page (a red that
    /// reads on the island), and the page those colors are judged against:
    /// white in Light, `#1e1e1e` in Dark.
    pub low: [u8; 3],
    pub page: [u8; 3],
}

const LIGHT: Tokens = Tokens {
    island: [0xff, 0xff, 0xff],
    icon: [0x1b, 0x1b, 0x1f],
    hover: [0xf1, 0xf0, 0xff],
    selected: [0xe0, 0xdf, 0xff],
    selected_icon: [0x03, 0x00, 0x64],
    disabled: [0xb8, 0xb8, 0xb8],
    swatch_border: [0xd6, 0xd6, 0xd6],
    swatch_active: [0x4a, 0x47, 0xb1],
    low: [0xc9, 0x2a, 0x2a],
    page: [0xff, 0xff, 0xff],
};

const DARK: Tokens = Tokens {
    island: [0x23, 0x23, 0x29],
    icon: [0xe3, 0xe3, 0xe8],
    hover: [0x32, 0x30, 0x39],
    selected: [0x40, 0x3e, 0x6a],
    selected_icon: [0xe0, 0xdf, 0xff],
    disabled: [0x5c, 0x5c, 0x5c],
    swatch_border: [0x5c, 0x5c, 0x66],
    swatch_active: [0xa8, 0xa5, 0xff],
    low: [0xff, 0x87, 0x87],
    page: [0x1e, 0x1e, 0x1e],
};

impl Theme {
    pub(crate) fn tokens(self) -> &'static Tokens {
        match self {
            Theme::Light => &LIGHT,
            Theme::Dark => &DARK,
        }
    }
}

/// Stock `--shadow-island`, approximated by stacked translucent rounded
/// rectangles, outermost first: (grow, offset down, alpha), logical pixels.
pub(crate) fn shadow(target: &mut Painter<'_, '_>, island: Area, s: f32, format: Format) {
    const LAYERS: [(f32, f32, f32); 6] = [
        (13.0, 7.0, 0.010),
        (10.0, 6.0, 0.015),
        (7.0, 4.0, 0.020),
        (4.0, 2.0, 0.025),
        (1.5, 0.0, 0.050),
        (0.5, 0.0, 0.090),
    ];
    for (grow, dy, alpha) in LAYERS {
        let r = Area {
            x: island.x - grow * s,
            y: island.y - grow * s + dy * s,
            w: island.w + 2.0 * grow * s,
            h: island.h + 2.0 * grow * s,
        };
        fill_rounded(
            target,
            r,
            (RADIUS + grow) * s,
            &solid_paint([0, 0, 0], alpha, format),
        );
    }
}

pub(crate) fn solid_paint(rgb: [u8; 3], alpha: f32, format: Format) -> Paint<'static> {
    let [r, g, b] = if format == Format::Bgra {
        [rgb[2], rgb[1], rgb[0]]
    } else {
        rgb
    };
    let mut paint = Paint::default();
    let mut color = Color::from_rgba8(r, g, b, 255);
    color.apply_opacity(alpha);
    paint.set_color(color);
    paint.anti_alias = true;
    paint
}

pub(crate) fn fill_rounded(target: &mut Painter<'_, '_>, r: Area, radius: f32, paint: &Paint<'_>) {
    target.fill_with(|pb| push_rounded(pb, r, radius), paint);
}

/// A rounded rectangle from four cubic corner arcs.
pub(crate) fn rounded_rect(r: Area, radius: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    push_rounded(&mut pb, r, radius);
    pb.finish()
}

/// Adds the rounded rectangle to `pb`.
pub(crate) fn push_rounded(pb: &mut PathBuilder, r: Area, radius: f32) {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0);
    let k = radius * (1.0 - 0.552_284_8);
    let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
    pb.move_to(x0 + radius, y0);
    pb.line_to(x1 - radius, y0);
    pb.cubic_to(x1 - k, y0, x1, y0 + k, x1, y0 + radius);
    pb.line_to(x1, y1 - radius);
    pb.cubic_to(x1, y1 - k, x1 - k, y1, x1 - radius, y1);
    pb.line_to(x0 + radius, y1);
    pb.cubic_to(x0 + k, y1, x0, y1 - k, x0, y1 - radius);
    pb.line_to(x0, y0 + radius);
    pb.cubic_to(x0, y0 + k, x0 + k, y0, x0 + radius, y0);
    pb.close();
}
