//! The Pen: one Stroke from pointer down to pointer up.
//!
//! The Stroke is the last Element of the Ink. It grows with each pointer
//! event; the freehand outline and its damage come from `Element`.

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
        if let Some(at) = ctx.logical(at) {
            self.stroking = true;
            let element = Element::start(*ctx.next_id, at, ctx.pressure);
            *ctx.next_id += 1;
            ctx.ink.add(element);
            ctx.advance(Element::preview);
        }
    }

    fn pointer_move(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        if self.stroking {
            if let Some(at) = ctx.logical(at) {
                let pressure = ctx.pressure;
                ctx.advance(|e, scratch| e.push(at, pressure, scratch));
            }
        }
    }

    fn pointer_up(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        if self.stroking {
            let (at, pressure) = (ctx.logical(at), ctx.pressure);
            ctx.advance(|e, scratch| match at {
                Some(at) => e.commit(at, pressure, scratch),
                None => e.finish(scratch),
            });
            self.stroking = false;
            ctx.history.record_add();
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
            ctx.advance(Element::finish);
            self.stroking = false;
            ctx.history.record_add();
        }
    }
}
