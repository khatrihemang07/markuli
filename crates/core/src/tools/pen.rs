//! The Pen: one Stroke from pointer down to pointer up.
//!
//! The Stroke is an Element at the end of the Ink that grows with each point.
//! Excalidraw's freehand outline arrives with the freehand ticket; it extends
//! this Tool, not the others.

use super::{Ctx, Cursor, Tool};
use crate::icons::{self, Icon};
use crate::ink::{Element, Point};
use crate::Key;

#[derive(Debug, Default)]
pub(crate) struct Pen {
    stroking: bool,
}

impl Tool for Pen {
    fn icon(&self) -> &'static Icon {
        &icons::PEN
    }

    /// Excalidraw's shortcuts: P and 7.
    fn keys(&self) -> &'static [char] {
        &['p', '7']
    }

    fn cursor(&self) -> Cursor {
        Cursor::Crosshair
    }

    fn pointer_down(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        // A lost pointer-up must not merge two Strokes into one log entry.
        self.finish(ctx);
        self.stroking = true;
        ctx.ink.add(Element::start(at));
    }

    fn pointer_move(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        if self.stroking {
            if let Some(element) = ctx.ink.last_mut() {
                element.push(at);
            }
        }
    }

    fn pointer_up(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        if self.stroking {
            self.pointer_move(ctx, at);
            self.finish(ctx);
        }
    }

    /// Esc only cancels the Stroke in progress; it never Clears.
    fn key(&mut self, ctx: &mut Ctx<'_>, key: Key, _command: bool, _shift: bool) -> bool {
        if key == Key::Escape && self.stroking {
            self.stroking = false;
            ctx.ink.pop();
            ctx.paint.full();
            return true;
        }
        false
    }

    fn busy(&self) -> bool {
        self.stroking
    }

    fn finish(&mut self, ctx: &mut Ctx<'_>) {
        if self.stroking {
            self.stroking = false;
            ctx.history.record_add();
        }
    }
}
