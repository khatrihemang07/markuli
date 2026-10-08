//! What the Annotator tells the outside: the [`View`], read access to Ink and
//! Selection, the copy text, and rendering to pixels.

use crate::toolbar::{Chrome, UiState};
use crate::tools::{StyleTarget, Tool};
use crate::{render, Annotator, Cursor, Damage, Element, Format, Ink, View};
#[cfg(feature = "test-support")]
use crate::{Button, Point};

impl Annotator {
    #[must_use]
    pub fn view(&self) -> View {
        let over_toolbar = self.toolbar.over() && !self.tool_busy();
        View {
            draw_mode: self.draw_mode,
            display: self.display,
            needs_render: self.paint.is_pending()
                || self.toolbar.is_dirty(self.draw_mode, self.ui()),
            cursor: match self.tools.get(self.tools.active()) {
                Some(tool) if !over_toolbar => tool.cursor(self.style),
                _ => Cursor::Arrow,
            },
            tool: self.tools.active_kind(),
            style: self.slots,
            palette: self.palette,
            edit: self.edit,
            next_frame: self.laser.next_frame(),
        }
    }

    #[must_use]
    pub fn ink(&self) -> &Ink {
        &self.ink
    }

    /// The ids ([`Element::id`]) of the selected Elements, in the order they
    /// were selected.
    #[must_use]
    pub fn selection(&self) -> &[u64] {
        self.selection.ids()
    }

    /// The Excalidraw clipboard JSON of the last copy (Cmd/Ctrl+C), once.
    /// The platform layer calls this after each key event and writes the text
    /// to the OS clipboard.
    pub fn take_copy(&mut self) -> Option<String> {
        self.copied.take()
    }

    /// The centre of a toolbar button in Overlay pixels; `None` while the
    /// toolbar is hidden (outside Draw Mode, or before the first `Resize`).
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn button_center(&self, button: Button) -> Option<Point> {
        if !self.draw_mode {
            return None;
        }
        self.toolbar.center_of(button, self.tools.len())
    }

    /// Draws what changed since the last call into `target` (premultiplied
    /// pixels, `format` channel order) and returns the changed region.
    pub fn render(
        &mut self,
        target: &mut tiny_skia::PixmapMut<'_>,
        format: Format,
    ) -> Option<Damage> {
        let chrome = Chrome {
            visible: self.draw_mode,
            ui: self.ui(),
            toolbar: &mut self.toolbar,
            tools: &self.tools,
        };
        render::render(
            &self.ink,
            &self.laser,
            &self.selection,
            &mut self.paint,
            self.draw_mode,
            self.scale,
            target,
            format,
            chrome,
        )
    }

    pub(super) fn ui(&self) -> UiState {
        let chosen = self.chosen_style();
        UiState {
            color: chosen.color,
            width: chosen.width,
            dimmed: chosen.dimmed,
            active: self.tools.active(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            has_ink: !self.ink.is_empty(),
            palette: self.palette,
        }
    }

    /// The chosen color and width the Toolbar highlights, and whether the
    /// active Tool leaves them dimmed. With a Selection they are the
    /// Selection's own values, `None` where it mixes them; otherwise the Pen's
    /// Style (a dimmed Tool shows it too: a click then switches to the Pen).
    fn chosen_style(&self) -> Chosen {
        let target = self.tools.get(self.tools.active()).map(Tool::styles);
        let own = Chosen {
            color: Some(self.slots.color),
            width: Some(self.slots.width),
            dimmed: false,
        };
        match target {
            Some(StyleTarget::NextStrokes) => own,
            Some(StyleTarget::Selection) if !self.selection.is_empty() => {
                let chosen = self
                    .ink
                    .elements()
                    .iter()
                    .filter(|e| self.selection.contains(e.id()));
                let mut appearances = chosen.map(Element::style);
                let Some(first) = appearances.next() else {
                    return own;
                };
                let p = &self.palette;
                let first = (first.color_index(p), Some(first.width_index(p)));
                let (color, width) = appearances.fold(first, |(c, w), s| {
                    (
                        c.filter(|&c| s.color_index(p) == Some(c)),
                        w.filter(|&w| s.width_index(p) == w),
                    )
                });
                Chosen {
                    color,
                    width,
                    dimmed: false,
                }
            }
            _ => Chosen {
                dimmed: true,
                ..own
            },
        }
    }
}

/// What the Toolbar highlights: palette and width indices (`None` where a
/// Selection mixes them), and whether the active Tool leaves them dimmed.
struct Chosen {
    color: Option<usize>,
    width: Option<usize>,
    dimmed: bool,
}
