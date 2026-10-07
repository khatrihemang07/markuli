//! The operation log behind undo and redo.
//!
//! Operations, not snapshots (standards rule 8): each entry holds only what
//! it changed. Applying or reverting an entry swaps its payload with the Ink,
//! so nothing is cloned and a Stroke's points exist exactly once.

use crate::freehand::Scratch;
use crate::ink::{Element, Ink};
use crate::style::Style;

#[derive(Debug)]
enum Op {
    /// One committed Stroke. Applied: the Element is the last one in the Ink
    /// and the payload is `None`. Reverted: the payload holds it.
    Add(Option<Element>),
    /// Clear. Applied: the payload holds what was removed and the Ink is
    /// empty. Reverted: the payload is empty and the Ink holds them again.
    Clear(Vec<Element>),
    /// Selected Elements moved. Each entry is an Ink index and the position
    /// the Element is *not* at: applying and reverting swap it in, so undo is
    /// exact (no float drift from adding and subtracting a delta).
    Move(Vec<(usize, f32, f32)>),
    /// Selected Elements deleted. `indices` (ascending) are where they sat in
    /// the Ink; `removed` holds them while the delete is applied and is empty
    /// while it is reverted.
    Delete {
        indices: Vec<usize>,
        removed: Vec<Element>,
    },
    /// One Eraser drag: the Elements it removed and where they were (oldest
    /// first). Applied: they are in the payload. Reverted: the payload is
    /// `None` and they are back in the Ink at their old positions.
    Erase(Vec<(usize, Option<Element>)>),
    /// One style change of the Selection. Each entry is an Ink index and the
    /// style the Element is *not* at: applying and reverting swap it in, like
    /// `Move`.
    Restyle(Vec<(usize, Style)>),
}

#[derive(Debug, Default)]
pub struct History {
    ops: Vec<Op>,
    /// Entries `..done` are applied; the rest can be redone.
    done: usize,
}

impl History {
    /// Logs the Stroke that just ended (it is already the Ink's last Element).
    pub fn record_add(&mut self) {
        self.push(Op::Add(None));
    }

    /// Logs a Clear; `removed` is what it took out of the Ink.
    pub fn record_clear(&mut self, removed: Vec<Element>) {
        self.push(Op::Clear(removed));
    }

    /// Logs a move that already happened; `before` holds each moved Element's
    /// Ink index and its position before the move.
    pub fn record_move(&mut self, before: Vec<(usize, f32, f32)>) {
        self.push(Op::Move(before));
    }

    /// Logs a delete that already happened: `indices` (ascending) are where
    /// the `removed` Elements sat.
    pub fn record_delete(&mut self, indices: Vec<usize>, removed: Vec<Element>) {
        self.push(Op::Delete { indices, removed });
    }

    /// Logs a restyle that already happened; `before` holds each restyled
    /// Element's Ink index and its style before the change.
    pub fn record_restyle(&mut self, before: Vec<(usize, Style)>) {
        self.push(Op::Restyle(before));
    }

    /// Logs an Eraser drag; `removed` is what it took out, with positions.
    pub fn record_erase(&mut self, removed: Vec<(usize, Element)>) {
        if !removed.is_empty() {
            let slots = removed.into_iter().map(|(i, e)| (i, Some(e))).collect();
            self.push(Op::Erase(slots));
        }
    }

    /// A new operation ends the redo branch.
    fn push(&mut self, op: Op) {
        self.ops.truncate(self.done);
        self.ops.push(op);
        self.done += 1;
    }

    /// Forgets everything, e.g. when the Overlay moves to another display.
    pub fn reset(&mut self) {
        self.ops.clear();
        self.done = 0;
    }

    pub fn can_undo(&self) -> bool {
        self.done > 0
    }

    pub fn can_redo(&self) -> bool {
        self.done < self.ops.len()
    }

    /// Returns whether the Ink changed.
    pub fn undo(&mut self, ink: &mut Ink, scratch: &mut Scratch) -> bool {
        let Some(index) = self.done.checked_sub(1) else {
            return false;
        };
        let Some(op) = self.ops.get_mut(index) else {
            return false;
        };
        let changed = flip(op, ink, scratch, Direction::Revert);
        self.done = index;
        changed
    }

    /// Returns whether the Ink changed.
    pub fn redo(&mut self, ink: &mut Ink, scratch: &mut Scratch) -> bool {
        let Some(op) = self.ops.get_mut(self.done) else {
            return false;
        };
        let changed = flip(op, ink, scratch, Direction::Apply);
        self.done += 1;
        changed
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Apply,
    Revert,
}

fn flip(op: &mut Op, ink: &mut Ink, scratch: &mut Scratch, direction: Direction) -> bool {
    match (op, direction) {
        (Op::Add(slot), Direction::Revert) => {
            *slot = ink.pop();
            slot.is_some()
        }
        (Op::Add(slot), Direction::Apply) => match slot.take() {
            Some(element) => {
                ink.add(element);
                true
            }
            None => false,
        },
        (Op::Erase(slots), Direction::Revert) => {
            for (index, slot) in slots.iter_mut() {
                if let Some(element) = slot.take() {
                    ink.insert_at(*index, element);
                }
            }
            true
        }
        (Op::Erase(slots), Direction::Apply) => {
            for (index, slot) in slots.iter_mut().rev() {
                *slot = ink.remove_at(*index);
            }
            true
        }
        // Applying and reverting a Clear are the same swap.
        (Op::Clear(payload), _) => {
            ink.swap_elements(payload);
            true
        }
        (Op::Restyle(styles), _) => {
            for (index, style) in styles.iter_mut() {
                if let Some(element) = ink.get_mut(*index) {
                    let current = element.style();
                    element.set_style(*style, scratch);
                    *style = current;
                }
            }
            !styles.is_empty()
        }
        (Op::Move(positions), _) => {
            for (index, x, y) in positions.iter_mut() {
                if let Some(element) = ink.get_mut(*index) {
                    let (ex, ey) = (element.x(), element.y());
                    element.set_position(*x, *y);
                    (*x, *y) = (ex, ey);
                }
            }
            !positions.is_empty()
        }
        (Op::Delete { indices, removed }, Direction::Revert) => {
            for (&index, element) in indices.iter().zip(removed.drain(..)) {
                ink.insert_at(index, element);
            }
            !indices.is_empty()
        }
        (Op::Delete { indices, removed }, Direction::Apply) => {
            for &index in indices.iter().rev() {
                removed.extend(ink.remove_at(index));
            }
            removed.reverse();
            !indices.is_empty()
        }
    }
}
