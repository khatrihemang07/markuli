//! Painting the toolbar: island fills, button backgrounds, icons, colour
//! swatches and width lines.

use super::layout::{self, Area};
use super::theme::{fill_rounded, rounded_rect, shadow, solid_paint, Tokens, RADIUS};
use super::{Button, Chrome, Toolbar, UiState};
use crate::icons::{self, Icon};
use crate::render::{Canvas, Format};
use crate::style::PALETTE;
use crate::tools::Tools;
use tiny_skia::{LineCap, PathBuilder, Stroke, Transform};

/// Icon edge, in logical pixels.
const ICON: f32 = 16.0;
/// Swatch edge (Excalidraw's 1.35 rem swatch is about 22 px).
const SWATCH: f32 = 22.0;
/// Line thickness of the three width buttons' icons.
const ICON_LINES: [f32; 3] = [1.5, 3.0, 4.5];
/// How much of its colour a dimmed swatch keeps.
const DIMMED: f32 = 0.35;

/// The pieces every button is painted from.
struct Brush<'a> {
    colors: &'static Tokens,
    format: Format,
    scale: f32,
    toolbar: &'a Toolbar,
    tools: &'a Tools,
    ui: UiState,
}

impl Chrome<'_> {
    pub fn paint(&self, target: &mut Canvas<'_, '_>, format: Format) {
        let (toolbar, tools) = (&*self.toolbar, self.tools);
        let Some(l) = toolbar.layout(tools.len()) else {
            return;
        };
        let brush = Brush {
            colors: toolbar.theme.tokens(),
            format,
            scale: l.scale,
            toolbar,
            tools,
            ui: self.ui,
        };
        let solid = |rgb: [u8; 3]| solid_paint(rgb, 1.0, format);
        for island in l.islands() {
            shadow(target, island, brush.scale, format);
            fill_rounded(
                target,
                island,
                RADIUS * brush.scale,
                &solid(brush.colors.island),
            );
        }
        for i in 0..layout::count(tools.len()) {
            brush.button(target, l.button_rect(i), layout::button(i, tools.len()));
        }
    }
}

impl Brush<'_> {
    fn button(&self, target: &mut Canvas<'_, '_>, rect: Area, button: Button) {
        let (toolbar, ui) = (self.toolbar, self.ui);
        let selected = match button {
            Button::Tool(t) => t == ui.active,
            Button::Color(i) => ui.color == Some(i),
            Button::Width(i) => ui.width == Some(i),
            _ => false,
        };
        let hot = toolbar.hover == Some(button) || toolbar.press == Some(button);
        if selected || hot {
            let bg = if selected {
                self.colors.selected
            } else {
                self.colors.hover
            };
            fill_rounded(
                target,
                rect,
                RADIUS * self.scale,
                &solid_paint(bg, 1.0, self.format),
            );
        }
        let enabled = match button {
            Button::Undo => ui.can_undo,
            Button::Redo => ui.can_redo,
            Button::Clear => ui.has_ink,
            Button::Color(_) | Button::Width(_) => !ui.dimmed,
            Button::Tool(_) => true,
        };
        let ink = match (enabled, selected) {
            (false, _) => self.colors.disabled,
            (true, true) => self.colors.selected_icon,
            (true, false) => self.colors.icon,
        };
        match button {
            Button::Color(i) => self.swatch(target, rect, i, selected, enabled),
            Button::Width(i) => self.width_line(target, rect, i, ink),
            _ => self.icon(target, rect, button, ink),
        }
    }

    fn icon(&self, target: &mut Canvas<'_, '_>, rect: Area, button: Button, ink: [u8; 3]) {
        let icon: &Icon = match button {
            Button::Tool(t) => match self.tools.get(t) {
                Some(tool) => tool.icon(),
                None => return,
            },
            Button::Undo => &icons::UNDO,
            Button::Redo => &icons::REDO,
            Button::Clear => &icons::TRASH,
            Button::Color(_) | Button::Width(_) => return,
        };
        let size = ICON * self.scale;
        let at = (
            rect.x + (rect.w - size) / 2.0,
            rect.y + (rect.h - size) / 2.0,
            size,
        );
        icons::draw(target, icon, at, &solid_paint(ink, 1.0, self.format));
    }

    /// A palette colour in a bordered square; the chosen one gets a ring.
    fn swatch(
        &self,
        target: &mut Canvas<'_, '_>,
        rect: Area,
        index: usize,
        chosen: bool,
        enabled: bool,
    ) {
        let Some(&color) = PALETTE.get(index) else {
            return;
        };
        let s = self.scale;
        let alpha = if enabled { 1.0 } else { DIMMED };
        let around = |a: Area, grow: f32| Area {
            x: a.x - grow * s,
            y: a.y - grow * s,
            w: a.w + 2.0 * grow * s,
            h: a.h + 2.0 * grow * s,
        };
        let square = around(rect, (SWATCH - rect.w / s) / 2.0);
        let border = solid_paint(self.colors.swatch_border, alpha, self.format);
        fill_rounded(target, square, 4.0 * s, &border);
        let fill = solid_paint(color, alpha, self.format);
        fill_rounded(target, around(square, -1.0), 3.0 * s, &fill);
        if chosen {
            let ring = solid_paint(self.colors.swatch_active, alpha, self.format);
            let line = Stroke {
                width: s,
                ..Stroke::default()
            };
            if let Some(path) = rounded_rect(around(square, 2.5), 5.0 * s) {
                target.stroke_path(&path, &ring, &line, Transform::identity());
            }
        }
    }

    /// A horizontal round-capped line as thick as the width it chooses.
    fn width_line(&self, target: &mut Canvas<'_, '_>, rect: Area, index: usize, ink: [u8; 3]) {
        let Some(&thickness) = ICON_LINES.get(index) else {
            return;
        };
        let s = self.scale;
        let mut line = PathBuilder::new();
        let y = rect.y + rect.h / 2.0;
        line.move_to(rect.x + 9.0 * s, y);
        line.line_to(rect.x + 23.0 * s, y);
        if let Some(path) = line.finish() {
            let stroke = Stroke {
                width: thickness * s,
                line_cap: LineCap::Round,
                ..Stroke::default()
            };
            let paint = solid_paint(ink, 1.0, self.format);
            target.stroke_path(&path, &paint, &stroke, Transform::identity());
        }
    }
}
