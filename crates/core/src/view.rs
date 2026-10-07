//! What the Annotator tells the outside: the [`View`], read access to Ink and
//! Selection, the copy text, and rendering to pixels.

use crate::panel::PanelView;
use crate::toolbar::{Chrome, UiState};
use crate::tools::{StyleTarget, Tool};
use crate::{render, Annotator, Cursor, Damage, Element, Format, Ink, View};
#[cfg(feature = "test-support")]
use crate::{Button, Control, Point};

impl Annotator {
    #[must_use]
    pub fn view(&self) -> View {
        let over_toolbar = (self.toolbar.over() || self.panel.over()) && !self.tool_busy();
        View {
            draw_mode: self.draw_mode,
            overlay_needed: self.draw_mode || !self.ink.is_empty(),
            display: self.display,
            needs_render: self.paint.is_pending()
                || self.toolbar.is_dirty(self.draw_mode, self.ui())
                || self.panel.is_dirty(self.panel_view()),
            cursor: match self.tools.get(self.tools.active()) {
                Some(tool) if !over_toolbar => tool.cursor(),
                _ => Cursor::Arrow,
            },
            tool: self.tools.active_kind(),
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

    /// The centre of a style panel control in Overlay pixels (for
    /// `Opacity`, the thumb at that value); `None` while the panel is hidden.
    /// It shows for the Pen and, with a Selection, for the Select Tool.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn panel_center(&self, control: Control) -> Option<Point> {
        self.panel_view().map(|_| self.panel.center_of(control))
    }

    /// Draws what changed since the last call into `target` (premultiplied
    /// pixels, `format` channel order) and returns the changed region.
    pub fn render(
        &mut self,
        target: &mut tiny_skia::PixmapMut<'_>,
        format: Format,
    ) -> Option<Damage> {
        let panel_view = self.panel_view();
        let chrome = Chrome {
            visible: self.draw_mode,
            ui: self.ui(),
            toolbar: &mut self.toolbar,
            tools: &self.tools,
            panel_view,
            panel: &mut self.panel,
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
        UiState {
            active: self.tools.active(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            has_ink: !self.ink.is_empty(),
        }
    }

    /// What the style panel shows, or `None` while it is hidden: in Draw Mode,
    /// for the Pen (the style of the next Strokes) and for the Select Tool
    /// once something is selected (the Selection's style).
    pub(super) fn panel_view(&self) -> Option<PanelView> {
        if !self.draw_mode || !self.panel.fits() {
            return None;
        }
        let target = self.tools.get(self.tools.active()).map(Tool::styles);
        match target {
            Some(StyleTarget::NextStrokes) => Some(PanelView {
                color: Some(self.style.color),
                width: Some(self.style.width),
                opacity: self.style.opacity,
            }),
            Some(StyleTarget::Selection) if !self.selection.is_empty() => {
                let chosen = self
                    .ink
                    .elements()
                    .iter()
                    .filter(|e| self.selection.contains(e.id()));
                chosen.map(Element::style).fold(None, |view, s| {
                    Some(match view {
                        None => PanelView {
                            color: Some(s.color),
                            width: Some(s.width),
                            opacity: s.opacity,
                        },
                        Some(v) => PanelView {
                            color: v.color.filter(|&c| c == s.color),
                            #[allow(clippy::float_cmp, reason = "widths come from a fixed list")]
                            width: v.width.filter(|&w| w == s.width),
                            ..v
                        },
                    })
                })
            }
            _ => None,
        }
    }
}
