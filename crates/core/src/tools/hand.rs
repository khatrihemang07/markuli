//! The Hand Tool: a drag changes the Scroll by the pointer's vertical movement.
//!
//! It never touches Ink, the Selection or the operation log. Holding Alt at
//! the press makes any Tool's drag a Hand drag (see `Tools::active_mut`).

use super::{Ctx, Cursor, Tool, ToolKind};
use crate::icons::{self, Icon};
use crate::ink::Point;
use crate::style::Appearance;
use crate::Key;

#[derive(Debug, Default)]
pub(crate) struct Hand {
    /// The previous pointer y (physical) while the pointer is down.
    last: Option<f32>,
}

impl Tool for Hand {
    fn kind(&self) -> ToolKind {
        ToolKind::Hand
    }

    fn icon(&self) -> &'static Icon {
        &icons::HAND
    }

    /// H (Excalidraw).
    fn keys(&self) -> &'static [char] {
        &['h']
    }

    fn cursor(&self, _: Appearance) -> Cursor {
        Cursor::Hand {
            grabbing: self.last.is_some(),
        }
    }

    fn pointer_down(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        self.last = ctx.screen(at).map(|_| at.y);
    }

    fn pointer_move(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        if let (Some(last), Some(_)) = (self.last, ctx.screen(at)) {
            ctx.scroll_by((at.y - last) / ctx.scale);
            self.last = Some(at.y);
        }
    }

    fn pointer_up(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        self.pointer_move(ctx, at);
        self.finish(ctx);
    }

    /// Esc ends the drag; the Scroll stays where it is.
    fn key(&mut self, _ctx: &mut Ctx<'_>, key: Key, _command: bool, _shift: bool) -> bool {
        key == Key::Escape && self.last.take().is_some()
    }

    fn busy(&self) -> bool {
        self.last.is_some()
    }

    fn finish(&mut self, _: &mut Ctx<'_>) {
        self.last = None;
    }
}
