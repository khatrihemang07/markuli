//! The Select Tool: click, shift-click, drag box, drag to move, Delete.
//!
//! Behaviour follows Excalidraw's selection tool (App.tsx, MIT): a click picks
//! the topmost Element whose input polyline is within 8 px of the pointer; a
//! press inside a selected Element's box also grabs it; a drag box selects the
//! Elements it fully encloses and updates live; shift adds to or removes from
//! the Selection. Deviation: moves wait for a 3 px drag (Excalidraw's own
//! threshold is larger) so a shaky click never logs a move.
//!
//! One gesture is one operation-log entry: a move is logged at pointer up.

use super::{Ctx, Cursor, StyleTarget, Tool, ToolKind};
use crate::icons::{self, Icon};
use crate::ink::{Point, Rect};
use crate::selection::{encloses, normalized};
use crate::style::Appearance;
use crate::Key;

/// Excalidraw's `DEFAULT_COLLISION_THRESHOLD`, logical pixels.
const HIT_TOLERANCE: f32 = 8.0;

/// Pointer travel before a press on an Element becomes a move.
const DRAG_THRESHOLD: f32 = 3.0;

#[derive(Debug, Default)]
enum Gesture {
    #[default]
    Idle,
    /// A press on an Element. Dragging moves the Selection.
    Moving {
        origin: Point,
        /// Ink index and position of every moved Element, before the move.
        before: Vec<(usize, f32, f32)>,
        dragged: bool,
        /// The Element pressed: a click without drag narrows to it.
        pressed: u64,
        narrow: bool,
    },
    /// A press on empty space. Dragging draws the selection box.
    Boxing { origin: Point, base: Vec<u64> },
}

#[derive(Debug, Default)]
pub(crate) struct Select {
    gesture: Gesture,
}

impl Tool for Select {
    fn kind(&self) -> ToolKind {
        ToolKind::Select
    }

    fn icon(&self) -> &'static Icon {
        &icons::SELECTION
    }

    /// V (Excalidraw).
    fn keys(&self) -> &'static [char] {
        &['v']
    }

    fn cursor(&self, _: Appearance) -> Cursor {
        Cursor::Arrow
    }

    fn styles(&self) -> StyleTarget {
        StyleTarget::Selection
    }

    fn pointer_down(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        self.finish(ctx);
        let Some(at) = ctx.logical(at) else { return };
        let hit = hit_element(ctx, at);
        self.gesture = match (hit, ctx.shift) {
            (Some(id), true) => {
                ctx.selection.toggle(id);
                Gesture::Idle
            }
            (Some(id), false) => {
                let narrow = ctx.selection.contains(id) && ctx.selection.ids().len() > 1;
                if !ctx.selection.contains(id) {
                    ctx.selection.set_one(id);
                }
                Gesture::Moving {
                    origin: at,
                    before: selected_positions(ctx),
                    dragged: false,
                    pressed: id,
                    narrow,
                }
            }
            (None, shift) => {
                if !shift {
                    ctx.selection.set(Vec::new());
                }
                Gesture::Boxing {
                    origin: at,
                    base: ctx.selection.ids().to_vec(),
                }
            }
        };
    }

    fn pointer_move(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        let Some(at) = ctx.logical(at) else { return };
        match &mut self.gesture {
            Gesture::Idle => {}
            Gesture::Moving {
                origin,
                before,
                dragged,
                ..
            } => {
                let (dx, dy) = (at.x - origin.x, at.y - origin.y);
                if !*dragged && dx.hypot(dy) < DRAG_THRESHOLD {
                    return;
                }
                *dragged = true;
                move_to(ctx, before, dx, dy);
            }
            Gesture::Boxing { origin, base } => {
                let rect = normalized((origin.x, origin.y), (at.x, at.y));
                ctx.selection.set_box(Some(rect));
                // Per pointer move: rebuilt in the Selection's own buffer.
                let more = ctx
                    .ink
                    .elements()
                    .iter()
                    .filter(|e| encloses(rect, e.absolute_extent()) && !base.contains(&e.id()))
                    .map(crate::Element::id);
                ctx.selection.set_union(base, more);
            }
        }
    }

    fn pointer_up(&mut self, ctx: &mut Ctx<'_>, at: Point) {
        self.pointer_move(ctx, at);
        self.finish(ctx);
    }

    fn key(&mut self, ctx: &mut Ctx<'_>, key: Key, _command: bool, _shift: bool) -> bool {
        if self.busy() {
            return false;
        }
        match key {
            Key::Escape if !ctx.selection.is_empty() => {
                ctx.selection.clear();
                true
            }
            Key::Delete if !ctx.selection.is_empty() => {
                delete_selection(ctx);
                true
            }
            _ => false,
        }
    }

    fn busy(&self) -> bool {
        !matches!(self.gesture, Gesture::Idle)
    }

    fn finish(&mut self, ctx: &mut Ctx<'_>) {
        match std::mem::take(&mut self.gesture) {
            Gesture::Idle => {}
            Gesture::Moving {
                before,
                dragged,
                pressed,
                narrow,
                ..
            } => {
                if dragged {
                    ctx.history.record_move(before);
                } else if narrow {
                    ctx.selection.set_one(pressed);
                }
            }
            Gesture::Boxing { .. } => ctx.selection.set_box(None),
        }
    }
}

