//! Recent Colors: the last colors picked, newest first.

/// How many Recent Colors are kept.
pub(crate) const CAPACITY: usize = 6;

/// The last 6 colors picked from the Presets, a Custom drag or the system
/// color panel, newest first. `Copy`, so the View and the config carry it by
/// value and the pick path never allocates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecentColors {
    colors: [[u8; 3]; CAPACITY],
    len: usize,
}

impl RecentColors {
    /// The first 6 of `colors`, in order (newest first).
    #[must_use]
    pub fn new(colors: &[[u8; 3]]) -> Self {
        let mut recent = Self::default();
        for (slot, &rgb) in recent.colors.iter_mut().zip(colors) {
            *slot = rgb;
            recent.len += 1;
        }
        recent
    }

    /// The colors, newest first.
    #[must_use]
    pub fn as_slice(&self) -> &[[u8; 3]] {
        self.colors.get(..self.len).unwrap_or_default()
    }

    /// Puts `rgb` first: a color already in the list moves up instead of
    /// appearing twice, and the oldest falls off the end.
    pub(crate) fn push(&mut self, rgb: [u8; 3]) {
        let at = self.as_slice().iter().position(|&c| c == rgb);
        let end = at.unwrap_or_else(|| self.len.min(CAPACITY - 1));
        self.colors.copy_within(..end, 1);
        self.colors[0] = rgb;
        self.len = (self.len + usize::from(at.is_none())).min(CAPACITY);
    }
}
