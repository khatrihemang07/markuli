//! The Eraser: drag across Elements to remove them, in one undoable step.
//!
//! Follows Excalidraw's `handleEraser` (App.tsx): while dragging, each
//! pointer segment is sampled every `threshold` px (both ends included) and
//! every Element whose input polyline passes within `threshold` of a sample
//! is marked and drawn at a fifth of its opacity; pointer up removes all the
//! marked Elements at once. Deviations: no eraser trail is drawn, and there is
//! no Alt-to-restore.

use super::{Ctx, Cursor, Tool, ToolKind};
use crate::icons::{self, Icon};
use crate::ink::{Element, Point};
use crate::style::Style;
use crate::Key;

/// Excalidraw's `DEFAULT_COLLISION_THRESHOLD` (2 * 4 - epsilon), in logical px.
const THRESHOLD: f32 = 8.0 - 0.00001;

/// The cursor circle: the reach `THRESHOLD`, both sides, in logical px.
const ERASER_DIAMETER: u16 = 16;

#[derive(Debug, Default)]
pub(crate) struct Eraser {
    /// The previous pointer position (logical) while the pointer is down.
    last: Option<Point>,
}

/// Marks the Elements near `at` and redraws them faded.
fn mark(ctx: &mut Ctx<'_>, at: Point) {
    let (paint, scale) = (&mut *ctx.paint, ctx.scale);
    ctx.ink
        .mark_near(at, THRESHOLD, |e| redraw(paint, scale, e));
}

fn redraw(paint: &mut crate::render::Pending, scale: f32, element: &Element) {
    if let Some(bounds) = element.bounds() {
        paint.damage_local(bounds, (element.x(), element.y()), scale);
    }
}

impl Tool for Eraser {
    fn kind(&self) -> ToolKind {
        ToolKind::Eraser
    }

    fn icon(&self) -> &'static Icon {
        &icons::ERASER
    }

    /// E, 3 (its place in the toolbar) and Excalidraw's 0.
    fn keys(&self) -> &'static [char] {
        &['e', '3', '0']
    }

    /// Pressing E or 0 on the Eraser swaps back to the Pen.
    fn toggle(&self) -> Option<(&'static [char], ToolKind)> {
        Some((&['e', '0'], ToolKind::Pen))
    }

    fn cursor(&self, _: Style) -> Cursor {
        Cursor::Eraser {
            diameter: ERASER_DIAMETER,
        }
    }

    fn pointer_down(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        self.finish(ctx);
        if let Some(at) = ctx.logical(at) {
            self.last = Some(at);
            mark(ctx, at);
        }
    }

    fn pointer_move(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        let (Some(from), Some(to)) = (self.last, ctx.logical(at)) else {
            return;
        };
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let steps = (dx.hypot(dy) / THRESHOLD).ceil().clamp(1.0, 10_000.0);
        let mut k = 1.0;
        while k <= steps {
            let t = k / steps;
            mark(
                ctx,
                Point {
                    x: from.x + dx * t,
                    y: from.y + dy * t,
                },
            );
            k += 1.0;
        }
        self.last = Some(to);
    }

    fn pointer_up(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        if self.last.is_some() {
            self.pointer_move(ctx, at);
            self.finish(ctx);
        }
    }

    /// Esc abandons the drag: nothing is removed.
    fn key(&mut self, ctx: &mut Ctx<'_>, key: Key, _command: bool, _shift: bool) -> bool {
        if key == Key::Escape && self.last.take().is_some() {
            let (paint, scale) = (&mut *ctx.paint, ctx.scale);
            ctx.ink.unmark(|e| redraw(paint, scale, e));
            return true;
        }
        false
    }

    fn busy(&self) -> bool {
        self.last.is_some()
    }

    fn finish(&mut self, ctx: &mut Ctx<'_>) {
        if self.last.take().is_some() {
            let removed = ctx.ink.take_marked();
            if !removed.is_empty() {
                ctx.paint.full();
            }
            ctx.history.record_erase(removed);
        }
    }
}
