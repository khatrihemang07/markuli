//! The Laser Tool: a fading trail while the pointer is down.
//!
//! It never touches Ink or the operation log (so it can't be undone, selected
//! or copied). The trail itself lives in `crate::laser`.

use super::{Ctx, Cursor, Tool, ToolKind};
use crate::icons::{self, Icon};
use crate::ink::Point;
use crate::style::Appearance;
use crate::Key;

#[derive(Debug, Default)]
pub(crate) struct LaserTool {
    down: bool,
}

impl Tool for LaserTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Laser
    }

    fn icon(&self) -> &'static Icon {
        &icons::LASER
    }

    /// K (Excalidraw).
    fn keys(&self) -> &'static [char] {
        &['k']
    }

    fn cursor(&self, _: Appearance) -> Cursor {
        Cursor::Laser
    }

    fn pointer_down(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        if let Some(at) = ctx.logical(at) {
            self.down = true;
            ctx.laser.start(at, ctx.paint, ctx.scale);
        }
    }

    fn pointer_move(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        // Hovering draws nothing: Excalidraw only trails while pressed.
        if self.down {
            if let Some(at) = ctx.logical(at) {
                ctx.laser.extend(at, ctx.paint, ctx.scale);
            }
        }
    }

    fn pointer_up(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        if self.down {
            self.pointer_move(ctx, at);
            self.finish(ctx);
        }
    }

    fn key(&mut self, ctx: &mut Ctx<'_>, key: Key, _command: bool, _shift: bool) -> bool {
        // Esc stops the trail from growing; it still fades out.
        if key == Key::Escape && self.down {
            self.finish(ctx);
            return true;
        }
        false
    }

    fn busy(&self) -> bool {
        self.down
    }

    fn finish(&mut self, ctx: &mut Ctx<'_>) {
        if self.down {
            self.down = false;
            ctx.laser.end(ctx.paint, ctx.scale);
        }
    }
}
