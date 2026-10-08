//! The Palette: the 5 color slots and 3 width slots on the Toolbar.
//!
//! The defaults are Excalidraw's: the quick stroke palette
//! (`DEFAULT_ELEMENT_STROKE_PICKS`, colors.ts, open-color shade 4 of the
//! 0/2/4/6/8 picks) and its stroke widths 1, 2 and 4 (constants.ts
//! `STROKE_WIDTH`: thin, bold, extra bold; Markuli calls them thin, medium
//! and bold). Excalidraw 0.18, MIT.

/// Color slots on the Toolbar.
pub(crate) const COLORS: usize = 5;
/// Width slots on the Toolbar.
pub(crate) const WIDTHS: usize = 3;
/// The width range a slot accepts, in px; slots snap to steps of 0.5.
pub const MIN_WIDTH: f32 = 0.5;
pub const MAX_WIDTH: f32 = 20.0;

/// The 5 color slots and 3 width slots. The user edits them and they are
/// remembered; a [`Style`](crate::Style) is the chosen slot indices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub colors: [[u8; 3]; COLORS],
    pub widths: [f32; WIDTHS],
}

impl Palette {
    /// Black, red, green, blue, yellow; thin 1, medium 2, bold 4. What Reset
    /// restores, slot by slot.
    pub const DEFAULT: Self = Self {
        colors: [
            [0x1e, 0x1e, 0x1e],
            [0xe0, 0x31, 0x31],
            [0x2f, 0x9e, 0x44],
            [0x19, 0x71, 0xc2],
            [0xf0, 0x8c, 0x00],
        ],
        widths: [1.0, 2.0, 4.0],
    };

    /// A width slot value: `width` snapped to 0.5 and clamped to 0.5..=20;
    /// `None` for NaN or infinity.
    #[must_use]
    pub fn snap_width(width: f32) -> Option<f32> {
        width
            .is_finite()
            .then(|| ((width * 2.0).round() / 2.0).clamp(MIN_WIDTH, MAX_WIDTH))
    }

    /// What Reset restores for a slot; `None` for an out-of-range slot.
    #[must_use]
    pub fn default_slot(kind: SlotKind, index: usize) -> Option<SlotValue> {
        match kind {
            SlotKind::Color => Self::DEFAULT
                .colors
                .get(index)
                .copied()
                .map(SlotValue::Color),
            SlotKind::Width => Self::DEFAULT
                .widths
                .get(index)
                .copied()
                .map(SlotValue::Width),
        }
    }

    /// A width typed by the user: snapped and clamped as a slot value; a
    /// comma counts as the decimal point. `None` when it is not a number.
    #[must_use]
    pub fn parse_width(text: &str) -> Option<f32> {
        Self::snap_width(text.trim().replace(',', ".").parse().ok()?)
    }

    /// A width as the editors show it: `2` and `2.5`, never `2.0`.
    #[must_use]
    pub fn width_text(width: f32) -> String {
        if width.fract() == 0.0 {
            format!("{width:.0}")
        } else {
            format!("{width:.1}")
        }
    }

    /// This Palette with every width made a valid slot value; one that is not
    /// finite takes the default of its slot.
    pub(crate) fn sanitized(mut self) -> Self {
        for (width, default) in self.widths.iter_mut().zip(Self::DEFAULT.widths) {
            *width = Self::snap_width(*width).unwrap_or(default);
        }
        self
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Which kind of Palette slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Color,
    Width,
}

/// What a slot holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SlotValue {
    Color([u8; 3]),
    Width(f32),
}

/// The side of a button an editor opens on: the one facing away from the
/// Toolbar's edge, so the editor never covers the rest of the Toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Below,
    Above,
    Left,
    Right,
}

/// A rectangle in physical pixels of the Overlay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchor {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A secondary click landed on a color or width button: the platform opens
/// its editor for that slot next to `anchor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditRequest {
    pub kind: SlotKind,
    pub index: usize,
    pub anchor: Anchor,
    /// Where the editor opens relative to `anchor`.
    pub side: Side,
    /// What the slot holds now.
    pub value: SlotValue,
}
