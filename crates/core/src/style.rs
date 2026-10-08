//! How an Element looks: stroke color, width and opacity.
//!
//! The choices are Excalidraw's: the quick stroke palette
//! (`DEFAULT_ELEMENT_STROKE_PICKS`, colors.ts, open-color shade 4 of the
//! 0/2/4/6/8 picks) and its stroke widths 1, 2 and 4 (constants.ts
//! `STROKE_WIDTH`: thin, bold, extra bold; Markuli calls them thin, medium
//! and bold). Excalidraw 0.18, MIT. Opacity is 0 to 100 in steps of 10, like
//! its range input.

use crate::freehand::Scratch;
use crate::history::History;
use crate::ink::{Ink, Rect};
use crate::render::Pending;

/// Black, red (the default), green, blue, yellow.
pub(crate) const PALETTE: [[u8; 3]; 5] = [
    [0x1e, 0x1e, 0x1e],
    [0xe0, 0x31, 0x31],
    [0x2f, 0x9e, 0x44],
    [0x19, 0x71, 0xc2],
    [0xf0, 0x8c, 0x00],
];

/// Thin, medium, bold.
pub(crate) const WIDTHS: [f32; 3] = [1.0, 2.0, 4.0];

pub(crate) const OPACITY_STEP: u8 = 10;

/// The look a user chooses: a color index into the 5-color palette and a
/// width index into the 3 presets (thin, medium, bold). Out-of-range indices
/// are ignored by whoever applies them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub color: usize,
    pub width: usize,
}

impl Default for Style {
    /// Excalidraw red, medium width.
    fn default() -> Self {
        Self { color: 1, width: 1 }
    }
}

/// What one Element looks like: its stroke rgb and px width (and opacity).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Appearance {
    pub color: [u8; 3],
    pub width: f32,
    pub opacity: u8,
}

impl Default for Appearance {
    /// Excalidraw red, medium width, opaque (user story 19).
    fn default() -> Self {
        Self {
            color: PALETTE[1],
            width: WIDTHS[1],
            opacity: 100,
        }
    }
}

/// One choice in the style panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// The i-th color of the palette.
    Color(usize),
    /// The i-th width: thin, medium, bold.
    Width(usize),
    /// An opacity, 0 to 100 (rounded to a multiple of 10).
    Opacity(u8),
}

impl Style {
    /// Whether both indices name a palette colour and a width preset.
    pub(crate) fn is_valid(self) -> bool {
        self.color < PALETTE.len() && self.width < WIDTHS.len()
    }
}

impl Appearance {
    /// The chosen look of the next Strokes, or `self` if `style` is out of
    /// range.
    pub fn with_style(self, style: Style) -> Self {
        if !style.is_valid() {
            return self;
        }
        Self {
            color: PALETTE[style.color],
            width: WIDTHS[style.width],
            ..self
        }
    }

    /// This look as palette and width indices (the defaults for a look that
    /// is not a preset, which never happens for the Pen's own Appearance).
    pub fn as_style(self) -> Style {
        let default = Style::default();
        Style {
            color: PALETTE
                .iter()
                .position(|c| *c == self.color)
                .unwrap_or(default.color),
            width: WIDTHS
                .iter()
                .position(|w| (*w - self.width).abs() < f32::EPSILON)
                .unwrap_or(default.width),
        }
    }

    /// The style after `control` was chosen; an unknown choice changes nothing.
    pub fn with(self, control: Control) -> Self {
        match control {
            Control::Color(i) => Self {
                color: PALETTE.get(i).copied().unwrap_or(self.color),
                ..self
            },
            Control::Width(i) => Self {
                width: WIDTHS.get(i).copied().unwrap_or(self.width),
                ..self
            },
            Control::Opacity(v) => Self {
                opacity: (v.min(100) + OPACITY_STEP / 2) / OPACITY_STEP * OPACITY_STEP,
                ..self
            },
        }
    }
}

/// Applies `control` to the selected Elements. `gesture` collects, on the
/// first change, the style every selected Element had before it (with its Ink
/// index): [`finish`] turns it into one operation-log entry.
pub(crate) fn restyle(
    ink: &mut Ink,
    ids: &[u64],
    control: Control,
    gesture: &mut Option<Vec<(usize, Appearance)>>,
    (scratch, paint, scale): (&mut Scratch, &mut Pending, f32),
) {
    let before = gesture.get_or_insert_with(|| {
        let selected = ink.elements().iter().enumerate();
        selected
            .filter(|(_, e)| ids.contains(&e.id()))
            .map(|(i, e)| (i, e.style()))
            .collect()
    });
    for &(index, old) in before.iter() {
        let Some(element) = ink.get_mut(index) else {
            continue;
        };
        let new = old.with(control);
        if element.style() != new {
            damage(element.set_style(new, scratch), element, paint, scale);
        }
    }
}

/// Marks what a restyle changed on screen.
pub(crate) fn damage(
    changed: Option<Rect>,
    element: &crate::Element,
    paint: &mut Pending,
    scale: f32,
) {
    if let Some(local) = changed {
        paint.damage_local(local, (element.x(), element.y()), scale);
    }
}

/// Ends a restyle gesture: logs it when it changed anything.
pub(crate) fn finish(gesture: Option<Vec<(usize, Appearance)>>, ink: &Ink, history: &mut History) {
    let Some(mut before) = gesture else { return };
    before.retain(|&(i, s)| ink.elements().get(i).is_some_and(|e| e.style() != s));
    if !before.is_empty() {
        history.record_restyle(before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_style_is_the_default_appearance() {
        let style = Style::default();
        let look = Appearance::default();
        assert_eq!(PALETTE.get(style.color), Some(&look.color));
        assert_eq!(WIDTHS.get(style.width), Some(&look.width));
    }
}
