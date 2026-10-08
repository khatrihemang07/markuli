//! How an Element looks: stroke color and width, and the choices that set
//! them. The values come from the [`Palette`].

use crate::freehand::Scratch;
use crate::history::History;
use crate::ink::{Ink, Rect};
use crate::palette::{Palette, COLORS, WIDTHS};
use crate::render::Pending;

/// The Style a user chooses: a color slot and a width slot of the
/// [`Palette`] (thin, medium, bold by default). Out-of-range indices are
/// ignored by whoever applies them.
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

impl Style {
    /// Whether both indices name a color slot and a width slot.
    pub(crate) fn is_valid(self) -> bool {
        self.color < COLORS && self.width < WIDTHS
    }
}

/// What one Element looks like: its stroke rgb and px width.
///
/// This is a [`Style`] resolved against the Palette, so the stroke color and
/// width can be applied to an Element (and compared with an Element's own)
/// without looking slots up again. It is an implementation detail, not a
/// glossary term.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Appearance {
    pub color: [u8; 3],
    pub width: f32,
}

impl Default for Appearance {
    /// Excalidraw red, medium width.
    fn default() -> Self {
        Self::of(Style::default(), &Palette::DEFAULT)
    }
}

/// One color or width choice: a key, a Toolbar click or a Palette edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    /// The i-th color slot.
    Color(usize),
    /// The i-th width slot.
    Width(usize),
    /// The previous width slot (`[`).
    Thinner,
    /// The next width slot (`]`).
    Bolder,
}

impl Choice {
    /// The choice as slot indices: `Thinner` and `Bolder` step from the
    /// chosen `slots`, so they follow slot order whatever the widths are.
    pub fn pinned(self, slots: Style) -> Self {
        match self {
            Self::Thinner => Self::Width(slots.width.saturating_sub(1)),
            Self::Bolder => Self::Width((slots.width + 1).min(WIDTHS - 1)),
            other => other,
        }
    }

    /// `slots` after this choice; unchanged when it names no slot.
    pub fn slots(self, slots: Style) -> Style {
        let next = match self {
            Self::Color(color) => Style { color, ..slots },
            Self::Width(width) => Style { width, ..slots },
            Self::Thinner | Self::Bolder => slots,
        };
        if next.is_valid() {
            next
        } else {
            slots
        }
    }
}

impl Appearance {
    /// The Appearance of `style`'s slots; out-of-range slots give the default
    /// slot's value.
    pub fn of(style: Style, palette: &Palette) -> Self {
        let style = if style.is_valid() {
            style
        } else {
            Style::default()
        };
        Self {
            color: palette.colors[style.color],
            width: palette.widths[style.width],
        }
    }

    /// This Appearance after `choice`, relative to its own width for
    /// `Thinner` and `Bolder` (a Selection has no slot of its own); an
    /// unknown slot changes nothing.
    pub fn with(self, choice: Choice, palette: &Palette) -> Self {
        match choice {
            Choice::Color(i) => Self {
                color: palette.colors.get(i).copied().unwrap_or(self.color),
                ..self
            },
            Choice::Width(i) => Self {
                width: palette.widths.get(i).copied().unwrap_or(self.width),
                ..self
            },
            Choice::Thinner => self.with(
                Choice::Width(self.width_index(palette).saturating_sub(1)),
                palette,
            ),
            Choice::Bolder => self.with(Choice::Width(self.width_index(palette) + 1), palette),
        }
    }

    /// The slot of this Appearance's color, if some slot holds it.
    pub fn color_index(self, palette: &Palette) -> Option<usize> {
        palette.colors.iter().position(|c| *c == self.color)
    }

    /// The nearest width slot, so an imported odd width still steps sensibly.
    pub fn width_index(self, palette: &Palette) -> usize {
        let distance = |w: &f32| (w - self.width).abs();
        palette
            .widths
            .iter()
            .enumerate()
            .min_by(|a, b| distance(a.1).total_cmp(&distance(b.1)))
            .map_or(0, |(i, _)| i)
    }
}

/// Applies `change` to each selected Element's own Appearance. `gesture` collects,
/// on the first change, the style every selected Element had before it (with
/// its Ink index): [`finish`] turns it into one operation-log entry.
pub(crate) fn restyle(
    ink: &mut Ink,
    ids: &[u64],
    change: impl Fn(Appearance) -> Appearance,
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
        let new = change(old);
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
