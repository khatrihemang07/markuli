//! The Palette: the 5 color slots and 3 width slots on the Toolbar.
//!
//! A width slot is the drawn line's thickness in pt (logical px) at mid
//! pressure. The default colors are red, amber, green, blue and grape: red and
//! grape are open-color picks (Excalidraw colors.ts, 0.18, MIT), amber and green
//! are Tailwind's amber-500 and green-600, blue is Apple's system blue. They
//! read on both a white and a dark page better than the black and yellow they
//! replace.

/// Color slots on the Toolbar.
pub(crate) const COLORS: usize = 5;
/// Width slots on the Toolbar.
pub(crate) const WIDTHS: usize = 3;
/// The width range a slot accepts, in pt; slots snap to steps of 0.25.
pub const MIN_WIDTH: f32 = 0.5;
pub const MAX_WIDTH: f32 = 30.0;

/// The 5 color slots and 3 width slots. The user edits them and they are
/// remembered; a [`Style`](crate::Style) is the chosen slot indices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub colors: [[u8; 3]; COLORS],
    pub widths: [f32; WIDTHS],
}

impl Palette {
    /// Red, amber, green, blue, grape; thin 2, medium 4, bold 8 pt. What
    /// Reset restores, slot by slot.
    pub const DEFAULT: Self = Self {
        colors: [
            [0xe0, 0x31, 0x31],
            [0xf5, 0x9e, 0x0b],
            [0x16, 0xa3, 0x4a],
            [0x00, 0x7a, 0xff],
            [0xbe, 0x4b, 0xdb],
        ],
        widths: [2.0, 4.0, 8.0],
    };

    /// A width slot value: `width` snapped to 0.25 and clamped to 0.5..=30;
    /// `None` for NaN or infinity.
    #[must_use]
    pub fn snap_width(width: f32) -> Option<f32> {
        width
            .is_finite()
            .then(|| ((width * 4.0).round() / 4.0).clamp(MIN_WIDTH, MAX_WIDTH))
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

/// The side of a button a picker or the system panel opens on: the one facing
/// away from the Toolbar's edge, so it never covers the rest of the Toolbar.
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

/// The "..." of the Color Picker was pressed: the platform opens the system
/// color panel for that slot, next to `anchor`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditRequest {
    pub kind: SlotKind,
    pub index: usize,
    pub anchor: Anchor,
    /// Where the panel opens relative to `anchor`.
    pub side: Side,
    /// What the slot holds now.
    pub value: SlotValue,
}
