//! Tools: what pointer input does in Draw Mode.
//!
//! Adding a Tool is one new type implementing [`Tool`] in its own file plus
//! one line in [`Tools::new`] (standards rule 5). The toolbar button, the
//! shortcut keys, the cursor and the key-binding hint all come from the trait,
//! so no other Tool and no other module is edited.

mod pen;

use crate::freehand::Scratch;
use crate::history::History;
use crate::icons::Icon;
use crate::ink::{Element, Ink, Point, Rect};
use crate::render::Pending;
use crate::Key;
use std::fmt::Debug;

/// The mouse cursor shape the platform layer should show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Arrow,
    Crosshair,
}

/// What a Tool may change. A struct of borrows, so a Tool never sees the
/// Annotator or another Tool.
pub(crate) struct Ctx<'a> {
    pub ink: &'a mut Ink,
    pub history: &'a mut History,
    pub paint: &'a mut Pending,
    pub freehand: &'a mut Scratch,
    pub next_id: &'a mut u64,
    /// Physical pixels per logical pixel of the Overlay.
    pub scale: f32,
    /// Pressure of the pointer events in flight; `None` simulates it.
    pub pressure: Option<f32>,
}

impl Ctx<'_> {
    /// Physical pointer position to logical Element coordinates.
    pub fn logical(&self, at: Point) -> Option<Point> {
        (at.x.is_finite() && at.y.is_finite()).then(|| Point {
            x: at.x / self.scale,
            y: at.y / self.scale,
        })
    }

    /// Runs `step` on the last Element (the Stroke in progress) and marks
    /// what it changed.
    pub fn advance(&mut self, step: impl FnOnce(&mut Element, &mut Scratch) -> Option<Rect>) {
        if let Some(element) = self.ink.last_mut() {
            let origin = (element.x(), element.y());
            if let Some(local) = step(element, self.freehand) {
                self.paint.damage_local(local, origin, self.scale);
            }
        }
    }
}

pub(crate) trait Tool: Debug {
    fn icon(&self) -> &'static Icon;
    /// Lowercase keys that select this Tool. The first digit is also the
    /// hint printed on its toolbar button.
    fn keys(&self) -> &'static [char];
    fn cursor(&self) -> Cursor;
    fn pointer_down(&mut self, ctx: &mut Ctx<'_>, at: Point);
    fn pointer_move(&mut self, ctx: &mut Ctx<'_>, at: Point);
    fn pointer_up(&mut self, ctx: &mut Ctx<'_>, at: Point);
    /// A key press while this Tool is active. Returns whether it was used.
    fn key(&mut self, _ctx: &mut Ctx<'_>, _key: Key, _command: bool, _shift: bool) -> bool {
        false
    }
    /// A gesture is in progress (undo, redo and switching wait for its end).
    fn busy(&self) -> bool;
    /// Commits the gesture in progress, if any (Draw Mode ends, a button is
    /// pressed, or a pointer-up was lost).
    fn finish(&mut self, ctx: &mut Ctx<'_>);
}

/// The registry of Tools and which one is active.
#[derive(Debug)]
pub(crate) struct Tools {
    list: Vec<Box<dyn Tool>>,
    active: usize,
}

impl Tools {
    pub fn new() -> Self {
        // Registration: one line per Tool. The first one is the default.
        Self::with(vec![Box::new(pen::Pen::default())])
    }

    fn with(list: Vec<Box<dyn Tool>>) -> Self {
        Self { list, active: 0 }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn get(&self, index: usize) -> Option<&dyn Tool> {
        self.list.get(index).map(|t| &**t)
    }

    pub fn active_mut(&mut self) -> &mut dyn Tool {
        // The registry is never empty and `active` only holds valid indices.
        &mut *self.list[self.active]
    }

    pub fn select(&mut self, index: usize) {
        if index < self.list.len() {
            self.active = index;
        }
    }

    /// Selects the Tool bound to `key`; returns whether there was one.
    pub fn select_by_key(&mut self, key: char) -> bool {
        match self.list.iter().position(|t| t.keys().contains(&key)) {
            Some(index) => {
                self.active = index;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons;

    /// A second Tool, defined here and registered by one `Tools::with` call:
    /// the proof that adding a Tool edits nothing else.
    #[derive(Debug)]
    struct Stub;
    impl Tool for Stub {
        fn icon(&self) -> &'static Icon {
            &icons::UNDO
        }
        fn keys(&self) -> &'static [char] {
            &['k', '9']
        }
        fn cursor(&self) -> Cursor {
            Cursor::Arrow
        }
        fn pointer_down(&mut self, _: &mut Ctx<'_>, _: Point) {}
        fn pointer_move(&mut self, _: &mut Ctx<'_>, _: Point) {}
        fn pointer_up(&mut self, _: &mut Ctx<'_>, _: Point) {}
        fn busy(&self) -> bool {
            false
        }
        fn finish(&mut self, _: &mut Ctx<'_>) {}
    }

    #[test]
    fn a_new_tool_is_selected_by_its_own_keys_only() {
        let mut tools = Tools::with(vec![Box::new(pen::Pen::default()), Box::new(Stub)]);
        assert!(tools.select_by_key('9'));
        assert_eq!(tools.active(), 1);
        assert!(tools.select_by_key('p'));
        assert_eq!(tools.active(), 0);
        assert!(!tools.select_by_key('x'));
    }
}
