//! Painting the toolbar: island fills, button backgrounds and icons.

use super::layout;
use super::theme::{fill_rounded, shadow, solid_paint, RADIUS};
use super::{Button, Chrome};
use crate::icons::{self, Icon};
use crate::render::{Canvas, Format};

/// Icon edge, in logical pixels.
const ICON: f32 = 16.0;

impl Chrome<'_> {
    pub fn paint(&self, target: &mut Canvas<'_, '_>, format: Format) {
        let (toolbar, tools) = (&*self.toolbar, self.tools);
        let Some(l) = toolbar.layout(tools.len()) else {
            return;
        };
        let s = l.scale;
        let colors = toolbar.theme.tokens();
        let solid = |rgb: [u8; 3]| solid_paint(rgb, 1.0, format);
        for island in l.islands() {
            shadow(target, island, s, format);
            fill_rounded(target, island, RADIUS * s, &solid(colors.island));
        }
        for i in 0..tools.len() + 3 {
            let rect = l.button_rect(i);
            let button = layout::button(i, tools.len());
            let selected = button == Button::Tool(self.ui.active);
            let hot = toolbar.hover == Some(button) || toolbar.press == Some(button);
            if selected || hot {
                let bg = if selected {
                    colors.selected
                } else {
                    colors.hover
                };
                fill_rounded(target, rect, RADIUS * s, &solid(bg));
            }
            let enabled = match button {
                Button::Tool(_) => true,
                Button::Undo => self.ui.can_undo,
                Button::Redo => self.ui.can_redo,
                Button::Clear => self.ui.has_ink,
            };
            let ink = match (selected, enabled) {
                (true, _) => colors.selected_icon,
                (false, true) => colors.icon,
                (false, false) => colors.disabled,
            };
            let icon: &Icon = match button {
                Button::Tool(t) => match tools.get(t) {
                    Some(tool) => tool.icon(),
                    None => continue,
                },
                Button::Undo => &icons::UNDO,
                Button::Redo => &icons::REDO,
                Button::Clear => &icons::TRASH,
            };
            let size = ICON * s;
            let at = (
                rect.x + (rect.w - size) / 2.0,
                rect.y + (rect.h - size) / 2.0,
                size,
            );
            icons::draw(target, icon, at, &solid(ink));
        }
    }
}