/// The topmost Element under `at`, else the first selected Element whose
/// padded box contains it (so a selection can be grabbed by its inside).
fn hit_element(ctx: &Ctx<'_>, at: Point) -> Option<u64> {
    let elements = ctx.ink.elements();
    elements
        .iter()
        .rev()
        .find(|e| e.is_near(at, HIT_TOLERANCE))
        .or_else(|| {
            elements
                .iter()
                .rev()
                .filter(|e| ctx.selection.contains(e.id()))
                .find(|e| inside(e.absolute_extent(), at))
        })
        .map(crate::Element::id)
}

fn inside([l, t, r, b]: Rect, at: Point) -> bool {
    // The same 4 px gap the selection rectangle is drawn with.
    at.x >= l - 4.0 && at.x <= r + 4.0 && at.y >= t - 4.0 && at.y <= b + 4.0
}

/// Ink index and position of every selected Element.
fn selected_positions(ctx: &Ctx<'_>) -> Vec<(usize, f32, f32)> {
    ctx.ink
        .elements()
        .iter()
        .enumerate()
        .filter(|(_, e)| ctx.selection.contains(e.id()))
        .map(|(i, e)| (i, e.x(), e.y()))
        .collect()
}

/// Puts every moved Element at its old position plus (`dx`, `dy`), damaging
/// where it was and where it is.
fn move_to(ctx: &mut Ctx<'_>, before: &[(usize, f32, f32)], dx: f32, dy: f32) {
    for &(index, x, y) in before {
        let Some(element) = ctx.ink.get_mut(index) else {
            continue;
        };
        let bounds = element.bounds();
        if let Some(local) = bounds {
            ctx.paint
                .damage_local(local, (element.x(), element.y()), ctx.scale);
        }
        element.set_position(x + dx, y + dy);
        if let Some(local) = bounds {
            ctx.paint
                .damage_local(local, (element.x(), element.y()), ctx.scale);
        }
    }
    ctx.selection.touch();
}

fn delete_selection(ctx: &mut Ctx<'_>) {
    let indices: Vec<usize> = selected_positions(ctx).iter().map(|p| p.0).collect();
    let mut removed: Vec<_> = indices
        .iter()
        .rev()
        .filter_map(|&i| ctx.ink.remove_at(i))
        .collect();
    removed.reverse();
    if removed.is_empty() {
        return;
    }
    ctx.history.record_delete(indices, removed);
    ctx.selection.clear();
    ctx.paint.full();
}
