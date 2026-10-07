//! The operation log behind undo and redo.
//!
//! Operations, not snapshots (standards rule 8): each entry holds only what
//! it changed. Applying or reverting an entry swaps its payload with the Ink,
//! so nothing is cloned and a Stroke's points exist exactly once.

use crate::ink::{Element, Ink};

#[derive(Debug)]
enum Op {
    /// One committed Stroke. Applied: the Element is the last one in the Ink
    /// and the payload is `None`. Reverted: the payload holds it.
    Add(Option<Element>),
    /// Clear. Applied: the payload holds what was removed and the Ink is
    /// empty. Reverted: the payload is empty and the Ink holds them again.
    Clear(Vec<Element>),
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
    pub fn undo(&mut self, ink: &mut Ink) -> bool {
        let Some(index) = self.done.checked_sub(1) else {
            return false;
        };
        let changed = flip(&mut self.ops[index], ink, Direction::Revert);
        self.done = index;
        changed
    }

    /// Returns whether the Ink changed.
    pub fn redo(&mut self, ink: &mut Ink) -> bool {
        let Some(op) = self.ops.get_mut(self.done) else {
            return false;
        };
        let changed = flip(op, ink, Direction::Apply);
        self.done += 1;
        changed
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Apply,
    Revert,
}

fn flip(op: &mut Op, ink: &mut Ink, direction: Direction) -> bool {
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
        // Applying and reverting a Clear are the same swap.
        (Op::Clear(payload), _) => {
            ink.swap_elements(payload);
            true
        }
    }
}
