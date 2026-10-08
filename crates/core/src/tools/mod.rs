//! Tools: what pointer input does in Draw Mode.
//!
//! Adding a Tool is one new type implementing [`Tool`] in its own file plus
//! one line in [`Tools::new`] (standards rule 5). The toolbar button, the
//! shortcut keys and the cursor all come from the trait,
//! so no other Tool and no other module is edited.

mod eraser;
mod laser;
mod pen;
mod select;

use crate::freehand::Scratch;
use crate::history::History;
use crate::icons::Icon;
use crate::ink::{Element, Ink, Point, Rect};
use crate::laser::Laser;
use crate::render::Pending;
use crate::selection::Selection;
use crate::style::Style;
use crate::Key;
use std::fmt::Debug;

/// The mouse cursor the platform layer should show.
///
/// The core decides what the cursor shows (a drawing Tool's cursor says what
/// it will do), the platform layer draws it. Sizes are logical pixels: the
/// platform multiplies them by the display scale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    /// The system arrow.
    Arrow,
    /// A marker whose dot is as wide as the Stroke the Pen draws, in its color.
    Pen { diameter: u16, color: [u8; 3] },
    /// An eraser block whose contact corner reaches `diameter / 2` around the pointer.
    Eraser { diameter: u16 },
    /// A red ring with a dot in the middle.
    Laser,
}

/// Which Tool is active, for the platform layer and tests (the registry
/// itself stays private).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Select,
    Pen,
    Eraser,
    Laser,
}

/// What the style panel does while a Tool is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StyleTarget {
    /// No panel: the Tool draws nothing styled.
    None,
    /// The panel styles the Strokes the Tool draws next.
    NextStrokes,
    /// The panel styles the Selection, and shows only when there is one.
    Selection,
}

/// What a Tool may change. A struct of borrows, so a Tool never sees the
/// Annotator or another Tool.
pub(crate) struct Ctx<'a> {
    pub ink: &'a mut Ink,
    pub history: &'a mut History,
    pub paint: &'a mut Pending,
    pub freehand: &'a mut Scratch,
    pub selection: &'a mut Selection,
    pub laser: &'a mut Laser,
    pub next_id: &'a mut u64,
    /// Physical pixels per logical pixel of the Overlay.
    pub scale: f32,
    /// Pressure of the pointer events in flight; `None` simulates it.
    pub pressure: Option<f32>,
    /// Shift is held.
    pub shift: bool,
    /// The style of the next Strokes.
    pub style: Style,
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
    fn kind(&self) -> ToolKind;
    fn icon(&self) -> &'static Icon;
    /// Lowercase keys that select this Tool: a letter, its toolbar position
    /// as a digit (1-4) and any Excalidraw digit.
    fn keys(&self) -> &'static [char];
    /// The keys that swap this Tool for another when it is already active,
    /// and that other Tool (P and E swap Pen and Eraser).
    fn toggle(&self) -> Option<(&'static [char], ToolKind)> {
        None
    }
    /// The cursor over the canvas; `style` is the style of the next Strokes.
    fn cursor(&self, style: Style) -> Cursor;
    /// What the style panel applies to while this Tool is active.
    fn styles(&self) -> StyleTarget {
        StyleTarget::None
    }
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
        // Registration: one line per Tool, in toolbar order.
        let mut tools = Self::with(vec![
            Box::new(select::Select::default()),
            Box::new(pen::Pen::default()),
            Box::new(eraser::Eraser::default()),
            Box::new(laser::LaserTool::default()),
        ]);
        tools.reset();
        tools
    }

    fn with(list: Vec<Box<dyn Tool>>) -> Self {
        Self { list, active: 0 }
    }

    /// The Pen is the default Tool whenever Draw Mode starts.
    pub fn reset(&mut self) {
        self.select_by_key('p');
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn active_kind(&self) -> ToolKind {
        self.list
            .get(self.active)
            .map_or(ToolKind::Pen, |t| t.kind())
    }

    pub fn get(&self, index: usize) -> Option<&dyn Tool> {
        self.list.get(index).map(|t| &**t)
    }

    pub fn active_mut(&mut self) -> &mut dyn Tool {
        // Invariant: the registry is never empty (`new` registers four Tools)
        // and `select*` only store indices below `len`, so this cannot panic.
        &mut *self.list[self.active]
    }

    pub fn select(&mut self, index: usize) {
        if index < self.list.len() {
            self.active = index;
        }
    }

    /// A Tool key pressed on the keyboard: the active Tool's toggle keys
    /// switch to its partner, any other key selects its Tool.
    pub fn press(&mut self, key: char) {
        let partner = self
            .list
            .get(self.active)
            .and_then(|t| t.toggle())
            .filter(|(keys, _)| keys.contains(&key))
            .and_then(|(_, kind)| self.list.iter().position(|t| t.kind() == kind));
        match partner {
            Some(index) => self.active = index,
            None => {
                self.select_by_key(key);
            }
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
        fn kind(&self) -> ToolKind {
            ToolKind::Laser
        }
        fn icon(&self) -> &'static Icon {
            &icons::UNDO
        }
        fn keys(&self) -> &'static [char] {
            &['k', '9']
        }
        fn cursor(&self, _: Style) -> Cursor {
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
